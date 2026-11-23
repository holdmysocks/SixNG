//! Research: every material preset (`f795e8ba`) of an archive's meta
//! resources as one JSON line: resource offset, preset name hash, slot,
//! asset GUID (`41acf8eb`), albedo, tint bytes, palette, drawable tags.
//! presetguids <packs.rpf> <index json> <out.jsonl>
use rayon::prelude::*;
use sixng_core::{meta7a::Meta, resource::Resource, scan};
use std::io::Write;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::Path::new(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let k = |h: u32| sixng_core::meta7a::field_name(h);
    let rows: Vec<Vec<serde_json::Value>> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a")
        .filter_map(|e| {
            let r = Resource::load_meta(path, e).ok()?;
            let m = Meta::parse(&r)?;
            let mut v = Vec::new();
            for o in m.objects(0xf795_e8ba) {
                let Some(p) = m.object(0xf795_e8ba, o, 2) else { continue };
                let base = p[k(0xebac_f18c).as_str()].as_array().into_iter().flatten().next().map(|x| x[k(0xe7cb_9e1b).as_str()].clone()).unwrap_or_default();
                v.push(serde_json::json!({
                    "res": format!("{:x}", e.off), "name": p[k(0x6512_1749).as_str()], "slot": p[k(0xb536_67be).as_str()],
                    "guid": base[k(0x41ac_f8eb).as_str()], "albedo": base[k(0xead7_33fc).as_str()],
                    "tint": [p[k(0xc4d7_5d79).as_str()].clone(), p[k(0xf4fb_3dec).as_str()].clone(), p[k(0xab5d_2a85).as_str()].clone()], "palette": p[k(0x5ecf_9331).as_str()],
                    "tags": p[k(0x12f8_5293).as_str()],
                }));
            }
            Some(v)
        })
        .collect();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[3])?);
    let mut n = 0;
    for r in rows.into_iter().flatten() {
        writeln!(out, "{r}")?;
        n += 1;
    }
    println!("{n} presets");
    Ok(())
}
