use std::io;
use std::process::Command;

use crate::config::AppConfig;

pub fn open_settings_dialog(current: &AppConfig) -> io::Result<Option<AppConfig>> {
    let Some(updated) = prompt_settings_form(current)? else {
        return Ok(None);
    };

    Ok(Some(
        AppConfig {
            nightscout_url: updated.nightscout_url,
            api_token: updated.api_token,
            refresh_minutes: updated.refresh_minutes,
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

#[derive(serde::Deserialize)]
struct SettingsDialogResult {
    nightscout_url: String,
    api_token: String,
    refresh_minutes: u64,
}

fn prompt_settings_form(current: &AppConfig) -> io::Result<Option<SettingsDialogResult>> {
    let output = Command::new("python3")
        .args([
            "-c",
            PYQT_SETTINGS_DIALOG,
            &current.nightscout_url,
            &current.api_token,
            &current.refresh_minutes.to_string(),
        ])
        .output()?;

    match output.status.code() {
        Some(0) => Ok(Some(
            serde_json::from_slice::<SettingsDialogResult>(&output.stdout).map_err(|error| {
                io::Error::other(format!("invalid settings dialog response: {error}"))
            })?,
        )),
        Some(1) => Ok(None),
        _ => Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        )),
    }
}

const PYQT_SETTINGS_DIALOG: &str = r#"
import json
import sys

from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import QApplication, QDialog, QDialogButtonBox, QFormLayout, QHBoxLayout, QLabel, QLineEdit, QSpinBox, QVBoxLayout, QWidget


class SettingsDialog(QDialog):
    def __init__(self, url: str, token: str, refresh_minutes: int) -> None:
        super().__init__()
        self.setWindowTitle('NightScout Settings')
        self.setMinimumWidth(420)

        self.url_input = QLineEdit(url)
        self.token_input = QLineEdit(token)
        self.refresh_input = QSpinBox()
        self.refresh_input.setRange(1, 120)
        self.refresh_input.setValue(max(1, refresh_minutes))
        self.refresh_input.setSuffix(' min')

        form = QFormLayout()
        form.addRow('NightScout URL', self.url_input)
        form.addRow('API token', self.token_input)
        form.addRow('Refresh frequency', self.refresh_input)

        help_label = QLabel('These values are saved to the NightScout Tray config file.')
        help_label.setWordWrap(True)
        help_label.setAlignment(Qt.AlignLeft | Qt.AlignTop)

        buttons = QDialogButtonBox(QDialogButtonBox.Save | QDialogButtonBox.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout()
        layout.addLayout(form)
        layout.addWidget(help_label)
        layout.addWidget(buttons)
        self.setLayout(layout)

    def result_payload(self) -> str:
        return json.dumps({
            'nightscout_url': self.url_input.text(),
            'api_token': self.token_input.text(),
            'refresh_minutes': self.refresh_input.value(),
        })


def main() -> int:
    app = QApplication(sys.argv)
    dialog = SettingsDialog(sys.argv[1], sys.argv[2], int(sys.argv[3]))
    if dialog.exec_() == QDialog.Accepted:
        print(dialog.result_payload())
        return 0
    return 1


if __name__ == '__main__':
    raise SystemExit(main())
"#;
