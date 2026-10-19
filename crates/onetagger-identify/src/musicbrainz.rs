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

/// Lucene phrase with quotes / backslashes escaped
fn phrase(s: &str) -> String {
    format!("\"{}\"", s.replace(['\\', '"'], " "))
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// Search release groups by title + artist (MusicBrainz search, score 0-100)
pub fn search_release_groups(album: &str, artist: &str) -> Result<Vec<Value>, Error> {
    let query = format!("releasegroup:{} AND artist:{}", phrase(album), phrase(artist));
    let url = format!("https://musicbrainz.org/ws/2/release-group?query={}&limit=10&fmt=json", encode(&query));
    let v: Option<Value> = http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))?;
    Ok(v.and_then(|v| v["release-groups"].as_array().cloned()).unwrap_or_default())
}

/// Official releases of a release group incl. tracklists
pub fn releases_of_group(release_group: &str) -> Result<Vec<Value>, Error> {
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={release_group}&status=official&inc=recordings+media+artist-credits&limit=25&fmt=json");
    let v: Option<Value> = http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))?;
    Ok(v.and_then(|v| v["releases"].as_array().cloned()).unwrap_or_default())
}

/// Recordings with this ISRC
pub fn isrc_recordings(isrc: &str) -> Result<Vec<Value>, Error> {
    let url = format!("https://musicbrainz.org/ws/2/isrc/{}?inc=artist-credits&fmt=json", encode(isrc));
    let v: Option<Value> = http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))?;
    Ok(v.and_then(|v| v["recordings"].as_array().cloned()).unwrap_or_default())
}

/// Recording incl. its releases, release groups and track positions
pub fn recording_releases(id: &str) -> Result<Option<Value>, Error> {
    let url = format!("https://musicbrainz.org/ws/2/recording/{id}?inc=releases+release-groups+media+artist-credits&status=official&fmt=json");
    http::get_json(&url, &MUSICBRAINZ, Some("musicbrainz"))
}
