use std::io;
use std::process::Command;

use crate::config::AppConfig;

pub fn open_settings_dialog(current: &AppConfig) -> io::Result<Option<AppConfig>> {
    let Some(nightscout_url) = prompt_input(
        "NightScout Settings",
        "NightScout base URL",
        &current.nightscout_url,
        "Next",
    )?
    else {
        return Ok(None);
    };

    let Some(api_token) = prompt_input(
        "NightScout Settings",
        "NightScout API token",
        &current.api_token,
        "Next",
    )?
    else {
        return Ok(None);
    };

    let Some(refresh_minutes) = prompt_slider(
        "NightScout Settings",
        "Refresh frequency in minutes",
        current.refresh_minutes,
        1,
        120,
        1,
        "Save",
    )?
    else {
        return Ok(None);
    };

    Ok(Some(
        AppConfig {
            nightscout_url,
            api_token,
            refresh_minutes,
            launch_on_startup: current.launch_on_startup,
        }
        .normalized(),
    ))
}

pub fn show_error_dialog(message: &str) {
    let _ = Command::new("kdialog")
        .args(["--title", "NightScout Settings", "--error", message])
        .status();
}

fn prompt_input(
    title: &str,
    prompt: &str,
    initial_value: &str,
    ok_label: &str,
) -> io::Result<Option<String>> {
    let output = Command::new("kdialog")
        .args([
            "--title",
            title,
            "--ok-label",
            ok_label,
            "--cancel-label",
            "Cancel",
            "--inputbox",
            prompt,
            initial_value,
        ])
        .output()?;

    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )),
        Some(1) => Ok(None),
        _ => Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
    }
}

fn prompt_slider(
    title: &str,
    prompt: &str,
    initial_value: u64,
    min: u64,
    max: u64,
    step: u64,
    ok_label: &str,
) -> io::Result<Option<u64>> {
    let initial_value = initial_value.clamp(min, max);
    let initial_value = initial_value.to_string();
    let min = min.to_string();
    let max = max.to_string();
    let step = step.to_string();

    let output = Command::new("kdialog")
        .args([
            "--title",
            title,
            "--ok-label",
            ok_label,
            "--cancel-label",
            "Cancel",
            "--default",
            &initial_value,
            "--slider",
            prompt,
            &min,
            &max,
            &step,
        ])
        .output()?;

    match output.status.code() {
        Some(0) => {
            let minutes = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u64>()
                .map_err(|error| io::Error::other(format!("invalid slider value: {error}")))?;
            Ok(Some(minutes))
        }
        Some(1) => Ok(None),
        _ => Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
    }
}
