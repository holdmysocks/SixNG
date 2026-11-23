//! Build the hair link table (asset GUID -> strand grooms) without the
//! cache and write it as JSON; with GUIDs (hex) after the output path,
//! print their placements.
//! hairlinks <packs.rpf> <index json> <work dir> <out json> [guid…]
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let path = std::path::PathBuf::from(&a[1]);
    let ix = sixng_core::scan::load_index(std::path::Path::new(&a[2]))?;
    let info = sixng_core::game::ArchiveInfo { id: ix.archive.clone(), size: ix.size, container: ix.container.clone(), path };
    let t0 = std::time::Instant::now();
    let t = sixng_core::hairlink::build(std::path::Path::new(&a[3]), &info, &ix.entries)?;
    let best: Vec<f32> = t.links.values().filter_map(|v| v.iter().find(|l| !l.grooms.is_empty()).map(|l| l.confidence)).collect();
    eprintln!(
        "{} assets in {:.0?}; with grooms {}; confidence >= 0.8: {}, >= 0.5: {}",
        t.links.len(),
        t0.elapsed(),
        best.len(),
        best.iter().filter(|c| **c >= 0.8).count(),
        best.iter().filter(|c| **c >= 0.5).count()
    );
    std::fs::write(&a[4], serde_json::to_vec_pretty(&t)?)?;
    for g in &a[5.min(a.len())..] {
        let guid = u64::from_str_radix(g, 16)?;
        println!("{g}: {}", serde_json::to_string_pretty(&t.links.get(&format!("{guid:016x}")))?);
        println!("resolve: {:?}", t.resolve(guid, None).map(|l| (&l.placeholder, l.confidence, l.grooms.iter().map(|o| format!("{o:x}")).collect::<Vec<_>>())));
    }
    Ok(())
}
