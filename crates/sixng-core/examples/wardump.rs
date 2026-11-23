//! Research: dump every wardrobe meta resource (structs a9d57751 variations,
//! f795e8ba items) of an archive as JSON lines, and the per-model variation
//! lists of its PSO files.
//! wardump <packs.rpf> <index json> <out dir>
use sixng_core::{meta7a::Meta, resource::Resource, scan, classify::Kind};
use std::io::Write;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let out = std::path::Path::new(&a[3]);
    let mut w = std::io::BufWriter::new(std::fs::File::create(out.join("wardrobe.jsonl"))?);
    let mut n = 0;
    for e in ix.entries.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a") {
        let Ok(r) = Resource::load_meta(arc, e) else { continue };
        let Some(m) = Meta::parse(&r) else { continue };
        let vars = m.objects(0xa9d5_7751);
        let items = m.objects(0xf795_e8ba);
        if vars.is_empty() && items.is_empty() {
            continue;
        }
        let v: Vec<_> = vars.into_iter().filter_map(|o| m.object(0xa9d5_7751, o, 4)).collect();
        let i: Vec<_> = items.into_iter().filter_map(|o| m.object(0xf795_e8ba, o, 4)).collect();
        writeln!(w, "{}", serde_json::json!({"off": format!("{:x}", e.off), "name": e.name, "schema": m.schema(), "variations": v, "items": i}))?;
        n += 1;
    }
    eprintln!("{n} wardrobe resources");
    let mut w = std::io::BufWriter::new(std::fs::File::create(out.join("varlists.jsonl"))?);
    for e in ix.entries.iter().filter(|e| e.kind == Kind::Pso) {
        let Ok(b) = scan::read_entry(arc, e) else { continue };
        let tag = 0xa5ea_6310u32;
        if !b.windows(4).any(|x| x == tag.to_le_bytes() || x == tag.to_be_bytes()) {
            continue;
        }
        if let Some(p) = sixng_core::pso::parse(&b) {
            writeln!(w, "{}", serde_json::json!({"off": format!("{:x}", e.off), "name": e.name, "pso": p.to_json()}))?;
        }
    }
    Ok(())
}
