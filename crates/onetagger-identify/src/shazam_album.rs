//! Second route to the release: look up the album Shazam names on MusicBrainz.
//! AcoustID only lists releases linked to the exact recording ID, and MusicBrainz has many
//! unmerged duplicate recordings, so the right album is often missing there. If Shazam's album
//! exists on MusicBrainz and its tracklist has the confirmed song with a matching length,
//! that album is backed by two sources (Shazam names it, MusicBrainz lists the song on it).
use serde_json::Value;
use crate::matching::{album_for_search, album_matches, artist_similarity, hard_versions, title_similarity};
use crate::musicbrainz;
use crate::release::{group_score, SECONDARY_BAD};
use crate::types::{ReleaseCandidate, ShazamEvidence};

/// Result of the lookup
pub struct ShazamAlbum {
    pub candidate: ReleaseCandidate,
    /// Shazam's album is a compilation / mix (not an original release)
    pub compilation: bool,
}

pub enum Lookup {
    /// Song with matching length is on Shazam's album
    Found(ShazamAlbum),
    /// Shazam's album has the song, but only in another length (another edit / version)
    OtherVersion(String),
    NotFound,
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn secondary(rg: &Value) -> Vec<String> {
    rg["secondary-types"].as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default()
}

fn duration_ok(length_ms: Option<u64>, file: u32) -> bool {
    match length_ms {
        Some(ms) => (ms as f64 / 1000.0 - file as f64).abs() <= (file as f64 * 0.03).max(5.0),
        None => false,
    }
}

fn versions_match(track_title: &str, recording_title: &str, live_group: bool) -> bool {
    let mut a = hard_versions(track_title);
    let b = hard_versions(recording_title);
    if live_group && !a.iter().any(|v| v == "live") {
        a.push("live".into());
        a.sort();
    }
    a == b || (live_group && b.is_empty() && a == vec!["live".to_string()])
}

/// Find the confirmed song on the album Shazam names
pub fn find(recording_title: &str, live: bool, file_duration: u32, shazam: &ShazamEvidence) -> Lookup {
    let Some(album) = shazam.album.as_ref() else { return Lookup::NotFound };
    let query = album_for_search(album);
    if query.is_empty() {
        return Lookup::NotFound;
    }
    let groups = match musicbrainz::search_release_groups(&query, &shazam.artist) {
        Ok(g) => g,
        Err(e) => {
            warn!("MusicBrainz release group search failed: {e}");
            return Lookup::NotFound;
        }
    };
    let mut other_version: Option<String> = None;

    let mut found: Vec<ShazamAlbum> = vec![];
    for rg in groups.iter().filter(|g| g["score"].as_i64().unwrap_or(0) >= 80).take(4) {
        let title = s(&rg["title"]);
        let credit = musicbrainz::artist_credit(&rg["artist-credit"]);
        let various = credit.to_lowercase().contains("various");
        if !album_matches(album, &title) || !(various || artist_similarity(&shazam.artist, &credit) >= 0.7) {
            continue;
        }
        let sec = secondary(rg);
        let live_group = sec.iter().any(|x| x == "Live");
        // A live album only fits a live recording and vice versa
        if live_group != live {
            continue;
        }
        let kind = rg["primary-type"].as_str().map(String::from);
        let (base, kind_reason) = group_score(kind.as_deref(), &sec, live, false);

        let mut releases = musicbrainz::releases_of_group(&s(&rg["id"])).unwrap_or_default();
        releases.sort_by_key(|r| { let d = s(&r["date"]); if d.is_empty() { "9999".to_string() } else { d } });
        'releases: for release in &releases {
            for medium in release["media"].as_array().cloned().unwrap_or_default() {
                for track in medium["tracks"].as_array().cloned().unwrap_or_default() {
                    let track_title = s(&track["title"]);
                    if title_similarity(&track_title, recording_title) < 0.9 || !versions_match(&track_title, recording_title, live_group) {
                        continue;
                    }
                    let length = track["length"].as_u64().or(track["recording"]["length"].as_u64());
                    if !duration_ok(length, file_duration) {
                        if length.is_some() && other_version.is_none() {
                            other_version = Some(s(&release["title"]));
                        }
                        continue;
                    }
                    found.push(ShazamAlbum {
                        compilation: base < 20 || sec.iter().any(|x| SECONDARY_BAD.contains(&x.as_str())),
                        candidate: ReleaseCandidate {
                            recording_id: s(&track["recording"]["id"]),
                            release_id: s(&release["id"]),
                            release_group_id: s(&rg["id"]),
                            title: s(&release["title"]),
                            artist: credit.clone(),
                            kind: kind.clone(),
                            secondary: sec.clone(),
                            date: Some(s(&release["date"])).filter(|d| !d.is_empty()),
                            country: Some(s(&release["country"])).filter(|c| !c.is_empty()),
                            disc: medium["position"].as_u64().map(|x| x as u32),
                            track: track["position"].as_u64().map(|x| x as u32),
                            track_count: medium["track-count"].as_u64().map(|x| x as u32),
                            score: base + 60,
                            reasons: vec![kind_reason.to_string(), "Shazam nennt dieses Album, MusicBrainz listet den Song darauf (Länge passt)".into()],
                        },
                    });
                    break 'releases;
                }
            }
        }
    }
    // Original releases before compilations, then earliest
    found.sort_by(|a, b| a.compilation.cmp(&b.compilation)
        .then(b.candidate.score.cmp(&a.candidate.score))
        .then(a.candidate.date.clone().unwrap_or("9999".into()).cmp(&b.candidate.date.clone().unwrap_or("9999".into()))));
    match (found.into_iter().next(), other_version) {
        (Some(f), _) => Lookup::Found(f),
        (None, Some(title)) => Lookup::OtherVersion(title),
        (None, None) => Lookup::NotFound,
    }
}

/// Release suggestions from Shazam's ISRC (for files AcoustID can't confirm; review only)
pub fn isrc_suggestions(shazam: &ShazamEvidence, file_duration: u32) -> Vec<ReleaseCandidate> {
    let Some(isrc) = shazam.isrc.as_ref() else { return vec![] };
    let recordings = musicbrainz::isrc_recordings(isrc).unwrap_or_default();
    let mut out: Vec<ReleaseCandidate> = vec![];
    for rec in recordings.iter().take(3) {
        let id = s(&rec["id"]);
        let length = rec["length"].as_u64();
        if length.is_some() && !duration_ok(length, file_duration) {
            continue;
        }
        let Ok(Some(full)) = musicbrainz::recording_releases(&id) else { continue };
        let live = hard_versions(&s(&full["title"])).iter().any(|v| v == "live");
        for release in full["releases"].as_array().cloned().unwrap_or_default() {
            let rg = &release["release-group"];
            let rg_id = s(&rg["id"]);
            if out.iter().any(|c| c.release_group_id == rg_id) {
                continue;
            }
            let sec = secondary(rg);
            let kind = rg["primary-type"].as_str().map(String::from);
            let (mut score, kind_reason) = group_score(kind.as_deref(), &sec, live, false);
            let mut reasons = vec![kind_reason.to_string(), "über Shazams ISRC".to_string()];
            if shazam.album.as_ref().map(|a| album_matches(a, &s(&rg["title"]))).unwrap_or(false) {
                score += 60;
                reasons.push("Shazam nennt dieses Album".into());
            }
            let (disc, track, count) = release["media"].as_array().and_then(|m| m.iter().find_map(|m| {
                let t = m["tracks"].as_array().or(m["track"].as_array())?.first()?.clone();
                Some((m["position"].as_u64().map(|x| x as u32), t["position"].as_u64().map(|x| x as u32), m["track-count"].as_u64().map(|x| x as u32)))
            })).unwrap_or((None, None, None));
            out.push(ReleaseCandidate {
                recording_id: id.clone(),
                release_id: s(&release["id"]),
                release_group_id: rg_id,
                title: s(&release["title"]),
                artist: musicbrainz::artist_credit(&full["artist-credit"]),
                kind,
                secondary: sec,
                date: Some(s(&release["date"])).filter(|d| !d.is_empty()),
                country: Some(s(&release["country"])).filter(|c| !c.is_empty()),
                disc,
                track,
                track_count: count,
                score,
                reasons,
            });
        }
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.date.clone().unwrap_or("9999".into()).cmp(&b.date.clone().unwrap_or("9999".into()))));
    out
}
