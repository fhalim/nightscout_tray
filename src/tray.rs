use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;

use ksni::menu::{MenuItem, StandardItem};

use crate::config::AppConfig;
use crate::icon::numeric_icon;
use crate::nightscout::CgmEntry;

pub struct SharedState {
    refresh_minutes: AtomicU64,
    recent_entries: Mutex<Vec<CgmEntry>>,
}

impl SharedState {
    pub fn new(refresh_minutes: u64) -> Self {
        Self {
            refresh_minutes: AtomicU64::new(refresh_minutes.max(1)),
            recent_entries: Mutex::new(Vec::new()),
        }
    }

    pub fn refresh_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.refresh_minutes
                .load(Ordering::Relaxed)
                .max(1)
                .saturating_mul(60),
        )
    }

    pub fn set_refresh_minutes(&self, refresh_minutes: u64) {
        self.refresh_minutes
            .store(refresh_minutes.max(1), Ordering::Relaxed);
    }

    pub fn replace_entries(&self, entries: Vec<CgmEntry>) {
        if let Ok(mut buffered) = self.recent_entries.lock() {
            *buffered = entries;
        }
    }

    fn buffered_entry_count(&self) -> usize {
        self.recent_entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }
}

pub enum AppCommand {
    OpenSettings,
    RefreshNow,
    Quit,
}

pub struct NightscoutTray {
    reading: u16,
    config: AppConfig,
    shared: Arc<SharedState>,
    command_sender: Sender<AppCommand>,
}

impl NightscoutTray {
    pub fn new(
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

    pub fn set_reading(&mut self, reading: u16) {
        self.reading = reading;
    }

    pub fn apply_config(&mut self, config: AppConfig) {
        self.config = config;
    }

    fn status_items(&self) -> Vec<MenuItem<Self>> {
        vec![
            disabled_item(self.url_status_label()).into(),
            disabled_item(format!("Refresh every {} min", self.config.refresh_minutes)).into(),
            disabled_item(self.token_status_label()).into(),
            disabled_item(format!(
                "Buffered entries: {}",
                self.shared.buffered_entry_count()
            ))
            .into(),
        ]
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

    fn token_status_label(&self) -> String {
        if self.config.api_token.is_empty() {
            "API token: not configured".to_string()
        } else {
            "API token: configured".to_string()
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
        let refresh_sender = self.command_sender.clone();
        let quit_sender = self.command_sender.clone();
        let mut items = vec![
            action_item("Refresh now", move || {
                let _ = refresh_sender.send(AppCommand::RefreshNow);
            })
            .into(),
            action_item("Settings...", move || {
                let _ = settings_sender.send(AppCommand::OpenSettings);
            })
            .into(),
            MenuItem::Separator,
        ];

        items.extend(self.status_items());
        items.push(MenuItem::Separator);
        items.push(
            action_item("Quit", move || {
                let _ = quit_sender.send(AppCommand::Quit);
            })
            .into(),
        );
        items
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.command_sender.send(AppCommand::RefreshNow);
    }
}

fn action_item<F>(label: &str, action: F) -> StandardItem<NightscoutTray>
where
    F: Fn() + Send + 'static,
{
    StandardItem {
        label: label.to_string(),
        activate: Box::new(move |_tray: &mut NightscoutTray| action()),
        ..Default::default()
    }
}

fn disabled_item(label: String) -> StandardItem<NightscoutTray> {
    StandardItem {
        label,
        enabled: false,
        ..Default::default()
    }
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
