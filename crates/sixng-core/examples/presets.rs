//! Research: print the material presets (`f795e8ba`) of meta (0x7a)
//! resources in full.
//! presets <packs.rpf> <index json> <hex offset>...
//! STRUCT=<hex struct hash> prints the objects of another struct instead.
use sixng_core::{meta7a::Meta, resource::Resource, scan};
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let st = std::env::var("STRUCT").ok().and_then(|s| u32::from_str_radix(&s, 16).ok()).unwrap_or(0xf795_e8ba);
    for off in &a[3..] {
        let off = u64::from_str_radix(off, 16)?;
        let Some(e) = ix.entries.iter().find(|e| e.off == off) else { continue };
        let r = Resource::load_meta(std::path::Path::new(&a[1]), e)?;
        let Some(m) = Meta::parse(&r) else { continue };
        let v: Vec<_> = m.objects(st).into_iter().filter_map(|o| m.object(st, o, 4)).collect();
        println!("{}", serde_json::json!({"offset": format!("{off:x}"), "presets": v}));
    }
    Ok(())
}
