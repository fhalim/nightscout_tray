use std::error::Error;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::config::AppConfig;

pub const READINGS_BUFFER_SIZE: usize = 10;

#[derive(Clone, Debug, Deserialize)]
pub struct CgmEntry {
    pub sgv: u16,
}

pub fn fetch_recent_entries(config: &AppConfig) -> Result<Vec<CgmEntry>, Box<dyn Error>> {
    if config.nightscout_url.is_empty() {
        return Ok(Vec::new());
    }

    let endpoint = format!(
        "{}/api/v1/entries.json",
        config.nightscout_url.trim_end_matches('/')
    );

    let client = Client::builder().build()?;
    let request = client.get(endpoint);
    let request = if config.api_token.is_empty() {
        request
    } else {
        request.query(&[("token", config.api_token.as_str())])
    };

    let mut entries = request
        .send()?
        .error_for_status()?
        .json::<Vec<CgmEntry>>()?;
    entries.truncate(READINGS_BUFFER_SIZE);
    Ok(entries)
}
