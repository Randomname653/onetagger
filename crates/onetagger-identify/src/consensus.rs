//! Decide which recording a file is: AcoustID and Shazam must agree
use std::collections::HashMap;
use crate::acoustid::{artist_credit, AcoustIdResponse, AidRecording};
use crate::matching::{artist_similarity, featured_artists, hard_versions, title_similarity};
use crate::musicbrainz;
use crate::types::{IdentifyState, RecordingCandidate, ShazamEvidence};

/// Fingerprint score needed to trust AcoustID
pub const MIN_ACOUSTID_SCORE: f64 = 0.80;
/// Lower score still shown as evidence
const MIN_LISTED_SCORE: f64 = 0.50;
const TITLE_THRESHOLD: f64 = 0.85;
/// Fingerprint score needed for the live rule
const LIVE_RULE_SCORE: f64 = 0.90;
const ARTIST_THRESHOLD: f64 = 0.70;
/// How many candidates may cost a MusicBrainz ISRC lookup
const MAX_ISRC_LOOKUPS: usize = 3;

pub struct Consensus {
    pub state: IdentifyState,
    pub reason: String,
    pub candidates: Vec<RecordingCandidate>,
    /// Matching AcoustID recordings (same order as candidates)
    pub recordings: Vec<AidRecording>,
    /// Index of the confirmed recording
    pub chosen: Option<usize>,
    /// All recordings both sources agree on (MusicBrainz often has unmerged duplicates)
    pub equivalent: Vec<usize>,
    pub best_score: Option<f64>,
}

fn duration_ok(recording: Option<f64>, file: u32) -> bool {
    match recording {
        Some(d) => (d - file as f64).abs() <= (file as f64 * 0.03).max(5.0),
        None => true,
    }
}

/// Shazam may omit a version (fingerprint already tells live from studio),
/// but a version Shazam names must be present on the recording
fn versions_compatible(shazam_title: &str, rec: &AidRecording) -> bool {
    let mut rec_versions = hard_versions(rec.title.as_deref().unwrap_or_default());
    // Live recordings often lack "(live)" but only appear on live releases
    if !rec.releasegroups.is_empty() && rec.releasegroups.iter().all(|g| g.secondarytypes.iter().any(|s| s == "Live")) {
        rec_versions.push("live".into());
    }
    let rec = rec_versions;
    hard_versions(shazam_title).iter().all(|v| rec.contains(v))
}

/// Shazam names a live version, the fingerprint matched a studio recording and nothing else differs
fn only_live_differs(shazam_title: &str, rec: &AidRecording) -> bool {
    let rec_versions = hard_versions(rec.title.as_deref().unwrap_or_default());
    let missing: Vec<String> = hard_versions(shazam_title).into_iter().filter(|v| !rec_versions.contains(v)).collect();
    missing == vec!["live".to_string()]
}

/// Best artist match incl. artists credited as "feat." in either title
fn artist_match(shazam: &ShazamEvidence, artist: &str, title: &str) -> f64 {
    let mut shazam_names = vec![shazam.artist.clone()];
    shazam_names.extend(featured_artists(&shazam.title));
    let mut rec_names = vec![artist.to_string()];
    rec_names.extend(featured_artists(title));
    shazam_names.iter().flat_map(|a| rec_names.iter().map(move |b| artist_similarity(a, b))).fold(0.0, f64::max)
}

pub fn evaluate(file_duration: u32, aid: &AcoustIdResponse, shazam: Option<&ShazamEvidence>) -> Consensus {
    // Flatten recordings, best score per recording id
    let mut by_id: HashMap<String, (f64, AidRecording)> = HashMap::new();
    for result in &aid.results {
        if result.score < MIN_LISTED_SCORE {
            continue;
        }
        for rec in &result.recordings {
            if rec.title.is_none() {
                continue;
            }
            let e = by_id.entry(rec.id.clone()).or_insert((result.score, rec.clone()));
            if result.score > e.0 {
                *e = (result.score, rec.clone());
            }
        }
    }
    let mut recs: Vec<(f64, AidRecording)> = by_id.into_values().collect();
    recs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(b.1.releasegroups.len().cmp(&a.1.releasegroups.len())));
    let best_score = recs.first().map(|r| r.0);

    let mut isrc_lookups = 0;
    let candidates: Vec<RecordingCandidate> = recs.iter().map(|(score, rec)| {
        let title = rec.title.clone().unwrap_or_default();
        let artist = artist_credit(&rec.artists);
        let mut c = RecordingCandidate {
            id: rec.id.clone(),
            title: title.clone(),
            artist: artist.clone(),
            duration: rec.duration,
            score: *score,
            duration_ok: duration_ok(rec.duration, file_duration),
            ..Default::default()
        };
        if let Some(s) = shazam {
            let ts = title_similarity(&s.title, &title);
            let as_ = artist_match(s, &artist, &title);
            let versions = versions_compatible(&s.title, rec);
            let same_song = ts >= TITLE_THRESHOLD && as_ >= ARTIST_THRESHOLD;
            if same_song && versions {
                c.shazam_agrees = true;
                c.note = format!("Shazam stimmt überein (Titel {:.0}%, Interpret {:.0}%)", ts * 100.0, as_ * 100.0);
            } else if same_song && only_live_differs(&s.title, rec) && *score >= LIVE_RULE_SCORE
                && rec.duration.is_some() && c.duration_ok {
                // Live rule: same song, only Shazam says live. The fingerprint is version specific
                // and the length matches the studio recording, so AcoustID decides the version.
                c.shazam_agrees = true;
                c.note = format!("Gleicher Song; Shazam nennt eine Live-Version, Fingerprint ({:.2}) und Dauer passen zur Studioaufnahme", score);
            } else if let (Some(isrc), true) = (&s.isrc, *score >= MIN_ACOUSTID_SCORE && isrc_lookups < MAX_ISRC_LOOKUPS) {
                // Different spelling / language: same ISRC proves the same recording
                isrc_lookups += 1;
                match musicbrainz::recording_isrcs(&rec.id) {
                    Ok(isrcs) if isrcs.contains(&isrc.to_uppercase()) => {
                        c.shazam_agrees = true;
                        c.note = format!("Gleicher ISRC wie Shazam ({isrc})");
                    },
                    Ok(_) => c.note = format!("Shazam weicht ab (Titel {:.0}%, Interpret {:.0}%{})", ts * 100.0, as_ * 100.0, if versions { "" } else { ", andere Version" }),
                    Err(e) => c.note = format!("ISRC-Abfrage fehlgeschlagen: {e}"),
                }
            } else {
                c.note = format!("Shazam weicht ab (Titel {:.0}%, Interpret {:.0}%{})", ts * 100.0, as_ * 100.0, if versions { "" } else { ", andere Version" });
            }
        }
        c
    }).collect();

    let recordings: Vec<AidRecording> = recs.into_iter().map(|r| r.1).collect();
    let mut out = Consensus { state: IdentifyState::Unknown, reason: String::new(), candidates, recordings, chosen: None, equivalent: vec![], best_score };

    // Confirmed: trusted fingerprint + matching duration + Shazam agrees
    let confirmed = out.candidates.iter().position(|c| c.score >= MIN_ACOUSTID_SCORE && c.duration_ok && c.shazam_agrees);
    if let Some(i) = confirmed {
        let c = &out.candidates[i];
        out.state = IdentifyState::Confirmed;
        out.reason = format!("AcoustID ({:.2}) und Shazam: {} – {}", c.score, c.artist, c.title);
        out.chosen = Some(i);
        out.equivalent = out.candidates.iter().enumerate()
            .filter(|(_, c)| c.score >= MIN_ACOUSTID_SCORE && c.duration_ok && c.shazam_agrees)
            .map(|(i, _)| i).collect();
        return out;
    }

    let top = out.candidates.first();
    out.state = IdentifyState::Review;
    out.reason = match (top, shazam) {
        (None, None) => {
            out.state = IdentifyState::Unknown;
            "Weder AcoustID noch Shazam kennen die Datei".to_string()
        },
        (None, Some(s)) => format!("Nur Shazam erkannt: {} – {}", s.artist, s.title),
        (Some(c), None) if c.score >= MIN_ACOUSTID_SCORE => format!("Nur AcoustID erkannt ({:.2}): {} – {}", c.score, c.artist, c.title),
        (Some(c), None) => format!("Nur unsicherer AcoustID-Treffer ({:.2}): {} – {}", c.score, c.artist, c.title),
        (Some(_), Some(s)) => {
            if let Some(c) = out.candidates.iter().find(|c| c.shazam_agrees) {
                if !c.duration_ok {
                    format!("Dauer passt nicht: Datei {}s, Aufnahme {:.0}s ({} – {})", file_duration, c.duration.unwrap_or_default(), c.artist, c.title)
                } else {
                    format!("AcoustID-Score zu niedrig ({:.2}): {} – {}", c.score, c.artist, c.title)
                }
            } else {
                let c = &out.candidates[0];
                format!("Widerspruch: AcoustID sagt {} – {}, Shazam sagt {} – {}", c.artist, c.title, s.artist, s.title)
            }
        },
    };
    out
}
