//! Write verified tags (zero trust: old tags are wiped) and move the file
use anyhow::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use onetagger_tag::{CoverType, Field, Tag, TagDate, TagSeparators};
use crate::journal::{self, Journal};
use crate::musicbrainz;
use crate::types::IdentifyResult;

pub const DEFAULT_TEMPLATE: &str = "%albumartist%/%album% (%year%)/%track% - %title%";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfig {
    pub target: PathBuf,
    pub template: String,
}

/// Everything we write, taken from MusicBrainz only
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackTags {
    pub title: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub date: Option<String>,
    pub original_date: Option<String>,
    pub track: u32,
    pub track_total: u32,
    pub disc: u32,
    pub disc_total: u32,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub isrc: Option<String>,
    pub genres: Vec<String>,
    pub release_type: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub recording_id: String,
    pub release_id: String,
    pub release_group_id: String,
    pub release_track_id: String,
    pub artist_ids: Vec<String>,
    pub album_artist_ids: Vec<String>,
}

fn s(v: &Value) -> Option<String> {
    // MusicBrainz uses "[none]" for "no catalog number" etc.
    v.as_str().filter(|x| !x.is_empty() && !x.starts_with('[')).map(String::from)
}

fn ids(credit: &Value) -> Vec<String> {
    credit.as_array().map(|a| a.iter().filter_map(|c| s(&c["artist"]["id"])).collect()).unwrap_or_default()
}

fn top_genres(v: &Value) -> Vec<String> {
    let mut g: Vec<(i64, String)> = v.as_array().map(|a| a.iter()
        .filter_map(|x| Some((x["count"].as_i64().unwrap_or(0), s(&x["name"])?))).collect()).unwrap_or_default();
    g.sort_by(|a, b| b.0.cmp(&a.0));
    g.into_iter().take(3).map(|x| x.1).collect()
}

/// Build tags for `recording_id` on release `release_id`
pub fn track_tags(release_id: &str, recording_id: &str, prefer: (Option<u32>, Option<u32>)) -> Result<TrackTags, Error> {
    let rel = musicbrainz::release(release_id)?.ok_or(anyhow!("Release {release_id} not found on MusicBrainz"))?;
    let media = rel["media"].as_array().cloned().unwrap_or_default();
    let mut found: Option<(Value, Value)> = None;
    for m in &media {
        for t in m["tracks"].as_array().cloned().unwrap_or_default() {
            if t["recording"]["id"].as_str() == Some(recording_id) {
                let exact = prefer.0 == m["position"].as_u64().map(|x| x as u32) && prefer.1 == t["position"].as_u64().map(|x| x as u32);
                if found.is_none() || exact {
                    found = Some((m.clone(), t.clone()));
                }
            }
        }
    }
    let (medium, track) = found.ok_or(anyhow!("Recording not on release {release_id}"))?;
    let rg = &rel["release-group"];
    let label = rel["label-info"].as_array().and_then(|l| l.iter().find(|x| x["label"].is_object()).cloned());
    let mut genres = top_genres(&rel["genres"]);
    if genres.is_empty() {
        genres = top_genres(&rg["genres"]);
    }
    Ok(TrackTags {
        title: s(&track["title"]).unwrap_or_default(),
        artist: musicbrainz::artist_credit(&track["artist-credit"]),
        artists: musicbrainz::artist_names(&track["artist-credit"]),
        album: s(&rel["title"]).unwrap_or_default(),
        album_artist: musicbrainz::artist_credit(&rel["artist-credit"]),
        date: s(&rel["date"]),
        original_date: s(&rg["first-release-date"]),
        track: track["position"].as_u64().unwrap_or(0) as u32,
        track_total: medium["track-count"].as_u64().unwrap_or(0) as u32,
        disc: medium["position"].as_u64().unwrap_or(1) as u32,
        disc_total: media.len() as u32,
        label: label.as_ref().and_then(|l| s(&l["label"]["name"])),
        catalog_number: label.as_ref().and_then(|l| s(&l["catalog-number"])),
        isrc: track["recording"]["isrcs"].as_array().and_then(|i| i.first()).and_then(s),
        genres,
        release_type: s(&rg["primary-type"]).map(|t| t.to_lowercase()),
        country: s(&rel["country"]),
        barcode: s(&rel["barcode"]),
        recording_id: recording_id.to_string(),
        release_id: release_id.to_string(),
        release_group_id: s(&rg["id"]).unwrap_or_default(),
        release_track_id: s(&track["id"]).unwrap_or_default(),
        artist_ids: ids(&track["artist-credit"]),
        album_artist_ids: ids(&rel["artist-credit"]),
    })
}

/// Windows safe path component
fn sanitize(input: &str) -> String {
    let mut out: String = input.chars().map(|c| match c {
        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
        c if c.is_control() => '_',
        c => c,
    }).collect();
    out = out.trim().trim_end_matches('.').trim().to_string();
    // Keep paths below MAX_PATH
    if out.chars().count() > 100 {
        out = out.chars().take(100).collect::<String>().trim().to_string();
    }
    if out.is_empty() { "_".into() } else { out }
}

/// Destination path from template
pub fn destination(config: &ApplyConfig, tags: &TrackTags, source: &Path) -> PathBuf {
    // Year of the first release, not of a reissue
    let year = tags.original_date.clone().or(tags.date.clone()).unwrap_or_default().chars().take(4).collect::<String>();
    let track = if tags.disc_total > 1 { format!("{}-{:02}", tags.disc, tags.track) } else { format!("{:02}", tags.track) };
    let rendered = config.template
        .replace("%albumartist%", &tags.album_artist)
        .replace("%artist%", &tags.artist)
        .replace("%album%", &tags.album)
        .replace("%title%", &tags.title)
        .replace("%year%", &year)
        .replace("%track%", &track)
        .replace(" ()", "");
    let mut path = config.target.clone();
    let parts: Vec<&str> = rendered.split(['/', '\\']).collect();
    for (i, part) in parts.iter().enumerate() {
        let part = sanitize(part);
        if i + 1 == parts.len() {
            let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            path.push(format!("{part}.{ext}"));
        } else {
            path.push(part);
        }
    }
    path
}

/// Audio derived values that survive the wipe
const KEEP: [&str; 6] = ["REPLAYGAIN_TRACK_GAIN", "REPLAYGAIN_TRACK_PEAK", "ENCODER", "TSSE", "TENC", "\u{a9}too"];

/// Remove all metadata except audio derived values, returns what was kept
fn wipe(path: &Path) -> Result<Vec<(String, Vec<String>)>, Error> {
    let kept: Vec<(String, Vec<String>)> = match Tag::load_file(path, false) {
        Ok(t) => KEEP.iter().filter_map(|k| t.tag().get_raw(k).map(|v| (k.to_string(), v))).collect(),
        Err(_) => vec![],
    };
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "flac" => {
            let mut tag = metaflac::Tag::read_from_path(path)?;
            tag.remove_blocks(metaflac::BlockType::VorbisComment);
            tag.remove_blocks(metaflac::BlockType::Picture);
            tag.save()?;
        },
        "m4a" | "mp4" => {
            let mut tag = mp4ameta::Tag::read_from_path(path)?;
            tag.clear();
            tag.write_to_path(path)?;
        },
        _ => {
            // Cut the ID3v2 block off
            let data = std::fs::read(path)?;
            let len = journal::metadata_len(path, &data)?;
            if len > 0 {
                let tmp = path.with_extension("onetagger-tmp");
                {
                    let mut f = std::fs::File::create(&tmp)?;
                    f.write_all(&data[len..])?;
                    f.flush()?;
                }
                std::fs::rename(&tmp, path)?;
            }
        }
    }
    Ok(kept)
}

fn tag_date(d: &str) -> Option<TagDate> {
    let mut p = d.split('-');
    Some(TagDate {
        year: p.next()?.parse().ok()?,
        month: p.next().and_then(|x| x.parse().ok()),
        day: p.next().and_then(|x| x.parse().ok()),
    })
}

/// Write tags in place (after wipe)
pub fn write_tags(path: &Path, tags: &TrackTags, cover: Option<Vec<u8>>, kept: Vec<(String, Vec<String>)>) -> Result<(), Error> {
    let mut wrap = Tag::load_file(path, true)?;
    wrap.set_separators(&TagSeparators { id3: "; ".into(), vorbis: None, mp4: "; ".into() });
    let format = wrap.format();
    let is_id3 = matches!(wrap, Tag::ID3(_));
    let is_vorbis = matches!(wrap, Tag::FLAC(_) | Tag::Vorbis(_));
    if let Tag::ID3(t) = &mut wrap {
        t.set_id3v24(true);
    }
    // Picard style custom keys per format
    let key = |vorbis: &str, id3: &str| -> String {
        match &wrap {
            Tag::FLAC(_) | Tag::Vorbis(_) => vorbis.to_string(),
            Tag::ID3(_) => id3.to_string(),
            Tag::MP4(_) => format!("com.apple.iTunes:{id3}"),
        }
    };
    let custom: Vec<(String, Vec<String>)> = vec![
        (key("MUSICBRAINZ_TRACKID", "MusicBrainz Track Id"), vec![tags.recording_id.clone()]),
        (key("MUSICBRAINZ_ALBUMID", "MusicBrainz Album Id"), vec![tags.release_id.clone()]),
        (key("MUSICBRAINZ_RELEASEGROUPID", "MusicBrainz Release Group Id"), vec![tags.release_group_id.clone()]),
        (key("MUSICBRAINZ_RELEASETRACKID", "MusicBrainz Release Track Id"), vec![tags.release_track_id.clone()]),
        (key("MUSICBRAINZ_ARTISTID", "MusicBrainz Artist Id"), tags.artist_ids.clone()),
        (key("MUSICBRAINZ_ALBUMARTISTID", "MusicBrainz Album Artist Id"), tags.album_artist_ids.clone()),
        (key("ARTISTS", "ARTISTS"), tags.artists.clone()),
        (key("RELEASETYPE", "MusicBrainz Album Type"), tags.release_type.clone().into_iter().collect()),
        (key("RELEASECOUNTRY", "MusicBrainz Album Release Country"), tags.country.clone().into_iter().collect()),
        (key("BARCODE", "BARCODE"), tags.barcode.clone().into_iter().collect()),
        (key("DISCTOTAL", "DISCTOTAL"), vec![tags.disc_total.to_string()]),
    ];

    let t = wrap.tag_mut();
    t.set_field(Field::Title, vec![tags.title.clone()], true);
    t.set_field(Field::Artist, vec![tags.artist.clone()], true);
    t.set_field(Field::Album, vec![tags.album.clone()], true);
    t.set_field(Field::AlbumArtist, vec![tags.album_artist.clone()], true);
    if let Some(d) = tags.date.as_deref().and_then(tag_date) {
        t.set_date(&d, true);
    }
    if let Some(d) = &tags.original_date {
        if is_id3 {
            t.set_raw("TDOR", vec![d.clone()], true);
        } else if let Some(d) = tag_date(d) {
            t.set_publish_date(&d, true);
        }
    }
    t.set_track_number(&tags.track.to_string(), Some(tags.track_total as u16), true);
    t.set_field(Field::DiscNumber, vec![tags.disc.to_string()], true);
    if let Some(l) = &tags.label {
        t.set_field(Field::Label, vec![l.clone()], true);
        if is_vorbis {
            // Picard writes LABEL, OneTagger's Field::Label is PUBLISHER
            t.set_raw("LABEL", vec![l.clone()], true);
        }
    }
    if let Some(c) = &tags.catalog_number { t.set_field(Field::CatalogNumber, vec![c.clone()], true); }
    if let Some(i) = &tags.isrc { t.set_field(Field::ISRC, vec![i.clone()], true); }
    if !tags.genres.is_empty() { t.set_field(Field::Genre, tags.genres.clone(), true); }
    for (k, v) in custom {
        if !v.is_empty() && v.iter().any(|x| !x.is_empty()) {
            t.set_raw(&k, v, true);
        }
    }
    for (k, v) in kept {
        t.set_raw(&k, v, true);
    }
    if let Some(cover) = cover {
        t.set_art(CoverType::CoverFront, "image/jpeg", Some("Cover"), cover);
    }
    t.save_file(path)?;
    debug!("Wrote {format:?} tags to {path:?}");
    Ok(())
}

/// Apply a confirmed result: journal, wipe, write, move. Returns the new path
pub fn apply(result: &IdentifyResult, config: &ApplyConfig, journal: &Journal) -> Result<PathBuf, Error> {
    let release = result.chosen_release().ok_or(anyhow!("No release"))?;
    let recording = &release.recording_id;
    let tags = track_tags(&release.release_id, recording, (release.disc, release.track))?;
    let dest = destination(config, &tags, &result.path);
    if dest.exists() {
        return Err(anyhow!("Ziel existiert bereits: {}", dest.display()));
    }
    let cover = musicbrainz::front_cover(&release.release_id).unwrap_or(None);

    let mut entry = journal.snapshot(&result.path, &dest, Some(recording.clone()), Some(release.release_id.clone()))?;
    journal.append(&entry)?;

    let kept = wipe(&result.path)?;
    write_tags(&result.path, &tags, cover, kept)?;
    journal::move_file(&result.path, &dest)?;

    entry.done = true;
    journal.append(&entry)?;
    Ok(dest)
}
