//! Background run for the UI: results are collected here and polled by the UI,
//! so the review list can be worked on while the run continues.
//! Every result is also appended to a report file, so a restart keeps the list.
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::apply::{self, ApplyConfig};
use crate::journal::{self, Journal};
use crate::pipeline::{self, IdentifyConfig};
use crate::types::{IdentifyResult, IdentifyState};

#[derive(Default)]
struct RunState {
    running: bool,
    total: usize,
    results: Vec<IdentifyResult>,
    journal: Option<Arc<Journal>>,
    report: Option<PathBuf>,
}

lazy_static::lazy_static! {
    static ref RUN: Mutex<RunState> = Mutex::new(RunState::default());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    /// Results with index >= `since`
    pub results: Vec<IdentifyResult>,
    pub since: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalInfo {
    pub path: PathBuf,
    pub name: String,
    pub files: usize,
}

fn reports_dir() -> PathBuf {
    let dir = onetagger_shared::Settings::get_folder().unwrap_or_else(|_| std::env::temp_dir()).join("identify_runs");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn append_report(path: &Path, result: &IdentifyResult) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(line) = serde_json::to_string(result) {
            writeln!(f, "{line}").ok();
        }
    }
}

/// Start a run in the background
pub fn start(files: Vec<PathBuf>, config: IdentifyConfig) -> Result<(), Error> {
    let mut state = RUN.lock().unwrap();
    if state.running {
        return Err(anyhow!("Identify is already running"));
    }
    let (rx, journal) = pipeline::run(files.clone(), config.clone())?;
    let report = reports_dir().join(format!("{}.jsonl", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    *state = RunState {
        running: true,
        total: files.len(),
        results: vec![],
        journal,
        report: Some(report.clone()),
    };
    drop(state);

    std::thread::spawn(move || {
        for result in rx {
            append_report(&report, &result);
            RUN.lock().unwrap().results.push(result);
        }
        RUN.lock().unwrap().running = false;
        info!("Identify run finished");
    });
    Ok(())
}

pub fn status(since: usize) -> RunStatus {
    let state = RUN.lock().unwrap();
    RunStatus {
        running: state.running,
        total: state.total,
        done: state.results.len(),
        results: state.results.iter().skip(since).cloned().collect(),
        since,
    }
}

/// Load the latest report when nothing runs (after a restart)
pub fn load_last() -> Result<usize, Error> {
    let mut state = RUN.lock().unwrap();
    if state.running {
        return Ok(state.results.len());
    }
    let mut reports: Vec<PathBuf> = std::fs::read_dir(reports_dir())?.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false)).collect();
    reports.sort();
    let Some(last) = reports.pop() else { return Ok(0) };
    let results: Vec<IdentifyResult> = std::fs::read_to_string(&last)?.lines()
        .filter_map(|l| serde_json::from_str(l).ok()).collect();
    *state = RunState {
        running: false,
        total: results.len(),
        results,
        journal: None,
        report: Some(last),
    };
    Ok(state.results.len())
}

/// Apply one result manually with the chosen release (review list)
pub fn apply_one(index: usize, release: usize, apply_config: ApplyConfig) -> Result<PathBuf, Error> {
    let (mut result, journal, report) = {
        let mut state = RUN.lock().unwrap();
        let result = state.results.get(index).cloned().ok_or(anyhow!("Unknown result"))?;
        if state.journal.is_none() {
            state.journal = Some(Arc::new(pipeline::new_journal(&apply_config.target)?));
        }
        (result, state.journal.clone().unwrap(), state.report.clone())
    };
    if result.applied.is_some() {
        return Err(anyhow!("Already applied"));
    }
    if release >= result.releases.len() {
        return Err(anyhow!("Unknown release"));
    }
    result.release = Some(release);
    let dest = apply::apply(&result, &apply_config, &journal)?;
    result.applied = Some(dest.clone());
    result.state = IdentifyState::Confirmed;
    result.reason = format!("Manuell übernommen: {}", result.releases[release].title);
    if let Some(r) = &report {
        append_report(r, &result);
    }
    RUN.lock().unwrap().results[index] = result;
    Ok(dest)
}

/// Mark a review item as handled without changing the file
pub fn skip(index: usize) {
    if let Some(r) = RUN.lock().unwrap().results.get_mut(index) {
        r.state = IdentifyState::Unknown;
        r.reason = format!("Übersprungen ({})", r.reason);
    }
}

/// Undo journals in a target folder, newest first
pub fn journals(target: &Path) -> Vec<JournalInfo> {
    let dir = target.join(".onetagger-identify");
    let mut out: Vec<JournalInfo> = std::fs::read_dir(&dir).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| JournalInfo {
            files: Journal::entries(&p).map(|e| e.iter().filter(|x| x.done).count()).unwrap_or(0),
            name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            path: p,
        }).collect()).unwrap_or_default();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

pub fn undo(journal_dir: &Path) -> Result<(usize, Vec<String>), Error> {
    if RUN.lock().unwrap().running {
        return Err(anyhow!("Stop the running identify first"));
    }
    journal::undo(journal_dir)
}
