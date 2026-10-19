//! Text normalization and comparison for titles / artists / album names
use regex::Regex;
use strsim::normalized_levenshtein;
use unidecode::unidecode;

lazy_static::lazy_static! {
    static ref FEAT: Regex = Regex::new(r"(?i)[\(\[]?\b(feat|ft|featuring)\b\.?.*?([\)\]]|$)").unwrap();
    static ref FEAT_NAMES: Regex = Regex::new(r"(?i)\b(?:feat|ft|featuring)\b\.?\s+([^\)\]]+)").unwrap();
    static ref LEADING_THE: Regex = Regex::new(r"^the\s+").unwrap();
    static ref BRACKETS: Regex = Regex::new(r"[\(\[][^\)\]]*[\)\]]").unwrap();
    /// "Title -JP ver.-" style suffix
    static ref DASH_SUFFIX: Regex = Regex::new(r"\s-[^-]+-\s*$").unwrap();
    static ref NON_ALNUM: Regex = Regex::new(r"[^a-z0-9 ]").unwrap();
    static ref SPACES: Regex = Regex::new(r"\s+").unwrap();
    /// Edition noise in album titles
    static ref TRAILING_NUMBER: Regex = Regex::new(r"(\s(vol|volume|pt|part))?\s\d+$").unwrap();
    static ref RELEASE_SUFFIX: Regex = Regex::new(r"(?i)\s-\s(ep|single)\s*$").unwrap();
    static ref EDITION: Regex = Regex::new(r"(?i)[\(\[][^\)\]]*(edition|version|remaster|deluxe|expanded|anniversary|bonus|special|explicit|clean|live|extended|tracks|soundtrack)[^\)\]]*[\)\]]").unwrap();
}

/// Versions that make a recording a different recording
const HARD_VERSIONS: [&str; 10] = ["live", "remix", "acoustic", "instrumental", "demo", "a cappella", "acapella", "unplugged", "karaoke", "rmx"];

/// Lowercase ascii, no punctuation, single spaces
pub fn normalize(input: &str) -> String {
    let s = unidecode(input).to_lowercase().replace('&', " and ");
    let s = NON_ALNUM.replace_all(&s, " ");
    SPACES.replace_all(s.trim(), " ").to_string()
}

/// Title without feat. and bracketed parts
pub fn core_title(title: &str) -> String {
    let t = FEAT.replace_all(title, "");
    let t = BRACKETS.replace_all(&t, "");
    let t = DASH_SUFFIX.replace_all(&t, "");
    // "Song - Live at X" style suffix
    let t = t.split(" - ").next().unwrap_or(&t).to_string();
    LEADING_THE.replace(&normalize(&t), "").to_string()
}

/// Album title without edition / remaster noise and " - EP" / " - Single" suffixes
pub fn core_album(album: &str) -> String {
    let a = FEAT.replace_all(album, "");
    let a = EDITION.replace_all(&a, "");
    normalize(&RELEASE_SUFFIX.replace_all(&a, ""))
}

/// Album title for a MusicBrainz search: without edition brackets, feat. and " - Single" / " - EP"
pub fn album_for_search(album: &str) -> String {
    let a = FEAT.replace_all(album, "");
    let a = EDITION.replace_all(&a, "");
    RELEASE_SUFFIX.replace_all(&a, "").trim().to_string()
}

/// Artists mentioned as "feat. X" in a title
pub fn featured_artists(title: &str) -> Vec<String> {
    FEAT_NAMES.captures_iter(title).filter_map(|c| c.get(1)).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Shorter forms of an album title: parts around ':' / ',' and without a trailing volume / number
fn album_variants(album: &str) -> Vec<String> {
    let mut out = vec![];
    let cleaned = EDITION.replace_all(&FEAT.replace_all(album, ""), "").to_string();
    for sep in [':', ','] {
        if let Some((a, b)) = cleaned.split_once(sep) {
            out.push(core_album(a));
            out.push(core_album(b));
        }
    }
    let core = core_album(&cleaned);
    out.push(TRAILING_NUMBER.replace(&core, "").trim().to_string());
    out.retain(|v| v.len() >= 4 && *v != core);
    out
}

/// Same album despite naming variants: editions, "Vol. 1", "Album: Subtitle",
/// "Label presents: Album", double A-side singles ("A / B")
pub fn album_matches(shazam_album: &str, release_title: &str) -> bool {
    let rel = core_album(release_title);
    if rel.is_empty() {
        return false;
    }
    let shazam_album = RELEASE_SUFFIX.replace_all(shazam_album, "").to_string();
    for part in shazam_album.split(" / ") {
        let a = core_album(part);
        if a.is_empty() {
            continue;
        }
        if a == rel || normalized_levenshtein(&a, &rel) >= 0.9 {
            return true;
        }
        // "Album: Subtitle", "Album, Vol. 1", "Label: Album", "Album 30" (anniversary)
        if album_variants(part).contains(&rel) || album_variants(release_title).contains(&a) {
            return true;
        }
    }
    false
}

/// Hard version markers found in a title (live, remix, ...), incl. remixer name for remixes
pub fn hard_versions(title: &str) -> Vec<String> {
    let lower = unidecode(title).to_lowercase();
    let mut out = vec![];
    for v in HARD_VERSIONS {
        if Regex::new(&format!(r"\b{}\b", regex::escape(v))).unwrap().is_match(&lower) {
            out.push(if v == "rmx" { "remix".to_string() } else { v.to_string() });
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    normalized_levenshtein(&a, &b)
}

pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (core_title(a), core_title(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    normalized_levenshtein(&a, &b)
}

/// Artist similarity tolerant to separators and partial credits ("A & B" vs "A")
pub fn artist_similarity(a: &str, b: &str) -> f64 {
    let split = |s: &str| -> Vec<String> {
        let s = FEAT.replace_all(s, " , ");
        Regex::new(r"(?i)\s*(,|&|\band\b|\bx\b|\bvs\.?\b|/|;|\bwith\b)\s*").unwrap()
            .split(&s).map(normalize).filter(|x| !x.is_empty()).collect()
    };
    let (sa, sb) = (split(a), split(b));
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    // Best pairwise match of any credited artist
    let mut best: f64 = 0.0;
    let contains_words = |long: &str, short: &str| short.len() >= 4 && format!(" {long} ").contains(&format!(" {short} "));
    for x in &sa {
        for y in &sb {
            let s = if x == y {
                1.0
            } else if contains_words(x, y) || contains_words(y, x) {
                // "The Diabolical Biz Markie" / "Biz Markie"
                0.9
            } else {
                normalized_levenshtein(x, y)
            };
            best = best.max(s);
        }
    }
    best.max(similarity(a, b))
}

pub fn album_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (core_album(a), core_album(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    normalized_levenshtein(&a, &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(core_title("Dance with My Ghost (feat. Elderbrook)"), "dance with my ghost");
        assert_eq!(core_title("Bad Boy (radio mix)"), "bad boy");
        assert_eq!(hard_versions("Battery (live)"), vec!["live"]);
        assert_eq!(hard_versions("Low (Sondr Remix)"), vec!["remix"]);
        assert!(hard_versions("Bad Boy (radio mix)").is_empty());
        assert!(title_similarity("Message to B.A.", "Message To B.A.") > 0.99);
        assert_eq!(core_title("STAY -JP ver.-"), "stay");
        assert_eq!(core_title("The Two"), core_title("Two"));
        assert_eq!(featured_artists("Crush (Las Salinas Remix) [feat. Second Sun]"), vec!["Second Sun"]);
        assert_eq!(album_for_search("Go Mental (feat. Big Narstie) - Single"), "Go Mental");
    }

    #[test]
    fn artists() {
        assert!(artist_similarity("N.W.A", "N.W.A.") > 0.99);
        assert!(artist_similarity("Outbreak & Kronos", "Outbreak") > 0.99);
        assert!(artist_similarity("Cascada", "John Lee Hooker") < 0.5);
        assert!(artist_similarity("The Diabolical Biz Markie", "Biz Markie") >= 0.9);
    }

    #[test]
    fn albums() {
        assert!(album_similarity("Everytime We Touch (Premium Edition)", "Everytime We Touch") > 0.99);
        assert!(album_similarity("Please Insert Coin - EP", "Please Insert Coin") > 0.99);
        assert!(album_matches("Till I Die, Vol. 1", "Till I Die"));
        assert!(album_matches("Live Shit: Binge & Purge (Live In Mexico City)", "Live Shit: Binge & Purge"));
        assert!(album_matches("Go Mental (feat. Big Narstie & Atlantic Horns) - Single", "Go Mental"));
        assert!(album_matches("Aftermath / the Gift - Single", "Aftermath"));
        assert!(album_matches("Double Vision: Then and Now", "Double Vision"));
        assert!(album_matches("U Know What I'm Throwin Up", "DPGC: U Know What I'm Throwin' Up"));
        assert!(!album_matches("19", "Adele Live at the Royal Albert Hall"));
        assert!(!album_matches("Master of Puppets (Remastered)", "S&M"));
        assert!(!album_matches("W.A.S.P.", "The Headless Children"));
        assert!(!album_matches("Summer Club", "Summer"));
        assert!(album_matches("Turbo 30", "Turbo"));
        assert!(album_matches("Hercules (Original Motion Picture Soundtrack)", "Hercules: An Original Walt Disney Records Soundtrack"));
    }
}
