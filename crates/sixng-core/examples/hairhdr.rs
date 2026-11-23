//! Research: dump the first 0x140 bytes of every groom's system page as
//! "<off hex> <sys len hex> <hex>" lines. hairhdr <packs.rpf> <index json> <grooms catalog json> <out>
use rayon::prelude::*;
use sixng_core::resource::Resource;
use std::io::Write;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = sixng_core::scan::load_index(std::path::Path::new(&a[2]))?;
    let cat: Vec<sixng_core::groom::GroomInfo> = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let by: std::collections::HashMap<u64, &sixng_core::scan::Entry> = ix.entries.iter().map(|e| (e.off, e)).collect();
    let lines: Vec<String> = cat
        .par_iter()
        .map(|g| {
            let Some(s) = by.get(&g.off).and_then(|e| Resource::load_sys(arc, e).ok()) else { return format!("{:x} 0 ", g.off) };
            let n = s.sys.len().min(0x140);
            format!("{:x} {:x} {}", g.off, s.sys.len(), s.sys[..n].iter().map(|b| format!("{b:02x}")).collect::<String>())
        })
        .collect();
    let mut f = std::fs::File::create(&a[4])?;
    for l in lines {
        writeln!(f, "{l}")?;
    }
    Ok(())
}
