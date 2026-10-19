//! Pick the release for a confirmed recording.
//! Only releases containing exactly this recording are candidates; without independent
//! evidence the earliest original album wins, compilations only if Shazam names them.
use std::collections::{HashMap, HashSet};
use crate::acoustid::{artist_credit, AidRecording};
use crate::matching::{album_matches, core_album, core_title, hard_versions};
use crate::types::{ReleaseCandidate, ShazamEvidence};

pub struct ReleaseChoice {
    pub candidates: Vec<ReleaseCandidate>,
    pub chosen: Option<usize>,
    /// Safe to apply without review
    pub clear: bool,
    pub reason: String,
    /// Why it is not clear
    pub unclear: Option<Unclear>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unclear {
    NoRelease,
    OnlyCompilations,
    ShazamOtherAlbum,
    TrackUnknown,
    Tie,
}

pub(crate) const SECONDARY_BAD: [&str; 4] = ["Compilation", "DJ-mix", "Mixtape/Street", "Remix"];

pub(crate) fn group_score(kind: Option<&str>, secondary: &[String], live_recording: bool, remix_recording: bool) -> (i32, &'static str) {
    let has = |s: &str| secondary.iter().any(|x| x == s);
    if has("Interview") || has("Spokenword") || has("Audiobook") {
        return (-40, "Interview/Wortbeitrag");
    }
    if has("Live") && !live_recording {
        return (-30, "Live-Release, Aufnahme ist nicht live");
    }
    if has("Remix") && remix_recording && !has("Compilation") {
        return (30, "Remix-Release");
    }
    if SECONDARY_BAD.iter().any(|b| has(b)) {
        return (-10, "Compilation/Mix");
    }
    if has("Soundtrack") {
        return (20, "Soundtrack");
    }
    match kind {
        Some("Album") => (40, "Album"),
        Some("EP") => (30, "EP"),
        Some("Single") => (25, "Single"),
        Some("Broadcast") => (0, "Broadcast"),
        _ => (5, "Sonstiges"),
    }
}

fn date_string(d: &Option<crate::acoustid::AidDate>) -> Option<String> {
    let d = d.as_ref()?;
    let y = d.year?;
    Some(match (d.month, d.day) {
        (Some(m), Some(day)) => format!("{y:04}-{m:02}-{day:02}"),
        (Some(m), None) => format!("{y:04}-{m:02}"),
        _ => format!("{y:04}"),
    })
}

/// `recs`: recordings both sources agree on, best first.
/// `siblings`: release group id -> number of files already confirmed on it in this run
pub fn choose(recs: &[&AidRecording], shazam: Option<&ShazamEvidence>, siblings: &HashMap<String, u32>) -> ReleaseChoice {
    let mut candidates = vec![];
    let mut shazam_named: HashSet<String> = HashSet::new();
    let mut seen_groups = HashSet::new();
    for rec in recs {
        candidates_for(rec, shazam, siblings, &mut seen_groups, &mut shazam_named, &mut candidates);
    }
    // Shazam naming just the single of this song ("Song - Single") is no objection to the album
    let is_own_single = |a: &str| a.trim_end().to_lowercase().ends_with("- single")
        && recs.iter().any(|r| core_album(a) == core_title(r.title.as_deref().unwrap_or_default()));
    let shazam_album = shazam.and_then(|s| s.album.as_ref())
        .filter(|a| !core_album(a).is_empty() && !is_own_single(a))
        .cloned();
    decide(candidates, shazam_named, shazam_album)
}

fn candidates_for(
    rec: &AidRecording,
    shazam: Option<&ShazamEvidence>,
    siblings: &HashMap<String, u32>,
    seen_groups: &mut HashSet<String>,
    shazam_named: &mut HashSet<String>,
    candidates: &mut Vec<ReleaseCandidate>,
) {
    let title = rec.title.clone().unwrap_or_default();
    let versions = hard_versions(&title);
    // Live recordings often have no "(live)" in the title, but then only appear on live releases
    let only_live_groups = !rec.releasegroups.is_empty()
        && rec.releasegroups.iter().all(|g| g.secondarytypes.iter().any(|s| s == "Live"));
    let live = versions.iter().any(|v| v == "live") || only_live_groups;
    let remix = versions.iter().any(|v| v == "remix");

    for group in &rec.releasegroups {
        // Same release group via a duplicate recording: keep the first (best scored) recording
        if !seen_groups.insert(group.id.clone()) {
            continue;
        }
        let (mut base, kind_reason) = group_score(group.kind.as_deref(), &group.secondarytypes, live, remix);
        let mut reasons = vec![kind_reason.to_string()];
        let group_title = group.title.clone().unwrap_or_default();

        // Independent evidence: Shazam names this album
        if let Some(album) = shazam.and_then(|s| s.album.as_ref()) {
            if album_matches(album, &group_title) || group.releases.iter().any(|r| album_matches(album, r.title.as_deref().unwrap_or_default())) {
                base += 60;
                reasons.push("Shazam nennt dieses Album".into());
                shazam_named.insert(group.id.clone());
            }
        }
        // Independent evidence: other confirmed files of this run are on it
        if let Some(n) = siblings.get(&group.id) {
            base += (*n as i32 * 15).min(45);
            reasons.push(format!("{n} weitere Datei(en) dieses Laufs auf diesem Album"));
        }

        // Earliest release within the group, prefer ones that list the track position
        let mut releases = group.releases.clone();
        releases.sort_by_key(|r| (
            r.mediums.iter().all(|m| m.tracks.is_empty()),
            r.date.as_ref().map(|d| d.key()).unwrap_or((9999, 13, 32)),
        ));
        if let Some(r) = releases.first() {
            let (disc, track, count) = r.mediums.iter()
                .find_map(|m| m.tracks.first().map(|t| (m.position, t.position, m.track_count)))
                .unwrap_or((None, None, None));
            let year = r.date.as_ref().and_then(|d| d.year);
            if let (Some(y), Some(released)) = (year, shazam.and_then(|s| s.released.as_ref())) {
                if released.starts_with(&y.to_string()) {
                    base += 5;
                    reasons.push("Jahr wie bei Shazam".into());
                }
            }
            candidates.push(ReleaseCandidate {
                recording_id: rec.id.clone(),
                release_id: r.id.clone(),
                release_group_id: group.id.clone(),
                title: r.title.clone().unwrap_or(group_title.clone()),
                artist: artist_credit(&group.artists),
                kind: group.kind.clone(),
                secondary: group.secondarytypes.clone(),
                date: date_string(&r.date),
                country: r.country.clone(),
                disc,
                track,
                track_count: count,
                score: base,
                reasons,
            });
        }
    }
}

fn decide(mut candidates: Vec<ReleaseCandidate>, shazam_named: HashSet<String>, shazam_album: Option<String>) -> ReleaseChoice {
    // Best score first, then the earliest date (= original)
    candidates.sort_by(|a, b| b.score.cmp(&a.score)
        .then(a.date.clone().unwrap_or("9999".into()).cmp(&b.date.clone().unwrap_or("9999".into()))));

    let Some(best) = candidates.first() else {
        return ReleaseChoice { candidates, chosen: None, clear: false, reason: "AcoustID kennt kein Release dieser Aufnahme".into(), unclear: Some(Unclear::NoRelease) };
    };
    // Below a plain single: only compilations, mixes and the like
    let best_is_compilation = best.score < 20;
    let named = shazam_named.contains(&best.release_group_id);
    let year = |c: &ReleaseCandidate| c.date.clone().unwrap_or_default().chars().take(4).collect::<String>();
    let tie = candidates.get(1).map(|second| second.score == best.score && year(second) == year(best)).unwrap_or(false);

    let (unclear, reason) = if best_is_compilation && !named {
        (Some(Unclear::OnlyCompilations), "Kein Album/Single, nur Compilations oder Mixe, und Shazam nennt keins davon".to_string())
    } else if let (Some(album), false) = (&shazam_album, named) {
        // Zero trust: both sources have to agree on the album too
        (Some(Unclear::ShazamOtherAlbum), format!("Shazam nennt ein anderes Album („{album}“) als „{}“", best.title))
    } else if best.track.is_none() {
        (Some(Unclear::TrackUnknown), format!("Trackposition auf „{}“ unbekannt", best.title))
    } else if tie {
        (Some(Unclear::Tie), format!("Gleichwertige Releases im selben Jahr: „{}“ / „{}“", best.title, candidates[1].title))
    } else {
        (None, format!("„{}“ ({})", best.title, best.reasons.join(", ")))
    };
    let clear = unclear.is_none();
    ReleaseChoice { chosen: Some(0), clear, reason, candidates, unclear }
}
