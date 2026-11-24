//! Research: per entry of an archive, how many 4-aligned u32 words of its
//! pages are the joaat of a name from a list (one per line).
//! Usage: hashcount <archive path> <index json> <names file>
//! Prints one JSON line per entry with at least one hit: id, kind, the
//! number of hits and the distinct names hit (first 8).
use rayon::prelude::*;
use sixng_core::{meta7a::joaat, resource::Resource, scan};
use std::collections::{HashMap, HashSet};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let archive = std::path::PathBuf::from(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let names: HashMap<u32, String> = std::fs::read_to_string(&a[3])?.lines().filter(|l| !l.is_empty()).map(|l| (joaat(l), l.to_string())).collect();
    let out: Vec<String> = ix
        .entries
        .par_iter()
        .filter_map(|e| {
            let pages = match Resource::load(&archive, e) {
                Ok(r) => vec![r.sys, r.gfx],
                Err(_) => vec![scan::read_entry(&archive, e).ok()?],
            };
            let mut hits = 0usize;
            let mut seen: HashSet<u32> = HashSet::new();
            for p in &pages {
                for w in p.chunks_exact(4) {
                    let v = u32::from_le_bytes(w.try_into().unwrap());
                    if names.contains_key(&v) {
                        hits += 1;
                        seen.insert(v);
                    }
                }
            }
            (hits > 0).then(|| {
                let first: Vec<&String> = seen.iter().take(8).map(|h| &names[h]).collect();
                serde_json::json!({"id": e.id, "kind": format!("{:?}", e.kind), "sub": e.sub, "usize": e.usize, "hits": hits, "distinct": seen.len(), "names": first}).to_string()
            })
        })
        .collect();
    for l in out {
        println!("{l}");
    }
    Ok(())
}
