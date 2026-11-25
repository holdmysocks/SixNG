//! Research: archetype flag bits by model type (ft / dr / other).
//! archflagstats <game> <work> <archive>
use rayon::prelude::*;
use std::collections::HashMap;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(&a[3])?;
    let ix = ws.load_index(&ar)?;
    let mut kind: HashMap<u32, (String, String)> = HashMap::new();
    for e in &ix.entries {
        if let (Some(n), Some(t)) = (&e.name, &e.rtype) {
            kind.entry(sixng_core::meta7a::joaat(n)).or_insert((t.clone(), n.clone()));
        }
    }
    let defs: Vec<(u32, u32, u32)> = ix.entries.par_iter().filter(|e| e.sub == "7a").flat_map_iter(|e| {
        let mut out = Vec::new();
        if let Ok(r) = sixng_core::resource::Resource::load_sys(&ar.path, e) {
            if let Some(m) = sixng_core::meta7a::Meta::parse(&r) {
                for o in m.objects(sixng_core::export::ARCHETYPE_DEF) {
                    let f = m.scalars(sixng_core::export::ARCHETYPE_DEF, o, &[]);
                    let g = |k: &str| f.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    out.push((r.u32(o + 0x74), g("flags"), g("8f215e15")));
                }
            }
        }
        out
    }).collect();
    let mut uniq: HashMap<u32, (u32, u32)> = HashMap::new();
    for d in defs { uniq.insert(d.0, (d.1, d.2)); }
    let mut counts: HashMap<String, (usize, [usize; 32])> = HashMap::new();
    for (h, (f, at)) in &uniq {
        let k = format!("{} at{}", kind.get(h).map(|x| x.0.as_str()).unwrap_or("?"), at);
        let c = counts.entry(k).or_insert((0, [0; 32]));
        c.0 += 1;
        for b in 0..32 { if f >> b & 1 == 1 { c.1[b] += 1; } }
    }
    let mut ks: Vec<_> = counts.into_iter().collect();
    ks.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    for (k, (n, bits)) in ks.iter().take(8) {
        println!("{k}: {n}  {}", (0..32).filter(|&b| bits[b] > 0).map(|b| format!("b{b}:{:.0}%", 100.0 * bits[b] as f64 / *n as f64)).collect::<Vec<_>>().join(" "));
    }
    // samples: names with bit 17 clear among ft, and set among dr
    let mut s17 = (Vec::new(), Vec::new());
    for (h, (f, _)) in &uniq {
        if let Some((t, n)) = kind.get(h) {
            if t == "ft" && f >> 17 & 1 == 0 && s17.0.len() < 15 { s17.0.push(n.clone()); }
            if t == "dr" && f >> 17 & 1 == 1 && s17.1.len() < 15 { s17.1.push(n.clone()); }
        }
    }
    println!("ft without b17: {:?}\ndr with b17: {:?}", s17.0, s17.1);
    Ok(())
}
