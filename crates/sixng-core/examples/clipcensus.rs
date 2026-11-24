//! Research: decode every n-th clip dictionary of anim.rpf and count what
//! decodes: dictionaries, clips by type, clips with bindings, animations,
//! tracks, and the errors.
//! Usage: clipcensus <game folder> <work folder> [step]
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (game, work) = (std::path::Path::new(&a[1]), std::path::Path::new(&a[2]));
    let step: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(50);
    let dicts = sixng_core::anim::dictionaries(game, work)?;
    let ix = sixng_core::scan::load_index(&sixng_core::scan::index_path(&work.join("index"), "anim.rpf"))?;
    let arch = game.join("anim.rpf");
    let by_id: std::collections::HashMap<&str, &sixng_core::scan::Entry> = ix.entries.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut n: BTreeMap<String, usize> = BTreeMap::new();
    let mut errs: BTreeMap<String, usize> = BTreeMap::new();
    for d in dicts.iter().step_by(step) {
        *n.entry("dictionaries".into()).or_default() += 1;
        let Some(&e) = by_id.get(d.id.as_str()) else { continue };
        let b = sixng_core::anim::dictionary_bytes(&arch, e)?;
        match sixng_core::anim::ClipDictionary::parse(&b) {
            Ok(cd) => {
                *n.entry("decoded".into()).or_default() += 1;
                *n.entry("animations".into()).or_default() += cd.animations.len();
                *n.entry("tracks".into()).or_default() += cd.animations.iter().map(|a| a.tracks.len()).sum::<usize>();
                for c in &cd.clips {
                    *n.entry(format!("clips type {}", c.kind)).or_default() += 1;
                    if !c.bindings.is_empty() {
                        *n.entry("clips with bindings".into()).or_default() += 1;
                    }
                    if c.sections > 1 {
                        *n.entry("clips with sections".into()).or_default() += 1;
                        if std::env::var_os("CENSUS_VERBOSE").is_some() {
                            eprintln!("{} {} sections={} duration={} offsets={:?}", d.name, c.short_name(), c.sections, c.duration, c.bindings.iter().map(|b| (b.offset, b.start, b.end)).collect::<Vec<_>>());
                        }
                    }
                    if !c.pending.is_empty() {
                        *n.entry("clips with pending windows".into()).or_default() += 1;
                    }
                }
            }
            Err(err) => {
                let m = err.to_string();
                if std::env::var_os("CENSUS_VERBOSE").is_some() {
                    eprintln!("{} {}: {m} ({} bytes, streams {})", d.id, d.name, b.len(), e.streams.len());
                }
                *errs.entry(m.split(" at 0x").next().unwrap_or("").split(" 0x").next().unwrap_or("").to_string()).or_default() += 1
            }
        }
    }
    println!("{}", serde_json::json!({"counts": n, "errors": errs}));
    Ok(())
}
