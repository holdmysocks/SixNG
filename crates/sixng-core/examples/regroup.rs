//! Research: regroup an archive's cached raw streams (`<stem>.segs/`) with
//! the current `group_streams` and list entries that differ from its index.
//! regroup <index dir> [stem...]
use sixng_core::scan::{self, RawStream};
use std::collections::HashMap;
use std::path::Path;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let dir = Path::new(&a[1]);
    let stems: Vec<String> = if a.len() > 2 {
        a[2..].to_vec()
    } else {
        std::fs::read_dir(dir)?.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".segs").map(String::from)).collect()
    };
    for stem in stems {
        let Ok(ix) = scan::load_index(&dir.join(format!("{stem}.json"))) else { continue };
        let tag = format!("_{}_{}.json", ix.size, ix.mtime);
        let mut raw: Vec<RawStream> = vec![];
        for e in std::fs::read_dir(dir.join(format!("{stem}.segs")))?.flatten() {
            if e.file_name().to_string_lossy().ends_with(&tag) {
                raw.extend(serde_json::from_slice::<Vec<RawStream>>(&std::fs::read(e.path())?)?);
            }
        }
        let new = scan::group_streams(&ix.archive, raw);
        let old: HashMap<&str, usize> = ix.entries.iter().map(|e| (e.id.as_str(), e.streams.len())).collect();
        let newm: HashMap<&str, usize> = new.iter().map(|e| (e.id.as_str(), e.streams.len())).collect();
        let changed: Vec<_> = new.iter().filter(|e| old.get(e.id.as_str()) != Some(&e.streams.len())).collect();
        let gone: Vec<_> = ix.entries.iter().filter(|e| !newm.contains_key(e.id.as_str())).collect();
        println!("{stem}: {} -> {} entries, {} changed, {} merged away", ix.entries.len(), new.len(), changed.len(), gone.len());
        for e in changed.iter().take(20) {
            println!("  changed {} streams {} -> {}", e.id, old.get(e.id.as_str()).copied().unwrap_or(0), e.streams.len());
        }
        for e in gone.iter().take(20) {
            println!("  merged away {} {:?} {:?} {:?}", e.id, e.kind, e.rtype, e.name);
        }
    }
    Ok(())
}
