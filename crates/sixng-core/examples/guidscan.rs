//! Research: find known 64-bit values (hex, one per line) inside the system
//! stream of named drawables. guidscan <archive> <index json> <values.txt> [name regex]
use sixng_core::{resource::Resource, scan};
use std::collections::HashSet;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let set: HashSet<u64> = std::fs::read_to_string(&a[3])?.lines().filter_map(|l| u64::from_str_radix(l.trim(), 16).ok()).filter(|&v| v != 0).collect();
    let re = regex::Regex::new(a.get(4).map(|s| s.as_str()).unwrap_or("."))?;
    let all = std::env::var("ALL").is_ok();
    for e in ix.entries.iter() {
        let name = e.name.clone().unwrap_or_default();
        if !(all || (!name.is_empty() && re.is_match(&name))) {
            continue;
        }
        let Ok(r) = Resource::load_sys(arc, e) else { continue };
        let mut hits = Vec::new();
        for o in (0..r.sys.len().saturating_sub(7)).step_by(4) {
            let v = u64::from_le_bytes(r.sys[o..o + 8].try_into().unwrap());
            if set.contains(&v) {
                hits.push(format!("{o:x}:{v:016x}"));
            }
        }
        if !hits.is_empty() || !all {
            println!("{:x}\t{}\t{}\t{}\t{}", e.off, e.sub, name, r.sys.len(), hits.join(" "));
        }
    }
    Ok(())
}
