//! Snapshot an M4A, wipe its tags, restore from journal
use onetagger_identify::journal::{self, Journal};
use std::path::Path;

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let dir = std::env::args().nth(2).expect("journal dir");
    let path = Path::new(&path);
    let j = Journal::create(&dir).unwrap();
    let entry = j.snapshot(path, path, None, None).unwrap();
    let mut tag = mp4ameta::Tag::read_from_path(path).unwrap();
    tag.clear();
    tag.set_title("WIPED");
    tag.write_to_path(path).unwrap();
    println!("wiped, title now: {:?}", mp4ameta::Tag::read_from_path(path).unwrap().title());
    journal::restore_snapshot(Path::new(&dir), &entry, path).unwrap();
    println!("restored, title now: {:?}", mp4ameta::Tag::read_from_path(path).unwrap().title());
}
