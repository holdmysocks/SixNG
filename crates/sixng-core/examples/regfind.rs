//! Research: the asset-registry records (PSO files, struct 20e0f6ed) of
//! given asset GUIDs, printed in full, and a count of records by type.
//! Usage: regfind <archive> <index json> <guid hex>...
use rayon::prelude::*;
use sixng_core::{classify::Kind, scan};
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let want: Vec<String> = a[3..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let out: Vec<(Vec<String>, std::collections::BTreeMap<i64, usize>)> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == Kind::Pso)
        .filter_map(|e| {
            let b = scan::read_entry(arc, e).ok()?;
            if !b.windows(4).any(|x| x == 0x20e0_f6edu32.to_le_bytes() || x == 0x20e0_f6edu32.to_be_bytes()) {
                return None;
            }
            let j = sixng_core::pso::parse(&b)?.to_json();
            let (mut hits, mut types) = (Vec::new(), std::collections::BTreeMap::new());
            for objs in j.get("objects")?.as_object()?.values() {
                for o in objs.as_array().into_iter().flatten() {
                    for rec in o.as_object().into_iter().flat_map(|m| m.values()).filter_map(|v| v.as_array()).flatten() {
                        let (Some(key), Some(item)) = (rec.get("Key").and_then(|k| k.as_str()), rec.get("Item")) else { continue };
                        if item.get("_struct").and_then(|s| s.as_str()) != Some("20e0f6ed") {
                            continue;
                        }
                        *types.entry(item.get("2d4958cf").and_then(|t| t.as_i64()).unwrap_or(-1)).or_default() += 1;
                        if want.iter().any(|w| key.eq_ignore_ascii_case(w)) {
                            hits.push(format!("{} in {}: {}", key, e.id, item));
                        }
                    }
                }
            }
            Some((hits, types))
        })
        .collect();
    let mut types = std::collections::BTreeMap::new();
    for (hits, t) in out {
        hits.iter().for_each(|h| println!("{h}"));
        t.into_iter().for_each(|(k, n)| *types.entry(k).or_insert(0) += n);
    }
    println!("records by type: {types:?}");
    Ok(())
}
