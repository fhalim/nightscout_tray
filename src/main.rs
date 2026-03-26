mod autostart;
mod config;
mod controller;
mod dialogs;
mod icon;
mod nightscout;
mod tray;

use std::error::Error;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ksni::blocking::TrayMethods;

use crate::config::{config_path, load_config};
use crate::controller::run_controller;
use crate::tray::{NightscoutTray, SharedState};

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = config_path()?;
    let config = load_config(&config_path)?;
    let shared = Arc::new(SharedState::new(config.refresh_minutes));
    let (command_sender, command_receiver) = mpsc::channel();

    let tray = NightscoutTray::new(config.clone(), Arc::clone(&shared), command_sender);
    let handle = tray.spawn()?;

    let controller_handle = handle.clone();
    thread::spawn(move || {
        run_controller(
            controller_handle,
            command_receiver,
            config_path,
            config,
            shared,
        );
    });

    while !handle.is_closed() {
        thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}
