//! Research: per groom piece (furlink::pieces, archive order) strand
//! statistics of its densest LOD, as JSON lines: lods, strands, points per
//! strand, mean / max strand length, curl (path over chord), root centroid,
//! root box, bone count, up to 24 sampled roots.
//! hairstats <packs.rpf> <index json> <grooms catalog json> <out>
use rayon::prelude::*;
use sixng_core::resource::Resource;
use std::io::Write;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = sixng_core::scan::load_index(std::path::Path::new(&a[2]))?;
    let cat: Vec<sixng_core::groom::GroomInfo> = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let pcs = sixng_core::furlink::pieces(&cat);
    let by: std::collections::HashMap<u64, &sixng_core::scan::Entry> = ix.entries.iter().map(|e| (e.off, e)).collect();
    let info: std::collections::HashMap<u64, &sixng_core::groom::GroomInfo> = cat.iter().map(|g| (g.off, g)).collect();
    let lines: Vec<String> = pcs
        .par_iter()
        .map(|p| {
            let gi = info[&p.lods[0]];
            let mut v = serde_json::json!({"lods": p.lods, "strands": p.strands, "root_box": p.root_box, "bones": gi.bones.len(), "category": gi.category});
            let Some(g) = by.get(&p.lods[0]).and_then(|e| Resource::load(arc, e).ok()).and_then(|r| sixng_core::groom::parse(&r).ok()) else { return v.to_string() };
            let k = g.points_per_strand.max(1);
            let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            let (mut lens, mut curl) = (Vec::new(), Vec::new());
            let mut c = [0f64; 3];
            let mut tip = [0f64; 3];
            for s in 0..g.strands {
                let pts = &g.points[s * k..(s + 1) * k];
                let path: f32 = pts.windows(2).map(|w| d(w[0], w[1])).sum();
                let chord = d(pts[0], pts[k - 1]);
                lens.push(path);
                if chord > 1e-5 {
                    curl.push(path / chord);
                }
                for q in 0..3 {
                    c[q] += pts[0][q] as f64;
                    tip[q] += pts[k - 1][q] as f64;
                }
            }
            let n = g.strands.max(1) as f64;
            lens.sort_by(|a, b| a.total_cmp(b));
            curl.sort_by(|a, b| a.total_cmp(b));
            let step = (g.strands / 24).max(1);
            let roots: Vec<[f32; 3]> = (0..g.strands).step_by(step).map(|s| g.points[s * k]).collect();
            let o = v.as_object_mut().unwrap();
            o.insert("k".into(), k.into());
            o.insert("len_mean".into(), (lens.iter().sum::<f32>() / lens.len().max(1) as f32).into());
            o.insert("len_med".into(), lens.get(lens.len() / 2).copied().unwrap_or(0.0).into());
            o.insert("len_max".into(), lens.last().copied().unwrap_or(0.0).into());
            o.insert("curl_med".into(), curl.get(curl.len() / 2).copied().unwrap_or(1.0).into());
            o.insert("root_c".into(), serde_json::json!([c[0] / n, c[1] / n, c[2] / n]));
            o.insert("tip_c".into(), serde_json::json!([tip[0] / n, tip[1] / n, tip[2] / n]));
            o.insert("roots".into(), serde_json::json!(roots));
            o.insert("width".into(), g.width.into());
            v.to_string()
        })
        .collect();
    let mut f = std::fs::File::create(&a[4])?;
    for l in lines {
        writeln!(f, "{l}")?;
    }
    Ok(())
}
