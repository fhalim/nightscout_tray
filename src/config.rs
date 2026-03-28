use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const DEFAULT_REFRESH_MINUTES: u64 = 5;
pub const DEFAULT_NIGHTSCOUT_URL: &str = "http://localhost:1337";
pub const DEFAULT_API_TOKEN: &str = "mysecrettoken";
pub const DEFAULT_LOW_WARN: u16 = 70;
pub const DEFAULT_LOW_CRITICAL: u16 = 50;
pub const DEFAULT_HIGH_WARN: u16 = 200;
pub const DEFAULT_HIGH_CRITICAL: u16 = 300;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct GlucoseThresholds {
    pub low_warn: u16,
    pub low_critical: u16,
    pub high_warn: u16,
    pub high_critical: u16,
}

impl Default for GlucoseThresholds {
    fn default() -> Self {
        Self {
            low_warn: DEFAULT_LOW_WARN,
            low_critical: DEFAULT_LOW_CRITICAL,
            high_warn: DEFAULT_HIGH_WARN,
            high_critical: DEFAULT_HIGH_CRITICAL,
        }
    }
}

impl GlucoseThresholds {
    pub fn normalized(mut self) -> Self {
        self.low_warn = self.low_warn.max(self.low_critical);
        self.high_warn = self.high_warn.min(self.high_critical);
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub nightscout_url: String,
    pub api_token: String,
    pub refresh_minutes: u64,
    pub launch_on_startup: bool,
    pub thresholds: GlucoseThresholds,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            nightscout_url: DEFAULT_NIGHTSCOUT_URL.to_string(),
            api_token: DEFAULT_API_TOKEN.to_string(),
            refresh_minutes: DEFAULT_REFRESH_MINUTES,
            launch_on_startup: false,
            thresholds: GlucoseThresholds::default(),
        }
    }
}

impl AppConfig {
    pub fn normalized(mut self) -> Self {
        self.nightscout_url = self.nightscout_url.trim().to_string();
        self.api_token = self.api_token.trim().to_string();
        self.refresh_minutes = self.refresh_minutes.max(1);
        self.thresholds = self.thresholds.normalized();
        self
    }
}

pub fn parse_config(contents: &str) -> Result<AppConfig, toml::de::Error> {
    toml::from_str::<AppConfig>(contents).map(AppConfig::normalized)
}

pub fn config_path() -> io::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "nightscout_tray").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine the XDG config directory",
        )
    })?;

    Ok(project_dirs.config_dir().join("config.toml"))
}

pub fn load_config(path: &Path) -> Result<AppConfig, Box<dyn Error>> {
    match fs::read_to_string(path) {
        Ok(contents) => match parse_config(&contents) {
            Ok(config) => Ok(config),
            Err(error) => Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("could not parse {}: {error}", path.display()),
            ))),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(error) => Err(Box::new(error)),
    }
}

pub fn save_config(path: &Path, config: &AppConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)
        .map_err(|error| io::Error::other(format!("toml serialization failed: {error}")))?;

    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, DEFAULT_API_TOKEN, DEFAULT_HIGH_CRITICAL, DEFAULT_HIGH_WARN,
        DEFAULT_LOW_CRITICAL, DEFAULT_LOW_WARN, DEFAULT_NIGHTSCOUT_URL, DEFAULT_REFRESH_MINUTES,
        GlucoseThresholds, load_config, parse_config,
    };

    #[test]
    fn parse_config_uses_defaults_for_missing_fields() {
        let config = parse_config("").expect("config should parse");

        assert_eq!(config.nightscout_url, DEFAULT_NIGHTSCOUT_URL);
        assert_eq!(config.api_token, DEFAULT_API_TOKEN);
        assert_eq!(config.refresh_minutes, DEFAULT_REFRESH_MINUTES);
        assert!(!config.launch_on_startup);
        assert_eq!(config.thresholds.low_warn, DEFAULT_LOW_WARN);
        assert_eq!(config.thresholds.low_critical, DEFAULT_LOW_CRITICAL);
        assert_eq!(config.thresholds.high_warn, DEFAULT_HIGH_WARN);
        assert_eq!(config.thresholds.high_critical, DEFAULT_HIGH_CRITICAL);
    }

    #[test]
    fn parse_config_normalizes_whitespace_and_refresh_value() {
        let config = parse_config(
            r#"
nightscout_url = "  http://example.test  "
api_token = "  secret-token  "
refresh_minutes = 0
launch_on_startup = true
thresholds.low_warn = 75
thresholds.low_critical = 55
thresholds.high_warn = 190
thresholds.high_critical = 275
"#,
        )
        .expect("config should parse");

        assert_eq!(
            config,
            AppConfig {
                nightscout_url: "http://example.test".to_string(),
                api_token: "secret-token".to_string(),
                refresh_minutes: 1,
                launch_on_startup: true,
                thresholds: GlucoseThresholds {
                    low_warn: 75,
                    low_critical: 55,
                    high_warn: 190,
                    high_critical: 275,
                },
            }
        );
    }

    #[test]
    fn parse_config_normalizes_threshold_order() {
        let config = parse_config(
            r#"
thresholds.low_warn = 40
thresholds.low_critical = 50
thresholds.high_warn = 350
thresholds.high_critical = 300
"#,
        )
        .expect("config should parse");

        assert_eq!(config.thresholds.low_warn, 50);
        assert_eq!(config.thresholds.low_critical, 50);
        assert_eq!(config.thresholds.high_warn, 300);
        assert_eq!(config.thresholds.high_critical, 300);
    }

    #[test]
    fn load_config_returns_error_for_invalid_toml() {
        let path = std::env::temp_dir().join(format!(
            "nightscout-tray-invalid-config-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "nightscout_url = [").expect("temp config should be written");

        let error = load_config(&path).expect_err("invalid config should fail to load");
        let _ = std::fs::remove_file(&path);

        assert!(error.to_string().contains("could not parse"));
    }
}
