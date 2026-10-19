use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::fingerprint::Fingerprint;
use crate::http::{self, ACOUSTID};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AcoustIdResponse {
    pub status: String,
    #[serde(default)]
    pub results: Vec<AcoustIdResult>,
    pub error: Option<AcoustIdError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AcoustIdError {
    pub code: Option<i32>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AcoustIdResult {
    pub id: String,
    pub score: f64,
    #[serde(default)]
    pub recordings: Vec<AidRecording>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidArtist {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub joinphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidRecording {
    pub id: String,
    pub title: Option<String>,
    /// Seconds
    pub duration: Option<f64>,
    #[serde(default)]
    pub artists: Vec<AidArtist>,
    #[serde(default)]
    pub releasegroups: Vec<AidReleaseGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidReleaseGroup {
    pub id: String,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub secondarytypes: Vec<String>,
    #[serde(default)]
    pub artists: Vec<AidArtist>,
    #[serde(default)]
    pub releases: Vec<AidRelease>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl AidDate {
    /// Sortable key, unknown parts sort last
    pub fn key(&self) -> (i32, u32, u32) {
        (self.year.unwrap_or(9999), self.month.unwrap_or(13), self.day.unwrap_or(32))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidRelease {
    pub id: String,
    pub title: Option<String>,
    pub country: Option<String>,
    pub date: Option<AidDate>,
    pub track_count: Option<u32>,
    pub medium_count: Option<u32>,
    #[serde(default)]
    pub mediums: Vec<AidMedium>,
    #[serde(default)]
    pub releaseevents: Vec<AidReleaseEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidReleaseEvent {
    pub country: Option<String>,
    pub date: Option<AidDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidMedium {
    pub position: Option<u32>,
    pub format: Option<String>,
    pub track_count: Option<u32>,
    #[serde(default)]
    pub tracks: Vec<AidTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AidTrack {
    pub id: String,
    pub position: Option<u32>,
    pub title: Option<String>,
    #[serde(default)]
    pub artists: Vec<AidArtist>,
}

pub fn artist_credit(artists: &[AidArtist]) -> String {
    let mut out = String::new();
    for (i, a) in artists.iter().enumerate() {
        out.push_str(&a.name);
        match &a.joinphrase {
            Some(j) => out.push_str(j),
            None if i + 1 < artists.len() => out.push_str(", "),
            None => {}
        }
    }
    out
}

/// Look up a fingerprint. Results are cached on disk per fingerprint.
pub fn lookup(api_key: &str, fp: &Fingerprint) -> Result<AcoustIdResponse, Error> {
    let cache_file = http::cache_dir("acoustid").join(format!("{:016x}-{}", http::fnv64(fp.encoded.as_bytes()), fp.duration));
    if let Ok(data) = std::fs::read(&cache_file) {
        if let Ok(r) = serde_json::from_slice(&data) {
            return Ok(r);
        }
    }

    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let mut backoff = Duration::from_secs(3);
    for _ in 0..6 {
        ACOUSTID.wait();
        let response = client.post("https://api.acoustid.org/v2/lookup")
            .form(&[
                ("format", "json"),
                ("client", api_key),
                ("duration", &fp.duration.to_string()),
                ("fingerprint", &fp.encoded),
                ("meta", "recordings releasegroups releases tracks compress"),
            ])
            .send();
        let response = match response {
            Ok(r) => r,
            Err(e) => {
                warn!("AcoustID request failed: {e}");
                ACOUSTID.cooldown(backoff);
                backoff *= 2;
                continue;
            }
        };
        let status = response.status();
        if status.as_u16() == 429 || status.is_server_error() {
            warn!("AcoustID {status}, pausing {}s", backoff.as_secs());
            ACOUSTID.cooldown(backoff);
            backoff *= 2;
            continue;
        }
        let text = response.text()?;
        let parsed: AcoustIdResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Invalid AcoustID response ({e}): {}", text.chars().take(200).collect::<String>()))?;
        if parsed.status != "ok" {
            let msg = parsed.error.as_ref().and_then(|e| e.message.clone()).unwrap_or_default();
            return Err(anyhow!("AcoustID error: {msg}"));
        }
        std::fs::write(&cache_file, &text).ok();
        return Ok(parsed);
    }
    Err(anyhow!("AcoustID unavailable after retries"))
}
