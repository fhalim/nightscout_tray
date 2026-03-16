use std::io;
use std::process::Command;

use crate::config::AppConfig;

pub fn open_settings_dialog(current: &AppConfig) -> io::Result<Option<AppConfig>> {
    let mut draft = toml::to_string_pretty(current)
        .map_err(|error| io::Error::other(format!("toml serialization failed: {error}")))?;

    loop {
        let Some(value) = prompt_text_input(
            "NightScout Settings",
            "Edit the NightScout URL, API token, and refresh frequency, then click Save.",
            &draft,
        )?
        else {
            return Ok(None);
        };

        draft = value;

        match toml::from_str::<AppConfig>(&draft) {
            Ok(config) if config.refresh_minutes > 0 => return Ok(Some(config.normalized())),
            Ok(_) => {
                show_error_dialog("Refresh frequency must be a whole number greater than 0.");
            }
            Err(error) => {
                let message = format!(
                    "Settings must be valid TOML with `nightscout_url`, `api_token`, and `refresh_minutes`: {error}"
                );
                show_error_dialog(&message);
            }
        }
    }
}

pub fn show_error_dialog(message: &str) {
    let _ = Command::new("kdialog")
        .args(["--title", "NightScout Settings", "--error", message])
        .status();
}

fn prompt_text_input(title: &str, prompt: &str, initial_value: &str) -> io::Result<Option<String>> {
    let output = Command::new("kdialog")
        .args([
            "--title",
            title,
            "--ok-label",
            "Save",
            "--cancel-label",
            "Cancel",
            "--textinputbox",
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
