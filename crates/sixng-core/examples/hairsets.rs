//! Research: the strand sets each hair / fur asset names (registry
//! properties 960033653 and 2259149558), checked against the set names of
//! the game's strand tables; with words, print the assets whose
//! placeholder (from a hair link table) contains each word.
//! hairsets <game root> <work dir> <hair tables json> <hair links json> [word…]
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let arch = ws.resolve("packs.rpf")?;
    let ix = sixng_core::scan::load_index(&sixng_core::scan::index_path(&ws.index_dir(), &arch.id))?;
    let t0 = std::time::Instant::now();
    let t = sixng_core::hairsets::build(&arch, &ix.entries);
    eprintln!("{} assets with sets in {:.0?}", t.sets.len(), t0.elapsed());
    let tab: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let names = |k: &str| -> std::collections::HashMap<u32, String> { tab[k].as_object().map(|m| m.keys().map(|n| (sixng_core::meta7a::joaat(&n.to_ascii_lowercase()), n.clone())).collect()).unwrap_or_default() };
    let (cn, pn) = (names("colour_sets"), names("parameter_sets"));
    let (mut c_ok, mut p_ok, mut c_n, mut p_n) = (0, 0, 0, 0);
    let mut unknown_p = std::collections::BTreeMap::<u32, usize>::new();
    for s in t.sets.values() {
        if s.colour != 0 { c_n += 1; if cn.contains_key(&s.colour) { c_ok += 1 } }
        if s.param != 0 { p_n += 1; if pn.contains_key(&s.param) { p_ok += 1 } else { *unknown_p.entry(s.param).or_default() += 1 } }
    }
    println!("colour set named: {c_n}, of them a colour set of the tables: {c_ok}");
    println!("parameter set named: {p_n}, of them a parameter set of the tables: {p_ok}");
    println!("unmatched parameter values (count): {:?}", unknown_p.iter().take(10).collect::<Vec<_>>());
    let mut use_p = std::collections::BTreeMap::<&str, usize>::new();
    for s in t.sets.values() { if let Some(n) = pn.get(&s.param) { *use_p.entry(n.as_str()).or_default() += 1 } }
    println!("parameter sets in use: {}; most used: {:?}", use_p.len(), { let mut v: Vec<_> = use_p.iter().collect(); v.sort_by(|x, y| y.1.cmp(x.1)); v.into_iter().take(8).collect::<Vec<_>>() });
    let links: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[4])?)?;
    for w in &a[5.min(a.len())..] {
        if let Some(s) = t.sets.get(w.as_str()) {
            println!("{w}: colour {} param {}", cn.get(&s.colour).cloned().unwrap_or_default(), pn.get(&s.param).cloned().unwrap_or_default());
        }
        for (g, v) in links["links"].as_object().into_iter().flatten() {
            let ph = v[0]["placeholder"].as_str().unwrap_or("");
            if !ph.contains(w.as_str()) { continue }
            let s = t.sets.get(g).copied().unwrap_or_default();
            println!("{g} {ph}: colour {} param {}", cn.get(&s.colour).cloned().unwrap_or(format!("{:08x}", s.colour)), pn.get(&s.param).cloned().unwrap_or(format!("{:08x}", s.param)));
        }
    }
    Ok(())
}
