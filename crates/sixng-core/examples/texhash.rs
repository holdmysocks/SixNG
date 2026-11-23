//! Research: decode textures by 64-bit store hash, whichever store holds them.
//! texhash <game root> <out dir> <hex hash64>...
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let set = sixng_core::ttpf::global(std::path::Path::new(&a[1]));
    for h in &a[3..] {
        let Ok(k) = u64::from_str_radix(h, 16) else { continue };
        match set.get(k) {
            Some((store, t)) => match sixng_core::ttpf::write_png(store, t, std::path::Path::new(&a[2])) {
                Ok((p, st)) => println!("{h} {} {}x{} fmt {} mean {:?} -> {}", store.label(), t.width, t.height, t.dxgi_format, st.mean_rgba.map(|x| x.round()), p.display()),
                Err(e) => println!("{h} {} not decoded: {e}", store.label()),
            },
            None => println!("{h} not in any store"),
        }
    }
    Ok(())
}
