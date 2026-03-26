use std::error::Error;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::config::AppConfig;

pub const READINGS_BUFFER_SIZE: usize = 10;
const CONNECT_TIMEOUT_SECONDS: u64 = 5;
const REQUEST_TIMEOUT_SECONDS: u64 = 15;

#[derive(Clone, Debug, Deserialize)]
pub struct CgmEntry {
    pub sgv: u16,
    #[serde(rename = "dateString")]
    pub date_string: Option<String>,
    pub direction: Option<String>,
}

pub fn fetch_recent_entries(config: &AppConfig) -> Result<Vec<CgmEntry>, Box<dyn Error>> {
    if config.nightscout_url.is_empty() {
        return Ok(Vec::new());
    }

    let endpoint = format!(
        "{}/api/v1/entries.json",
        config.nightscout_url.trim_end_matches('/')
    );

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .build()?;
    let request = client.get(endpoint);
    let request = if config.api_token.is_empty() {
        request
    } else {
        request.query(&[("token", config.api_token.as_str())])
    };

    let body = request.send()?.error_for_status()?.text()?;
    Ok(parse_entries(&body)?)
}

pub fn parse_entries(body: &str) -> Result<Vec<CgmEntry>, serde_json::Error> {
    let mut entries = serde_json::from_str::<Vec<CgmEntry>>(body)?;
    entries.truncate(READINGS_BUFFER_SIZE);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{parse_entries, READINGS_BUFFER_SIZE};

    #[test]
    fn parse_entries_reads_latest_cgm_fields() {
        let entries = parse_entries(
            r#"[
                {
                    "sgv": 235,
                    "dateString": "2026-03-16T02:24:07.158Z",
                    "direction": "SingleDown"
                }
            ]"#,
        )
        .expect("entries should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sgv, 235);
        assert_eq!(
            entries[0].date_string.as_deref(),
            Some("2026-03-16T02:24:07.158Z")
        );
        assert_eq!(entries[0].direction.as_deref(), Some("SingleDown"));
    }

    #[test]
    fn parse_entries_keeps_only_latest_ten() {
        let body = format!(
            "[{}]",
            (0..12)
                .map(|index| format!(r#"{{"sgv": {}}}"#, 100 + index))
                .collect::<Vec<_>>()
                .join(",")
        );

        let entries = parse_entries(&body).expect("entries should parse");

        assert_eq!(entries.len(), READINGS_BUFFER_SIZE);
        assert_eq!(entries[0].sgv, 100);
        assert_eq!(entries[READINGS_BUFFER_SIZE - 1].sgv, 109);
    }
}
