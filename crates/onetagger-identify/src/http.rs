use anyhow::Error;
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const USER_AGENT: &str = "OneTagger-Identify/0.1 ( https://github.com/Randomname653/onetagger )";

lazy_static::lazy_static! {
    static ref CLIENT: Client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
}

/// Global request spacing for one service (shared across threads)
pub struct Throttle {
    next: Mutex<Instant>,
    interval: Duration,
}

impl Throttle {
    pub fn new(interval: Duration) -> Throttle {
        Throttle { next: Mutex::new(Instant::now()), interval }
    }

    pub fn wait(&self) {
        let wait = {
            let mut next = self.next.lock().unwrap();
            let now = Instant::now();
            let slot = (*next).max(now);
            *next = slot + self.interval;
            slot - now
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// Push back all future requests
    pub fn cooldown(&self, duration: Duration) {
        let mut next = self.next.lock().unwrap();
        *next = (*next).max(Instant::now() + duration);
    }
}

lazy_static::lazy_static! {
    /// MusicBrainz allows 1 request / second
    pub static ref MUSICBRAINZ: Throttle = Throttle::new(Duration::from_millis(1100));
    /// AcoustID allows 3 requests / second
    pub static ref ACOUSTID: Throttle = Throttle::new(Duration::from_millis(350));
    pub static ref COVERART: Throttle = Throttle::new(Duration::from_millis(250));
}

/// Folder for cached API responses
pub fn cache_dir(service: &str) -> PathBuf {
    let base = onetagger_shared::Settings::get_folder().unwrap_or_else(|_| std::env::temp_dir());
    let dir = base.join("identify_cache").join(service);
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Stable hash for cache file names
pub fn fnv64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// GET with throttling, retries and an optional disk cache.
/// Returns None on 404.
pub fn get(url: &str, throttle: &Throttle, cache: Option<&str>) -> Result<Option<Vec<u8>>, Error> {
    let cache_file = cache.map(|c| cache_dir(c).join(format!("{:016x}", fnv64(url.as_bytes()))));
    if let Some(f) = &cache_file {
        if let Ok(data) = std::fs::read(f) {
            return Ok(Some(data));
        }
        if f.with_extension("404").exists() {
            return Ok(None);
        }
    }

    let mut backoff = Duration::from_secs(5);
    for attempt in 0..6 {
        throttle.wait();
        let response = match CLIENT.get(url).send() {
            Ok(r) => r,
            Err(e) => {
                warn!("Request failed ({url}, attempt {}): {e}", attempt + 1);
                throttle.cooldown(backoff);
                backoff *= 2;
                continue;
            }
        };
        let status = response.status();
        if status.as_u16() == 404 {
            if let Some(f) = &cache_file {
                std::fs::write(f.with_extension("404"), b"").ok();
            }
            return Ok(None);
        }
        if status.as_u16() == 429 || status.as_u16() == 503 || status.is_server_error() {
            warn!("{status} from {url}, pausing {}s", backoff.as_secs());
            throttle.cooldown(backoff);
            backoff *= 2;
            continue;
        }
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            return Err(anyhow!("{status} from {url}: {}", text.chars().take(200).collect::<String>()));
        }
        let data = response.bytes()?.to_vec();
        if let Some(f) = &cache_file {
            std::fs::write(f, &data).ok();
        }
        return Ok(Some(data));
    }
    Err(anyhow!("Giving up on {url} after retries"))
}

pub fn get_json<T: serde::de::DeserializeOwned>(url: &str, throttle: &Throttle, cache: Option<&str>) -> Result<Option<T>, Error> {
    match get(url, throttle, cache)? {
        Some(data) => Ok(Some(serde_json::from_slice(&data)
            .map_err(|e| anyhow!("Invalid JSON from {url}: {e}"))?)),
        None => Ok(None),
    }
}
