use anyhow::Error;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use crossbeam_channel::{unbounded, Receiver};
use serde::{Deserialize, Serialize};
use onetagger_autotag::shazam::Shazam;
use onetagger_tag::{Field, Tag};
use crate::acoustid::{self, AcoustIdResponse};
use crate::fingerprint;
use crate::http;
use crate::matching::{album_similarity, title_similarity};
use crate::types::*;
use crate::{consensus, release};
use crate::apply::{self, ApplyConfig};
use crate::journal::Journal;

lazy_static::lazy_static! {
    pub static ref STOP: AtomicBool = AtomicBool::new(false);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyConfig {
    pub acoustid_key: String,
    pub threads: usize,
    /// Tag + move confirmed files right away
    pub apply: Option<ApplyConfig>,
}

/// Cached per-file Shazam result (keyed by file content), so a restart doesn't ask again
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShazamCache {
    evidence: Option<ShazamEvidence>,
}

/// Content key: hash of the first 256 KB + size
fn file_key(path: &Path) -> Result<String, Error> {
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    let mut buf = vec![0u8; 256 * 1024];
    let n = f.read(&mut buf)?;
    Ok(format!("{:016x}-{size}", http::fnv64(&buf[..n])))
}

fn shazam_evidence(path: &Path, duration: u32) -> Result<Option<ShazamEvidence>, Error> {
    let cache_file = http::cache_dir("shazam_files").join(file_key(path)?);
    if let Ok(data) = std::fs::read(&cache_file) {
        if let Ok(c) = serde_json::from_slice::<ShazamCache>(&data) {
            return Ok(c.evidence);
        }
    }
    // Middle first, then a second window if Shazam doesn't know that part
    let mut positions = vec![0.5];
    if duration >= 30 {
        positions.push(0.3);
    }
    let mut evidence = None;
    for position in positions {
        if STOP.load(Ordering::Relaxed) {
            return Err(anyhow!("Stopped"));
        }
        let (track, _) = Shazam::recognize_at(path, position)?;
        if let Some(t) = track {
            evidence = Some(ShazamEvidence {
                album: t.metadata("Album").map(String::from),
                label: t.metadata("Label").map(String::from),
                released: t.metadata("Released").map(String::from),
                title: t.title,
                artist: t.subtitle,
                isrc: t.isrc,
                position,
            });
            break;
        }
    }
    std::fs::write(&cache_file, serde_json::to_vec(&ShazamCache { evidence: evidence.clone() })?).ok();
    Ok(evidence)
}

fn claims(path: &Path) -> Claims {
    let mut c = Claims {
        filename: path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
        ..Default::default()
    };
    if let Ok(tag) = Tag::load_file(path, false) {
        let t = tag.tag();
        let first = |f: Field| t.get_field(f).and_then(|v| v.first().cloned());
        c.title = first(Field::Title);
        c.artist = first(Field::Artist);
        c.album = first(Field::Album);
    }
    c
}

/// Identify one file. `siblings`: release group -> confirmed files in this run
pub fn identify_file(path: &Path, config: &IdentifyConfig, siblings: &HashMap<String, u32>) -> IdentifyResult {
    let mut result = IdentifyResult {
        path: path.to_owned(),
        state: IdentifyState::Error,
        reason: String::new(),
        duration: 0,
        acoustid_score: None,
        shazam: None,
        claims: claims(path),
        recordings: vec![],
        recording: None,
        releases: vec![],
        release: None,
        applied: None,
    };

    // AcoustID
    let fp = match fingerprint::fingerprint_file(path) {
        Ok(fp) => fp,
        Err(e) => {
            result.reason = format!("Datei nicht lesbar: {e}");
            return result;
        }
    };
    result.duration = fp.duration;
    let aid = match acoustid::lookup(&config.acoustid_key, &fp) {
        Ok(a) => a,
        Err(e) => {
            result.reason = format!("AcoustID-Fehler: {e}");
            return result;
        }
    };

    // Shazam
    let shazam = match shazam_evidence(path, fp.duration) {
        Ok(s) => s,
        Err(e) => {
            result.reason = format!("Shazam-Fehler: {e}");
            return result;
        }
    };
    result.shazam = shazam.clone();

    evaluate(&mut result, &aid, siblings);
    result
}

/// Consensus + release choice on gathered evidence
fn evaluate(result: &mut IdentifyResult, aid: &AcoustIdResponse, siblings: &HashMap<String, u32>) {
    let cons = consensus::evaluate(result.duration, aid, result.shazam.as_ref());
    result.acoustid_score = cons.best_score;
    result.recordings = cons.candidates.clone();
    result.state = cons.state;
    result.reason = cons.reason.clone();
    let Some(i) = cons.chosen else { return };

    let recs: Vec<&crate::acoustid::AidRecording> = cons.equivalent.iter().map(|j| &cons.recordings[*j]).collect();
    let choice = release::choose(&recs, result.shazam.as_ref(), siblings);
    result.releases = choice.candidates;
    result.release = choice.chosen;
    result.recording = result.chosen_release().map(|r| r.recording_id.clone()).or(Some(cons.recordings[i].id.clone()));
    if !choice.clear {
        result.state = IdentifyState::Review;
        result.reason = format!("{} – Album unklar: {}", cons.reason, choice.reason);
    } else {
        result.reason = format!("{} → {}", cons.reason, choice.reason);
    }

    // Did the old tags tell the truth?
    let title = &result.recordings.iter().find(|r| Some(&r.id) == result.recording.as_ref()).unwrap_or(&result.recordings[i]).title.clone();
    result.claims.title_confirmed = result.claims.title.as_ref().map(|t| title_similarity(t, title) >= 0.9).unwrap_or(false);
    if let Some(r) = result.chosen_release() {
        let album = r.title.clone();
        result.claims.album_confirmed = result.claims.album.as_ref().map(|a| album_similarity(a, &album) >= 0.9).unwrap_or(false);
    }
}

/// Journal folder for a new run inside the target folder
pub fn new_journal(target: &Path) -> Result<Journal, Error> {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    Journal::create(target.join(".onetagger-identify").join(stamp))
}

/// Identify files on worker threads; results stream back in completion order
pub fn run(files: Vec<PathBuf>, config: IdentifyConfig) -> Result<Receiver<IdentifyResult>, Error> {
    STOP.store(false, Ordering::SeqCst);
    let journal = match &config.apply {
        Some(a) => Some(Arc::new(new_journal(&a.target)?)),
        None => None,
    };
    let (tx, rx) = unbounded();
    let queue = Arc::new(Mutex::new(files.into_iter().rev().collect::<Vec<_>>()));
    let siblings: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    for _ in 0..config.threads.max(1) {
        let (tx, queue, siblings, config, journal) = (tx.clone(), queue.clone(), siblings.clone(), config.clone(), journal.clone());
        std::thread::spawn(move || loop {
            if STOP.load(Ordering::Relaxed) {
                break;
            }
            let Some(path) = queue.lock().unwrap().pop() else { break };
            let snapshot = siblings.lock().unwrap().clone();
            let mut result = identify_file(&path, &config, &snapshot);
            if result.state == IdentifyState::Confirmed {
                if let Some(r) = result.chosen_release() {
                    *siblings.lock().unwrap().entry(r.release_group_id.clone()).or_insert(0) += 1;
                }
                if let (Some(apply_config), Some(journal)) = (&config.apply, &journal) {
                    match apply::apply(&result, apply_config, journal) {
                        Ok(dest) => result.applied = Some(dest),
                        Err(e) => {
                            warn!("Apply failed for {path:?}: {e}");
                            result.state = IdentifyState::Review;
                            result.reason = format!("Anwenden fehlgeschlagen: {e} ({})", result.reason);
                        }
                    }
                }
            }
            if tx.send(result).is_err() {
                break;
            }
        });
    }
    Ok(rx)
}
