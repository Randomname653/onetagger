use anyhow::Error;
use serde_json::Value;
use crate::http::{self, MUSICBRAINZ, COVERART};

/// ISRCs of a recording
pub fn recording_isrcs(id: &str) -> Result<Vec<String>, Error> {
    let url = format!("https://musicbrainz.org/ws/2/recording/{id}?inc=isrcs&fmt=json");
    let v: Option<Value> = http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))?;
    Ok(v.and_then(|v| v["isrcs"].as_array().cloned()).unwrap_or_default()
        .into_iter().filter_map(|i| i.as_str().map(|s| s.to_uppercase())).collect())
}

/// Full release incl. tracks, labels, ISRCs and genres (used for writing tags)
pub fn release(id: &str) -> Result<Option<Value>, Error> {
    let url = format!("https://musicbrainz.org/ws/2/release/{id}?inc=recordings+artist-credits+labels+isrcs+release-groups+genres+media&fmt=json");
    http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))
}

/// Front cover from the Cover Art Archive (1200px), None if missing.
/// Not cached on disk: almost every file is on a different release, so the cache only grew.
pub fn front_cover(release_id: &str) -> Result<Option<Vec<u8>>, Error> {
    let url = format!("https://coverartarchive.org/release/{release_id}/front-1200");
    http::get(&url, &COVERART, None)
}

/// "Artist A feat. Artist B" from an artist-credit array
pub fn artist_credit(credit: &Value) -> String {
    credit.as_array().map(|a| a.iter().map(|c| format!(
        "{}{}",
        c["name"].as_str().unwrap_or_default(),
        c["joinphrase"].as_str().unwrap_or_default()
    )).collect::<String>()).unwrap_or_default()
}

/// Individual artist names from an artist-credit array
pub fn artist_names(credit: &Value) -> Vec<String> {
    credit.as_array().map(|a| a.iter().filter_map(|c| c["name"].as_str().map(String::from)).collect()).unwrap_or_default()
}
