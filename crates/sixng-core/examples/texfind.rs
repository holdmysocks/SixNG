//! Research: look texture hashes up in every texture store and write PNGs.
//! texfind <game root> <out dir> <hex hash>...
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let set = sixng_core::ttpf::global(std::path::Path::new(&a[1]));
    let out = std::path::Path::new(&a[2]);
    for h in &a[3..] {
        let Ok(v) = u64::from_str_radix(h, 16) else { continue };
        match set.get(v) {
            Some((s, t)) => {
                let r = sixng_core::ttpf::write_png(s, t, out);
                println!("{h} {} {}x{} fmt {} decodable {} -> {}", s.label(), t.width, t.height, t.dxgi_format, t.decodable(), r.map(|(p, st)| format!("{} mean {:?} mip {}", p.display(), st.mean_rgba, st.mip)).unwrap_or_else(|e| format!("ERR {e:#}")));
            }
            None => println!("{h} not in any store"),
        }
    }
    Ok(())
}
