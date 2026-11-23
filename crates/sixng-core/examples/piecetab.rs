//! Research: dump the groom piece table (asset GUID -> piece ids) as JSON.
//! piecetab <packs.rpf> <index json>
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ix = sixng_core::scan::load_index(std::path::Path::new(&a[2]))?;
    let t = sixng_core::furlink::piece_table(std::path::Path::new(&a[1]), &ix.entries);
    let m: std::collections::BTreeMap<String, Vec<String>> = t.iter().map(|(k, v)| (format!("{k:016x}"), v.iter().map(|x| format!("{x:016x}")).collect())).collect();
    println!("{}", serde_json::to_string(&m)?);
    Ok(())
}
