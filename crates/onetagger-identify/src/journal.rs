//! Lossless undo: the raw metadata of every file is saved before it is touched.
//! MP3: the ID3v2 block, FLAC: all metadata blocks (audio frames are never modified),
//! M4A: every ilst data item. Undo splices the bytes back and moves the file home.
use anyhow::Error;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use mp4ameta::{Data, DataIdent};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub original_path: PathBuf,
    pub new_path: PathBuf,
    /// File holding the snapshot (inside the journal folder)
    pub snapshot: String,
    pub kind: SnapshotKind,
    pub recording_id: Option<String>,
    pub release_id: Option<String>,
    /// false until tags were written and the file moved
    pub done: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotKind {
    /// Raw bytes in front of the audio data (ID3v2 / FLAC metadata)
    Prefix,
    Mp4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Mp4Item {
    fourcc: Option<[u8; 4]>,
    mean: Option<String>,
    name: Option<String>,
    kind: String,
    code: u32,
    data: String,
}

pub struct Journal {
    pub dir: PathBuf,
    file: PathBuf,
    counter: std::sync::atomic::AtomicUsize,
}

fn syncsafe(b: &[u8]) -> usize {
    ((b[0] as usize & 0x7f) << 21) | ((b[1] as usize & 0x7f) << 14) | ((b[2] as usize & 0x7f) << 7) | (b[3] as usize & 0x7f)
}

/// Length of the ID3v2 tag at the start of the data (0 if none)
fn id3_len(data: &[u8]) -> usize {
    if data.len() >= 10 && &data[0..3] == b"ID3" {
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        (10 + syncsafe(&data[6..10]) + footer).min(data.len())
    } else {
        0
    }
}

/// Length of everything in front of the audio data
pub fn metadata_len(path: &Path, data: &[u8]) -> Result<usize, Error> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let start = id3_len(data);
    if ext != "flac" {
        return Ok(start);
    }
    if data.len() < start + 4 || &data[start..start + 4] != b"fLaC" {
        return Err(anyhow!("Not a FLAC stream"));
    }
    let mut pos = start + 4;
    loop {
        if pos + 4 > data.len() {
            return Err(anyhow!("Truncated FLAC metadata"));
        }
        let last = data[pos] & 0x80 != 0;
        let len = ((data[pos + 1] as usize) << 16) | ((data[pos + 2] as usize) << 8) | data[pos + 3] as usize;
        pos += 4 + len;
        if last {
            return Ok(pos.min(data.len()));
        }
    }
}

fn is_mp4(path: &Path) -> bool {
    matches!(path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("m4a") | Some("mp4"))
}

fn b64(d: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(d)
}

fn unb64(s: &str) -> Result<Vec<u8>, Error> {
    Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
}

impl Journal {
    /// New journal folder for one run
    pub fn create(dir: impl AsRef<Path>) -> Result<Journal, Error> {
        let dir = dir.as_ref().to_owned();
        std::fs::create_dir_all(&dir)?;
        Ok(Journal { file: dir.join("journal.jsonl"), dir, counter: Default::default() })
    }

    /// Save the current metadata of `path`, returns the entry to complete later
    pub fn snapshot(&self, path: &Path, new_path: &Path, recording_id: Option<String>, release_id: Option<String>) -> Result<JournalEntry, Error> {
        let n = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (kind, name) = if is_mp4(path) {
            let tag = mp4ameta::Tag::read_from_path(path)?;
            let items: Vec<Mp4Item> = tag.data().map(|(ident, data)| {
                let (fourcc, mean, name) = match ident {
                    DataIdent::Fourcc(f) => (Some(f.0), None, None),
                    DataIdent::Freeform { mean, name } => (None, Some(mean.to_string()), Some(name.to_string())),
                };
                let (kind, code, bytes): (&str, u32, Vec<u8>) = match data {
                    Data::Reserved(d) => ("reserved", 0, d.clone()),
                    Data::Utf8(s) => ("utf8", 0, s.as_bytes().to_vec()),
                    Data::Utf16(s) => ("utf16", 0, s.as_bytes().to_vec()),
                    Data::Jpeg(d) => ("jpeg", 0, d.clone()),
                    Data::Png(d) => ("png", 0, d.clone()),
                    Data::Bmp(d) => ("bmp", 0, d.clone()),
                    Data::BeSigned(d) => ("besigned", 0, d.clone()),
                    Data::Unknown { code, data } => ("unknown", *code, data.clone()),
                };
                Mp4Item { fourcc, mean, name, kind: kind.into(), code, data: b64(&bytes) }
            }).collect();
            let name = format!("{n:06}.mp4.json");
            std::fs::write(self.dir.join(&name), serde_json::to_vec(&items)?)?;
            (SnapshotKind::Mp4, name)
        } else {
            let data = std::fs::read(path)?;
            let len = metadata_len(path, &data)?;
            let name = format!("{n:06}.meta");
            std::fs::write(self.dir.join(&name), &data[..len])?;
            (SnapshotKind::Prefix, name)
        };
        Ok(JournalEntry {
            original_path: path.to_owned(),
            new_path: new_path.to_owned(),
            snapshot: name,
            kind,
            recording_id,
            release_id,
            done: false,
        })
    }

    pub fn append(&self, entry: &JournalEntry) -> Result<(), Error> {
        let mut f = OpenOptions::new().create(true).append(true).open(&self.file)?;
        writeln!(f, "{}", serde_json::to_string(entry)?)?;
        f.flush()?;
        Ok(())
    }

    /// Latest state of each entry (the file is append-only)
    pub fn entries(dir: &Path) -> Result<Vec<JournalEntry>, Error> {
        let text = std::fs::read_to_string(dir.join("journal.jsonl"))?;
        let mut map: Vec<JournalEntry> = vec![];
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let e: JournalEntry = serde_json::from_str(line)?;
            match map.iter_mut().find(|x| x.snapshot == e.snapshot) {
                Some(x) => *x = e,
                None => map.push(e),
            }
        }
        Ok(map)
    }
}

/// Put the saved metadata back into `path`
pub fn restore_snapshot(dir: &Path, entry: &JournalEntry, path: &Path) -> Result<(), Error> {
    let snap = dir.join(&entry.snapshot);
    match entry.kind {
        SnapshotKind::Prefix => {
            let prefix = std::fs::read(&snap)?;
            let data = std::fs::read(path)?;
            let current = metadata_len(path, &data)?;
            let tmp = path.with_extension("onetagger-undo");
            {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(&prefix)?;
                f.write_all(&data[current..])?;
                f.flush()?;
            }
            std::fs::rename(&tmp, path)?;
        },
        SnapshotKind::Mp4 => {
            let items: Vec<Mp4Item> = serde_json::from_slice(&std::fs::read(&snap)?)?;
            let mut tag = mp4ameta::Tag::read_from_path(path)?;
            tag.clear();
            for i in items {
                let bytes = unb64(&i.data)?;
                let data = match i.kind.as_str() {
                    "utf8" => Data::Utf8(String::from_utf8(bytes)?),
                    "utf16" => Data::Utf16(String::from_utf8(bytes)?),
                    "jpeg" => Data::Jpeg(bytes),
                    "png" => Data::Png(bytes),
                    "bmp" => Data::Bmp(bytes),
                    "besigned" => Data::BeSigned(bytes),
                    "reserved" => Data::Reserved(bytes),
                    _ => Data::Unknown { code: i.code, data: bytes },
                };
                let ident = match (i.fourcc, i.mean, i.name) {
                    (Some(f), _, _) => DataIdent::fourcc(f),
                    (None, Some(mean), Some(name)) => DataIdent::freeform(mean, name),
                    _ => continue,
                };
                tag.add_data(ident, data);
            }
            tag.write_to_path(path)?;
        }
    }
    Ok(())
}

/// Move across folders / volumes
pub fn move_file(from: &Path, to: &Path) -> Result<(), Error> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_err() {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)?;
    }
    Ok(())
}

/// Undo a whole run, newest first. Returns (restored, errors)
pub fn undo(dir: &Path) -> Result<(usize, Vec<String>), Error> {
    let mut ok = 0;
    let mut errors = vec![];
    let mut entries = Journal::entries(dir)?;
    entries.reverse();
    for e in entries {
        // Not finished entries may have been written but not moved
        let current = if e.new_path.exists() { e.new_path.clone() } else { e.original_path.clone() };
        if !current.exists() {
            errors.push(format!("Datei fehlt: {:?}", e.new_path));
            continue;
        }
        let r = (|| -> Result<(), Error> {
            if current != e.original_path {
                if e.original_path.exists() {
                    return Err(anyhow!("Ursprungspfad belegt: {:?}", e.original_path));
                }
                move_file(&current, &e.original_path)?;
            }
            restore_snapshot(dir, &e, &e.original_path)
        })();
        match r {
            Ok(_) => ok += 1,
            Err(err) => errors.push(format!("{:?}: {err}", e.original_path)),
        }
    }
    Ok((ok, errors))
}
