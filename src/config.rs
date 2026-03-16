use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const DEFAULT_REFRESH_MINUTES: u64 = 5;
pub const DEFAULT_NIGHTSCOUT_URL: &str = "http://localhost:1337";
pub const DEFAULT_API_TOKEN: &str = "mysecrettoken";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub nightscout_url: String,
    pub api_token: String,
    pub refresh_minutes: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            nightscout_url: DEFAULT_NIGHTSCOUT_URL.to_string(),
            api_token: DEFAULT_API_TOKEN.to_string(),
            refresh_minutes: DEFAULT_REFRESH_MINUTES,
        }
    }
}

impl AppConfig {
    pub fn normalized(mut self) -> Self {
        self.nightscout_url = self.nightscout_url.trim().to_string();
        self.api_token = self.api_token.trim().to_string();
        self.refresh_minutes = self.refresh_minutes.max(1);
        self
    }
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

pub fn save_config(path: &Path, config: &AppConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(config)
        .map_err(|error| io::Error::other(format!("toml serialization failed: {error}")))?;

    fs::write(path, contents)
}
