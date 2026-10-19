use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IdentifyState {
    /// Shazam and AcoustID agree, release is clear: safe to apply
    Confirmed,
    /// Needs a human decision
    Review,
    /// Neither source knows the file
    Unknown,
    Error,
}

/// What Shazam said
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShazamEvidence {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub label: Option<String>,
    pub released: Option<String>,
    pub isrc: Option<String>,
    /// Position in the file (0-1) that matched
    pub position: f64,
}

/// One recording AcoustID offered
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecordingCandidate {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<f64>,
    /// AcoustID fingerprint score
    pub score: f64,
    pub duration_ok: bool,
    /// Shazam agrees with this recording
    pub shazam_agrees: bool,
    /// Same song, but Shazam names another version (remix / live / edit): the fingerprint decided
    #[serde(default)]
    pub shazam_other_version: bool,
    pub note: String,
}

/// A release the confirmed recording appears on
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCandidate {
    /// Recording on this release (may differ from the top AcoustID match)
    pub recording_id: String,
    pub release_id: String,
    pub release_group_id: String,
    pub title: String,
    pub artist: String,
    pub kind: Option<String>,
    pub secondary: Vec<String>,
    pub date: Option<String>,
    pub country: Option<String>,
    pub disc: Option<u32>,
    pub track: Option<u32>,
    pub track_count: Option<u32>,
    pub score: i32,
    pub reasons: Vec<String>,
}

/// What the file itself claims (tags / filename) - never trusted on its own
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub filename: String,
    /// Claims match the confirmed result
    pub title_confirmed: bool,
    pub album_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyResult {
    pub path: PathBuf,
    pub state: IdentifyState,
    /// Human readable explanation
    pub reason: String,
    /// File duration in seconds
    pub duration: u32,
    pub acoustid_score: Option<f64>,
    pub shazam: Option<ShazamEvidence>,
    pub claims: Claims,
    pub recordings: Vec<RecordingCandidate>,
    /// Chosen recording MBID
    pub recording: Option<String>,
    pub releases: Vec<ReleaseCandidate>,
    /// Index into releases
    pub release: Option<usize>,
    /// Where the file was moved after tagging
    pub applied: Option<PathBuf>,
}

impl IdentifyResult {
    pub fn chosen_release(&self) -> Option<&ReleaseCandidate> {
        self.release.and_then(|i| self.releases.get(i))
    }
}
