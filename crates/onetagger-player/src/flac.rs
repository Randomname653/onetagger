use std::path::{PathBuf, Path};
use anyhow::Error;
use std::io::BufReader;
use std::fs::File;
use rodio::{Source, Decoder};
use crate::AudioSource;

pub struct FLACSource {
    path: PathBuf,
    duration: u128
}

impl FLACSource {
    pub fn new(path: impl AsRef<Path>) -> Result<FLACSource, Error> {
        let mut flac = FLACSource {
            path: path.as_ref().to_owned(),
            duration: 0
        };
        // Get duration from decoder, fall back to the STREAMINFO read by lofty
        flac.duration = match flac.get_source()?.total_duration() {
            Some(d) => d.as_millis(),
            None => {
                use lofty::prelude::*;
                lofty::read_from_path(&flac.path)?.properties().duration().as_millis()
            }
        };

        Ok(flac)
    }
}

impl AudioSource for FLACSource {
    // Get duration
    fn duration(&self) -> u128 {
        self.duration
    }

    // Get rodio decoder. Claxon rejects some valid files (e.g. unusual block sizes),
    // Symphonia's FLAC decoder is used as fallback
    fn get_source(&self) -> Result<Box<dyn Source<Item = i16> + Send>, Error> {
        match Decoder::new_flac(BufReader::new(File::open(&self.path)?)) {
            Ok(d) => Ok(Box::new(d)),
            Err(_) => Ok(Box::new(Decoder::new(BufReader::new(File::open(&self.path)?))?)),
        }
    }
}