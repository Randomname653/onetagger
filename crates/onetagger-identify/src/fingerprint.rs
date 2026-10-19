use anyhow::Error;
use base64::Engine;
use std::path::Path;
use onetagger_player::AudioSources;
use onetagger_player::rodio::Source;
use rusty_chromaprint::{Configuration, Fingerprinter, FingerprintCompressor};

/// Same length fpcalc uses by default
const FINGERPRINT_SECONDS: usize = 120;

pub struct Fingerprint {
    /// Compressed, base64 encoded fingerprint as AcoustID expects it
    pub encoded: String,
    /// Full file duration in seconds
    pub duration: u32,
}

/// Calculate Chromaprint fingerprint (fpcalc compatible, algorithm 2)
pub fn fingerprint_file(path: impl AsRef<Path>) -> Result<Fingerprint, Error> {
    let source = AudioSources::from_path(path.as_ref())?;
    let duration = ((source.duration() + 500) / 1000) as u32;
    // Feed native rate/channels: Chromaprint resamples and downmixes itself (like fpcalc),
    // a naive resample beforehand changes the fingerprint enough to miss AcoustID matches
    let decoded = source.get_source()?;
    let rate = decoded.sample_rate();
    let channels = decoded.channels() as u32;
    let samples = decoded
        .take(rate as usize * channels as usize * FINGERPRINT_SECONDS)
        .collect::<Vec<i16>>();
    if samples.len() < rate as usize * channels as usize * 3 {
        return Err(anyhow!("Audio too short to fingerprint"));
    }

    // Some files (e.g. M4A without duration in the header) report 0: ask lofty, else count samples
    let mut duration = duration;
    if duration == 0 {
        use lofty::prelude::*;
        duration = lofty::read_from_path(path.as_ref()).map(|f| f.properties().duration().as_secs() as u32).unwrap_or(0);
        if duration == 0 && samples.len() < rate as usize * channels as usize * FINGERPRINT_SECONDS {
            duration = (samples.len() / (rate as usize * channels as usize)) as u32;
        }
        if duration == 0 {
            return Err(anyhow!("Unknown duration"));
        }
    }

    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    printer.start(rate, channels).map_err(|e| anyhow!("Fingerprinter: {e:?}"))?;
    printer.consume(&samples);
    printer.finish();
    let raw = printer.fingerprint();
    if raw.is_empty() {
        return Err(anyhow!("Empty fingerprint"));
    }
    let compressed = FingerprintCompressor::from(&config).compress(raw);
    Ok(Fingerprint {
        encoded: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed),
        duration,
    })
}
