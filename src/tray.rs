use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};

use crate::config::{AppConfig, GlucoseThresholds};
use crate::icon::text_icon;
use crate::nightscout::CgmEntry;

const NORMAL_COLOR: [u8; 4] = [32, 122, 74, 255];
const WARNING_COLOR: [u8; 4] = [214, 155, 0, 255];
const CRITICAL_COLOR: [u8; 4] = [196, 46, 46, 255];
const INACTIVE_COLOR: [u8; 4] = [104, 112, 122, 255];

#[derive(Clone, Debug)]
enum ReadingState {
    Loading,
    Fresh(u16),
    Stale(Option<u16>),
    Unavailable,
}

impl ReadingState {
    fn icon_text(&self) -> String {
        match self {
            Self::Loading | Self::Unavailable | Self::Stale(None) => "--".to_string(),
            Self::Fresh(reading) | Self::Stale(Some(reading)) => reading.to_string(),
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Loading => "NightScout loading".to_string(),
            Self::Fresh(reading) => format!("NightScout {reading}"),
            Self::Stale(Some(reading)) => format!("NightScout {reading} (stale)"),
            Self::Stale(None) => "NightScout unavailable (stale)".to_string(),
            Self::Unavailable => "NightScout unavailable".to_string(),
        }
    }
}

pub struct SharedState {
    refresh_minutes: AtomicU64,
    recent_entries: Mutex<Vec<CgmEntry>>,
    last_error: Mutex<Option<String>>,
}

impl SharedState {
    pub fn new(refresh_minutes: u64) -> Self {
        Self {
            refresh_minutes: AtomicU64::new(refresh_minutes.max(1)),
            recent_entries: Mutex::new(Vec::new()),
            last_error: Mutex::new(None),
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

    pub fn record_entries(&self, entries: Vec<CgmEntry>) {
        if let Ok(mut buffered) = self.recent_entries.lock() {
            *buffered = entries;
        }

        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = None;
        }
    }

    pub fn record_error(&self, error: impl Into<String>) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(error.into());
        }
    }

    pub fn latest_entry(&self) -> Option<CgmEntry> {
        self.recent_entries
            .lock()
            .ok()
            .and_then(|entries| entries.first().cloned())
    }

    pub fn snapshot_entries(&self) -> Vec<CgmEntry> {
        self.recent_entries
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    fn buffered_entry_count(&self) -> usize {
        self.recent_entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    fn fetch_status_label(&self) -> String {
        match self.last_error.lock() {
            Ok(error) => match error.as_deref() {
                Some(message) => format!("Last fetch failed: {}", summarize(message, 36)),
                None if self.buffered_entry_count() > 0 => "Last fetch: ok".to_string(),
                None => "Last fetch: waiting for data".to_string(),
            },
            Err(_) => "Last fetch: unavailable".to_string(),
        }
    }

    fn tooltip_description(&self) -> String {
        let mut lines = Vec::new();

        if let Some(entry) = self.latest_entry() {
            lines.push(format!("SGV: {} mg/dL", entry.sgv));

            if let Some(direction) = entry.direction {
                lines.push(format!("Direction: {direction}"));
            }

            if let Some(date_string) = entry.date_string {
                lines.push(format!("Timestamp: {date_string}"));
            }
        } else {
            lines.push("No CGM data loaded yet".to_string());
        }

        match self.last_error.lock() {
            Ok(error) => {
                if let Some(message) = error.as_deref() {
                    lines.push(format!("Error: {}", summarize(message, 72)));
                }
            }
            Err(_) => lines.push("Error: could not read fetch status".to_string()),
        }

        lines.join("<br/>")
    }
}

pub enum AppCommand {
    OpenWebsite,
    OpenSettings,
    RefreshNow,
    ToggleChart,
    ToggleLaunchOnStartup,
    Quit,
}

pub struct NightscoutTray {
    reading_state: ReadingState,
    config: AppConfig,
    shared: Arc<SharedState>,
    command_sender: Sender<AppCommand>,
}

impl NightscoutTray {
    pub fn new(
        config: AppConfig,
        shared: Arc<SharedState>,
        command_sender: Sender<AppCommand>,
    ) -> Self {
        Self {
            reading_state: ReadingState::Loading,
            config,
            shared,
            command_sender,
        }
    }

    pub fn set_fresh_reading(&mut self, reading: u16) {
        self.reading_state = ReadingState::Fresh(reading);
    }

    pub fn mark_stale(&mut self) {
        self.reading_state = match self.reading_state {
            ReadingState::Fresh(reading) => ReadingState::Stale(Some(reading)),
            ReadingState::Stale(reading) => ReadingState::Stale(reading),
            ReadingState::Loading | ReadingState::Unavailable => ReadingState::Stale(None),
        };
    }

    pub fn show_unavailable(&mut self) {
        self.reading_state = ReadingState::Unavailable;
    }

    pub fn apply_config(&mut self, config: AppConfig) {
        self.config = config;
    }

    fn status_items(&self) -> Vec<MenuItem<Self>> {
        vec![
            action_item(self.url_status_label(), {
                let sender = self.command_sender.clone();
                move || {
                    let _ = sender.send(AppCommand::OpenWebsite);
                }
            })
            .into(),
            disabled_item(format!("Refresh every {} min", self.config.refresh_minutes)).into(),
            disabled_item(self.threshold_status_label()).into(),
            disabled_item(self.token_status_label()).into(),
            disabled_item(self.shared.fetch_status_label()).into(),
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

    fn threshold_status_label(&self) -> String {
        let thresholds = &self.config.thresholds;
        format!(
            "Thresholds L:{} / {} H:{} / {}",
            thresholds.low_critical,
            thresholds.low_warn,
            thresholds.high_warn,
            thresholds.high_critical
        )
    }

    fn icon_color(&self) -> [u8; 4] {
        match self.reading_state {
            ReadingState::Fresh(reading) => color_for_reading(reading, &self.config.thresholds),
            ReadingState::Loading | ReadingState::Stale(_) | ReadingState::Unavailable => {
                INACTIVE_COLOR
            }
        }
    }
}

impl ksni::Tray for NightscoutTray {
    fn id(&self) -> String {
        "nightscout-tray".to_string()
    }

    fn title(&self) -> String {
        self.reading_state.title()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![text_icon(
            &self.reading_state.icon_text(),
            self.icon_color(),
        )]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: String::new(),
            icon_pixmap: vec![text_icon(
                &self.reading_state.icon_text(),
                self.icon_color(),
            )],
            title: self.reading_state.title(),
            description: self.shared.tooltip_description(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let settings_sender = self.command_sender.clone();
        let refresh_sender = self.command_sender.clone();
        let startup_sender = self.command_sender.clone();
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
            checkmark_item(
                "Launch on startup",
                self.config.launch_on_startup,
                move || {
                    let _ = startup_sender.send(AppCommand::ToggleLaunchOnStartup);
                },
            )
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
        let _ = self.command_sender.send(AppCommand::ToggleChart);
    }
}

fn action_item<F, S>(label: S, action: F) -> StandardItem<NightscoutTray>
where
    F: Fn() + Send + 'static,
    S: Into<String>,
{
    StandardItem {
        label: label.into(),
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

fn checkmark_item<F>(label: &str, checked: bool, action: F) -> CheckmarkItem<NightscoutTray>
where
    F: Fn() + Send + 'static,
{
    CheckmarkItem {
        label: label.to_string(),
        checked,
        activate: Box::new(move |_tray: &mut NightscoutTray| action()),
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

pub(crate) fn color_for_reading(reading: u16, thresholds: &GlucoseThresholds) -> [u8; 4] {
    if reading < thresholds.low_critical || reading > thresholds.high_critical {
        CRITICAL_COLOR
    } else if reading < thresholds.low_warn || reading > thresholds.high_warn {
        WARNING_COLOR
    } else {
        NORMAL_COLOR
    }
}
