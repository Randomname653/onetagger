#[macro_use] extern crate log;
#[macro_use] extern crate anyhow;

pub mod http;
pub mod fingerprint;
pub mod acoustid;
pub mod musicbrainz;
pub mod matching;
pub mod types;
pub mod consensus;
pub mod release;
pub mod pipeline;
pub mod journal;
pub mod apply;

pub use pipeline::{identify_file, run, IdentifyConfig, STOP};
pub use types::*;
