use onetagger_identify::{fingerprint, acoustid};

fn main() {
    let key = std::env::var("ACOUSTID_KEY").expect("ACOUSTID_KEY");
    for path in std::env::args().skip(1) {
        let fp = fingerprint::fingerprint_file(&path).unwrap();
        let r = acoustid::lookup(&key, &fp).unwrap();
        println!("=== {path} ({}s, {} results)", fp.duration, r.results.len());
        for res in r.results.iter().take(3) {
            println!(" score {:.3} recs {}", res.score, res.recordings.len());
            for rec in res.recordings.iter().take(3) {
                let rels: usize = rec.releasegroups.iter().map(|g| g.releases.len()).sum();
                println!("   {} | {} | {:?}s | {} groups, {} releases", acoustid::artist_credit(&rec.artists), rec.title.clone().unwrap_or_default(), rec.duration, rec.releasegroups.len(), rels);
                for g in rec.releasegroups.iter().take(4) {
                    println!("      [{:?} {:?}] {}", g.kind, g.secondarytypes, g.title.clone().unwrap_or_default());
                }
            }
        }
    }
}
