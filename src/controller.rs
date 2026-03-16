use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;

use crate::config::{save_config, AppConfig};
use crate::dialogs::{open_settings_dialog, show_error_dialog};
use crate::nightscout::fetch_recent_entries;
use crate::tray::{AppCommand, NightscoutTray, SharedState};

pub fn run_controller(
    handle: ksni::blocking::Handle<NightscoutTray>,
    receiver: mpsc::Receiver<AppCommand>,
    config_path: PathBuf,
    mut config: AppConfig,
    shared: Arc<SharedState>,
) {
    if refresh_from_nightscout(&handle, &config, &shared).is_none() {
        return;
    }

    loop {
        if handle.is_closed() {
            break;
        }

        match receiver.recv_timeout(shared.refresh_timeout()) {
            Ok(AppCommand::OpenSettings) => {
                if !handle_settings(&handle, &config_path, &mut config, &shared) {
                    break;
                }
            }
            Ok(AppCommand::RefreshNow) => {
                if refresh_from_nightscout(&handle, &config, &shared).is_none() {
                    break;
                }
            }
            Ok(AppCommand::Quit) => {
                handle.shutdown();
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if refresh_from_nightscout(&handle, &config, &shared).is_none() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_settings(
    handle: &ksni::blocking::Handle<NightscoutTray>,
    config_path: &PathBuf,
    config: &mut AppConfig,
    shared: &Arc<SharedState>,
) -> bool {
    match open_settings_dialog(config) {
        Ok(Some(updated)) => {
            if let Err(error) = save_config(config_path, &updated) {
                let message = format!(
                    "Could not save settings to {}: {error}",
                    config_path.display()
                );
                eprintln!("{message}");
                show_error_dialog(&message);
                return true;
            }

            shared.set_refresh_minutes(updated.refresh_minutes);
            *config = updated.clone();

            if handle
                .update(move |tray| tray.apply_config(updated))
                .is_none()
            {
                return false;
            }

            refresh_from_nightscout(handle, config, shared).is_some()
        }
        Ok(None) => true,
        Err(error) => {
            let message = format!("Could not open settings dialog: {error}");
            eprintln!("{message}");
            show_error_dialog(&message);
            true
        }
    }
}

fn refresh_from_nightscout(
    handle: &ksni::blocking::Handle<NightscoutTray>,
    config: &AppConfig,
    shared: &Arc<SharedState>,
) -> Option<()> {
    match fetch_recent_entries(config) {
        Ok(entries) => {
            let reading = entries.first().map(|entry| entry.sgv);
            shared.replace_entries(entries);

            if let Some(reading) = reading {
                handle.update(move |tray| tray.set_reading(reading))?;
            } else {
                handle.update(|_tray| {})?;
            }

            Some(())
        }
        Err(error) => {
            eprintln!("NightScout refresh failed: {error}");
            handle.update(|_tray| {})?;
            Some(())
        }
    }
}
