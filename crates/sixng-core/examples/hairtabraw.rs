//! Research: write the raw groom piece table resource(s) (system + graphics
//! stream) to <out dir>/piecetab_<off>.bin. hairtabraw <packs.rpf> <index json> <out dir>
use sixng_core::resource::Resource;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let arc = std::path::Path::new(&a[1]);
    let ix = sixng_core::scan::load_index(std::path::Path::new(&a[2]))?;
    for e in ix.entries.iter().filter(|e| e.name.is_none() && e.kind == sixng_core::classify::Kind::RageObj && e.usize >= 0x8000) {
        let Ok(s) = Resource::load_sys(arc, e) else { continue };
        if s.sys.len() < 0x70 || s.u32(0x10) != 0x100 || s.u64(0x18) != 0x5000_0060 || sixng_core::groom::is_groom(&s.sys) {
            continue;
        }
        let r = Resource::load(arc, e)?;
        let mut b = r.sys.clone();
        let n = b.len();
        b.extend_from_slice(&r.gfx);
        println!("{:x} sys {:x} gfx {:x}", e.off, n, r.gfx.len());
        std::fs::write(std::path::Path::new(&a[3]).join(format!("piecetab_{:x}.bin", e.off)), b)?;
    }
    Ok(())
}
