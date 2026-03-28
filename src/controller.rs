use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};

use crate::autostart::sync_autostart;
use crate::config::{AppConfig, save_config};
use crate::dialogs::{open_settings_dialog, show_error_dialog, toggle_chart_dialog};
use crate::nightscout::fetch_recent_entries;
use crate::tray::{AppCommand, NightscoutTray, SharedState};

pub fn run_controller(
    handle: ksni::blocking::Handle<NightscoutTray>,
    receiver: mpsc::Receiver<AppCommand>,
    config_path: PathBuf,
    mut config: AppConfig,
    shared: Arc<SharedState>,
) {
    if let Err(error) = sync_autostart(&config) {
        eprintln!("Could not sync KDE startup integration: {error}");
    }

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
            Ok(AppCommand::OpenWebsite) => {
                handle_open_website(&config);
            }
            Ok(AppCommand::RefreshNow) => {
                if refresh_from_nightscout(&handle, &config, &shared).is_none() {
                    break;
                }
            }
            Ok(AppCommand::ToggleChart) => {
                handle_toggle_chart(&config, &shared);
            }
            Ok(AppCommand::ToggleLaunchOnStartup) => {
                if !handle_startup_toggle(&handle, &config_path, &mut config) {
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

fn handle_startup_toggle(
    handle: &ksni::blocking::Handle<NightscoutTray>,
    config_path: &Path,
    config: &mut AppConfig,
) -> bool {
    let mut updated = config.clone();
    updated.launch_on_startup = !updated.launch_on_startup;

    if let Err(error) = sync_autostart(&updated) {
        let message = format!("Could not update KDE startup integration: {error}");
        eprintln!("{message}");
        show_error_dialog(&message);
        return true;
    }

    if let Err(error) = save_config(config_path, &updated) {
        let message = format!(
            "Could not save settings to {}: {error}",
            config_path.display()
        );
        eprintln!("{message}");
        show_error_dialog(&message);
        return true;
    }

    *config = updated.clone();

    handle
        .update(move |tray| tray.apply_config(updated))
        .is_some()
}

fn handle_settings(
    handle: &ksni::blocking::Handle<NightscoutTray>,
    config_path: &Path,
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

fn handle_open_website(config: &AppConfig) {
    if let Err(error) = open_nightscout_website(config) {
        let message = format!("Could not open NightScout website: {error}");
        eprintln!("{message}");
        show_error_dialog(&message);
    }
}

fn handle_toggle_chart(config: &AppConfig, shared: &Arc<SharedState>) {
    if let Err(error) = toggle_chart_dialog(shared.snapshot_entries(), config.thresholds.clone()) {
        let message = format!("Could not open NightScout chart: {error}");
        eprintln!("{message}");
        show_error_dialog(&message);
    }
}

fn open_nightscout_website(config: &AppConfig) -> Result<(), std::io::Error> {
    let url = nightscout_website_url(config)?;
    Command::new("xdg-open").arg(url.as_str()).spawn()?;
    Ok(())
}

fn nightscout_website_url(config: &AppConfig) -> Result<reqwest::Url, std::io::Error> {
    if config.nightscout_url.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "NightScout URL is not configured",
        ));
    }

    let mut url = reqwest::Url::parse(&config.nightscout_url).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid NightScout URL: {error}"),
        )
    })?;

    if config.api_token.is_empty() {
        return Ok(url);
    }

    let existing_pairs = url
        .query_pairs()
        .into_owned()
        .filter(|(key, _)| key != "token")
        .collect::<Vec<_>>();

    {
        let mut pairs = url.query_pairs_mut();
        pairs.clear();
        pairs.extend_pairs(existing_pairs.iter().map(|(key, value)| (&**key, &**value)));
        pairs.append_pair("token", &config.api_token);
    }

    Ok(url)
}

fn refresh_from_nightscout(
    handle: &ksni::blocking::Handle<NightscoutTray>,
    config: &AppConfig,
    shared: &Arc<SharedState>,
) -> Option<()> {
    match fetch_recent_entries(config) {
        Ok(entries) => {
            let reading = entries.first().map(|entry| entry.sgv);
            shared.record_entries(entries);

            if let Some(reading) = reading {
                handle.update(move |tray| tray.set_fresh_reading(reading))?;
            } else {
                handle.update(|tray| tray.show_unavailable())?;
            }

            Some(())
        }
        Err(error) => {
            eprintln!("NightScout refresh failed: {error}");
            shared.record_error(error.to_string());
            handle.update(|tray| tray.mark_stale())?;
            Some(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::nightscout_website_url;
    use crate::config::{AppConfig, GlucoseThresholds};

    fn config(url: &str, token: &str) -> AppConfig {
        AppConfig {
            nightscout_url: url.to_string(),
            api_token: token.to_string(),
            refresh_minutes: 5,
            launch_on_startup: false,
            thresholds: GlucoseThresholds::default(),
        }
    }

    #[test]
    fn website_url_appends_token_query_parameter() {
        let url = nightscout_website_url(&config("https://example.test", "secret token"))
            .expect("url should build");

        assert_eq!(url.as_str(), "https://example.test/?token=secret+token");
    }

    #[test]
    fn website_url_replaces_existing_token_query_parameter() {
        let url = nightscout_website_url(&config(
            "https://example.test/view?foo=bar&token=old",
            "new",
        ))
        .expect("url should build");

        assert_eq!(url.as_str(), "https://example.test/view?foo=bar&token=new");
    }
}
