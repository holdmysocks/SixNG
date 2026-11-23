//! Research: strand roots of every groom chain head (LOD 0), to fit grooms
//! to skin meshes. One JSON line per groom: offset, strands, points per
//! strand, bone hashes, mean strand length and up to 128 root points
//! (RAGE space, Z-up metres).
//! groomroots <packs.rpf> <index json> <out.jsonl>
use rayon::prelude::*;
use sixng_core::{classify::Kind, groom, resource::Resource, scan};
use std::io::Write;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::Path::new(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let cands: Vec<&scan::Entry> = ix.entries.iter().filter(|e| e.name.is_none() && e.kind == Kind::RageObj).collect();
    let mut v: Vec<(u64, serde_json::Value, Vec<u32>, usize)> = cands
        .par_iter()
        .filter_map(|e| {
            let s = Resource::load_sys(path, e).ok()?;
            if !groom::is_groom(&s.sys) {
                return None;
            }
            let g = groom::parse(&Resource::load(path, e).ok()?).ok()?;
            let k = g.points_per_strand.max(1);
            let step = (g.strands / 128).max(1);
            let roots: Vec<[f32; 3]> = (0..g.strands).step_by(step).filter_map(|s| g.points.get(s * k).copied()).map(|p| p.map(|x| (x * 1000.0).round() / 1000.0)).collect();
            let len = if g.lengths.is_empty() { 0.0 } else { g.lengths.iter().sum::<f32>() / g.lengths.len() as f32 };
            let j = serde_json::json!({"off": format!("{:x}", e.off), "strands": g.strands, "k": k, "nbones": g.bones.len(), "len": len, "box": g.root_box, "box2": g.group_box, "roots": roots});
            Some((e.off, j, g.bones.clone(), g.strands))
        })
        .collect();
    v.sort_by_key(|x| x.0);
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[3])?);
    let (mut n, mut chain) = (0, 0);
    for i in 0..v.len() {
        let lod = i > 0 && v[i].2 == v[i - 1].2 && v[i].3 < v[i - 1].3;
        if lod {
            continue;
        }
        chain += 1;
        let mut j = v[i].1.clone();
        j["chain"] = chain.into();
        j["bones"] = v[i].2.iter().map(|h| format!("{h:08x}")).collect::<Vec<_>>().into();
        writeln!(out, "{j}")?;
        n += 1;
    }
    println!("{n} chain heads of {} grooms", v.len());
    Ok(())
}
