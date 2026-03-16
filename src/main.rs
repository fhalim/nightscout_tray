use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use directories::ProjectDirs;
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};
use serde::{Deserialize, Serialize};

const DEFAULT_REFRESH_MINUTES: u64 = 5;
const INITIAL_READING: u16 = 110;
const SAMPLE_READINGS: [u16; 6] = [110, 108, 112, 115, 109, 106];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct AppConfig {
    nightscout_url: String,
    refresh_minutes: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            nightscout_url: String::new(),
            refresh_minutes: DEFAULT_REFRESH_MINUTES,
        }
    }
}

impl AppConfig {
    fn normalized(mut self) -> Self {
        self.nightscout_url = self.nightscout_url.trim().to_string();
        self.refresh_minutes = self.refresh_minutes.max(1);
        self
    }
}

struct SharedState {
    refresh_minutes: AtomicU64,
    sample_index: AtomicUsize,
}

impl SharedState {
    fn new(refresh_minutes: u64) -> Self {
        Self {
            refresh_minutes: AtomicU64::new(refresh_minutes.max(1)),
            sample_index: AtomicUsize::new(0),
        }
    }
}

enum AppCommand {
    OpenSettings,
}

struct NightscoutTray {
    reading: u16,
    config: AppConfig,
    shared: Arc<SharedState>,
    command_sender: Sender<AppCommand>,
}

impl NightscoutTray {
    fn new(
        reading: u16,
        config: AppConfig,
        shared: Arc<SharedState>,
        command_sender: Sender<AppCommand>,
    ) -> Self {
        Self {
            reading,
            config,
            shared,
            command_sender,
        }
    }

    fn advance_sample(&mut self) {
        let index = self.shared.sample_index.fetch_add(1, Ordering::Relaxed);
        self.reading = SAMPLE_READINGS[index % SAMPLE_READINGS.len()];
    }

    fn apply_config(&mut self, config: AppConfig) {
        self.config = config;
    }

    fn url_status_label(&self) -> String {
        if self.config.nightscout_url.is_empty() {
            "NightScout URL: not configured".to_string()
        } else {
            format!(
                "NightScout URL: {}",
                summarize(&self.config.nightscout_url, 40)
            )
        }
    }
}

impl ksni::Tray for NightscoutTray {
    fn id(&self) -> String {
        "nightscout-tray".to_string()
    }

    fn title(&self) -> String {
        format!("NightScout {}", self.reading)
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![numeric_icon(self.reading)]
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let settings_sender = self.command_sender.clone();

        vec![
            StandardItem {
                label: "Refresh now".to_string(),
                activate: Box::new(|tray: &mut NightscoutTray| tray.advance_sample()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Settings...".to_string(),
                activate: Box::new(move |_tray: &mut NightscoutTray| {
                    let _ = settings_sender.send(AppCommand::OpenSettings);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: self.url_status_label(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("Refresh every {} min", self.config.refresh_minutes),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.advance_sample();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config_path = config_path()?;
    let config = load_config(&config_path)?;
    let shared = Arc::new(SharedState::new(config.refresh_minutes));
    let (command_sender, command_receiver) = mpsc::channel();

    let tray = NightscoutTray::new(
        INITIAL_READING,
        config.clone(),
        Arc::clone(&shared),
        command_sender,
    );
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

fn run_controller(
    handle: ksni::blocking::Handle<NightscoutTray>,
    receiver: mpsc::Receiver<AppCommand>,
    config_path: PathBuf,
    mut config: AppConfig,
    shared: Arc<SharedState>,
) {
    loop {
        if handle.is_closed() {
            break;
        }

        let timeout = Duration::from_secs(
            shared
                .refresh_minutes
                .load(Ordering::Relaxed)
                .max(1)
                .saturating_mul(60),
        );

        match receiver.recv_timeout(timeout) {
            Ok(AppCommand::OpenSettings) => match open_settings_dialog(&config) {
                Ok(Some(updated)) => {
                    if let Err(error) = save_config(&config_path, &updated) {
                        let message = format!(
                            "Could not save settings to {}: {error}",
                            config_path.display()
                        );
                        eprintln!("{message}");
                        show_error_dialog(&message);
                        continue;
                    }

                    shared
                        .refresh_minutes
                        .store(updated.refresh_minutes, Ordering::Relaxed);
                    config = updated.clone();

                    if handle
                        .update(move |tray| {
                            tray.apply_config(updated);
                        })
                        .is_none()
                    {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let message = format!("Could not open settings dialog: {error}");
                    eprintln!("{message}");
                    show_error_dialog(&message);
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                if handle.update(|tray| tray.advance_sample()).is_none() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn config_path() -> io::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "nightscout_tray").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine the XDG config directory",
        )
    })?;

    Ok(project_dirs.config_dir().join("config.toml"))
}

fn load_config(path: &Path) -> Result<AppConfig, Box<dyn Error>> {
    match fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<AppConfig>(&contents) {
            Ok(config) => Ok(config.normalized()),
            Err(error) => {
                eprintln!(
                    "Could not parse {}: {error}. Falling back to defaults.",
                    path.display()
                );
                Ok(AppConfig::default())
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(error) => Err(Box::new(error)),
    }
}

fn save_config(path: &Path, config: &AppConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)
        .map_err(|error| io::Error::other(format!("toml serialization failed: {error}")))?;

    fs::write(path, contents)
}

fn open_settings_dialog(current: &AppConfig) -> io::Result<Option<AppConfig>> {
    let nightscout_url = match prompt_input(
        "NightScout Settings",
        "NightScout URL",
        &current.nightscout_url,
    )? {
        Some(value) => value,
        None => return Ok(None),
    };

    let refresh_minutes = loop {
        let Some(value) = prompt_input(
            "NightScout Settings",
            "Refresh frequency in minutes",
            &current.refresh_minutes.to_string(),
        )?
        else {
            return Ok(None);
        };

        match value.trim().parse::<u64>() {
            Ok(minutes) if minutes > 0 => break minutes,
            _ => {
                show_error_dialog("Refresh frequency must be a whole number greater than 0.");
            }
        }
    };

    Ok(Some(
        AppConfig {
            nightscout_url,
            refresh_minutes,
        }
        .normalized(),
    ))
}

fn prompt_input(title: &str, prompt: &str, initial_value: &str) -> io::Result<Option<String>> {
    let output = Command::new("kdialog")
        .args(["--title", title, "--inputbox", prompt, initial_value])
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

fn show_error_dialog(message: &str) {
    let _ = Command::new("kdialog")
        .args(["--title", "NightScout Settings", "--error", message])
        .status();
}

fn summarize(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let mut shortened = value
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    shortened.push_str("...");
    shortened
}

fn numeric_icon(reading: u16) -> ksni::Icon {
    const SIZE: usize = 32;
    const SCALE: usize = 2;
    const DIGIT_WIDTH: usize = 3;
    const DIGIT_HEIGHT: usize = 5;
    const DIGIT_SPACING: usize = 1;

    let text = reading.to_string();
    let digit_count = text.len();
    let text_width =
        digit_count * DIGIT_WIDTH * SCALE + digit_count.saturating_sub(1) * DIGIT_SPACING * SCALE;
    let text_height = DIGIT_HEIGHT * SCALE;
    let offset_x = (SIZE - text_width) / 2;
    let offset_y = (SIZE - text_height) / 2;

    let mut rgba = vec![0_u8; SIZE * SIZE * 4];

    fill_rect(&mut rgba, SIZE, 0, 0, SIZE, SIZE, [32, 122, 74, 255]);
    fill_rect(
        &mut rgba,
        SIZE,
        2,
        2,
        SIZE - 4,
        SIZE - 4,
        [238, 248, 241, 255],
    );

    for (index, ch) in text.chars().enumerate() {
        let digit_x = offset_x + index * (DIGIT_WIDTH + DIGIT_SPACING) * SCALE;
        draw_digit(
            &mut rgba,
            SIZE,
            ch,
            digit_x,
            offset_y,
            SCALE,
            [23, 69, 44, 255],
        );
    }

    rgba_to_argb(&mut rgba);

    ksni::Icon {
        width: SIZE as i32,
        height: SIZE as i32,
        data: rgba,
    }
}

fn fill_rect(
    rgba: &mut [u8],
    canvas_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    for row in y..(y + height) {
        for column in x..(x + width) {
            let pixel = (row * canvas_width + column) * 4;
            rgba[pixel..pixel + 4].copy_from_slice(&color);
        }
    }
}

fn draw_digit(
    rgba: &mut [u8],
    canvas_width: usize,
    ch: char,
    start_x: usize,
    start_y: usize,
    scale: usize,
    color: [u8; 4],
) {
    let pattern = match digit_pattern(ch) {
        Some(pattern) => pattern,
        None => return,
    };

    for (row, row_pattern) in pattern.iter().enumerate() {
        for (column, pixel) in row_pattern.chars().enumerate() {
            if pixel == '1' {
                fill_rect(
                    rgba,
                    canvas_width,
                    start_x + column * scale,
                    start_y + row * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }
}

fn digit_pattern(ch: char) -> Option<[&'static str; 5]> {
    match ch {
        '0' => Some(["111", "101", "101", "101", "111"]),
        '1' => Some(["010", "110", "010", "010", "111"]),
        '2' => Some(["111", "001", "111", "100", "111"]),
        '3' => Some(["111", "001", "111", "001", "111"]),
        '4' => Some(["101", "101", "111", "001", "001"]),
        '5' => Some(["111", "100", "111", "001", "111"]),
        '6' => Some(["111", "100", "111", "101", "111"]),
        '7' => Some(["111", "001", "001", "001", "001"]),
        '8' => Some(["111", "101", "111", "101", "111"]),
        '9' => Some(["111", "101", "111", "001", "111"]),
        _ => None,
    }
}

fn rgba_to_argb(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
}
