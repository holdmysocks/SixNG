//! Research: where the animations of a scene's unjoined clip windows are.
//! Usage: gapprobe <archive path> <index json> <catalog json> <scene> [<scene> ...]
//! Prints each scene's pending windows (clip, key, window), then every clip
//! dictionary of the archive whose map holds one of those keys or whose
//! name mentions one of the scenes' stems.
use rayon::prelude::*;
use sixng_core::{anim, cutscene, resource::Resource, scan};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let archive = std::path::PathBuf::from(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let cat: cutscene::Catalog = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let mut want: HashMap<u32, String> = HashMap::new();
    let mut stems: Vec<String> = Vec::new();
    for scene in &a[4..] {
        let sd = cat.dicts.iter().find(|d| d.name.eq_ignore_ascii_case(&format!("cin@{scene}"))).ok_or_else(|| anyhow::anyhow!("no {scene}"))?;
        let e = ix.entries.iter().find(|e| e.id == sd.id).unwrap();
        let d = anim::ClipDictionary::parse(&anim::dictionary_bytes(&archive, e)?)?;
        println!("== {} ({}), {} sections: {:?}", sd.name, sd.id, sd.sections, sd.section_ids);
        for c in &d.clips {
            for p in &c.pending {
                println!("  {} sections={} dur={:.3} key={:08x} off={:.3} [{:.3},{:.3}] rate={}", c.short_name(), c.sections, c.duration, p.animation, p.offset, p.start, p.end, p.rate);
                want.insert(p.animation, format!("{}/{}", sd.name, c.short_name()));
            }
            for b in &c.bindings {
                println!("  {} bound off={:.3} [{:.3},{:.3}] {}", c.short_name(), b.offset, b.start, b.end, d.animations[b.animation].name);
            }
        }
        // the scene name up to its shot suffix, to find related dictionaries
        let stem = scene.split("_p").next().unwrap_or(scene).to_string();
        stems.push(stem);
    }
    println!("stems {stems:?}");
    let hits: Vec<String> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == sixng_core::classify::Kind::RageObj && !e.streams.is_empty())
        .filter_map(|e| {
            let r = Resource::load_sys(&archive, e).ok()?;
            let n = anim::dictionary_name(&r.sys)?;
            let keys = anim::map_keys(&r.sys);
            let found: Vec<String> = keys.iter().filter_map(|k| want.get(k).map(|w| format!("{k:08x} for {w}"))).collect();
            let named = stems.iter().any(|s| n.contains(s.as_str()));
            (!found.is_empty() || named).then(|| format!("{} {} keys={} found={:?}", e.id, n, keys.len(), found))
        })
        .collect();
    let mut hits = hits;
    hits.sort();
    for h in hits {
        println!("{h}");
    }
    Ok(())
}
