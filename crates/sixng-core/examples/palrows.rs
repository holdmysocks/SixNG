//! Research: list the ped tint palettes (name hash, size) and print rows of
//! one as sRGB at mid shade. palrows <game root> [hex palette] [row,row,...]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let _ws = sixng_core::export::Workspace::new(&a[1], std::env::var("SIXNG_WORK").unwrap_or_else(|_| "work".into()));
    let p = sixng_core::pedtint::palettes(std::path::Path::new(&a[1])).expect("palettes");
    let mut k: Vec<_> = p.iter().collect();
    k.sort_by_key(|x| *x.0);
    for (h, pal) in &k {
        println!("{h:08x} {:?} {}x{}", sixng_core::pedtint::palette_name(**h), pal.w, pal.h);
    }
    if let Some(h) = a.get(2).and_then(|s| u32::from_str_radix(s, 16).ok()) {
        if let Some(pal) = p.get(&h) {
            let rows: Vec<u8> = a.get(3).map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_else(|| (0..64).collect());
            for r in rows {
                println!("row {r}: dark {:?} mid {:?} light {:?}", pal.sample(r, 40), pal.sample(r, 128), pal.sample(r, 220));
            }
        } else {
            println!("{h:08x}: not a palette of the dictionary");
        }
    }
}
