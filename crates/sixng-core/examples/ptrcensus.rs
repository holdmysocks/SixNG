//! Research: every pointer / array / map member of every 7a object, classified
//! as pool reference, real pointer or ambiguous.  ptrcensus <game> <archive>
use rayon::prelude::*;
use std::collections::BTreeMap;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], "work");
    let ar = ws.resolve(&a[2])?;
    let ix = ws.load_index(&ar)?;
    let r: Vec<BTreeMap<String, usize>> = ix.entries.par_iter().filter(|e| e.sub == "7a").filter_map(|e| {
        let rm = sixng_core::resource::Resource::load_meta(&ar.path, e).ok()?;
        let m = sixng_core::meta7a::Meta::parse(&rm)?;
        let mut h = BTreeMap::new();
        let starts: std::collections::HashSet<usize> = m.blocks.iter().map(|b| b.2).collect();
        for sd in m.structs.values() {
            for o in m.objects(sd.hash) {
                for mem in &sd.members {
                    let (at, exp) = match mem.ty {
                        0x07 => (o + mem.off as usize, mem.extra),
                        0x52 => (o + mem.off as usize, sd.members.get(mem.reference as usize).filter(|e| e.ty == 0x05).map_or(0, |e| e.extra)),
                        0x71 => (o + mem.off as usize + 8, 0),
                        _ => continue,
                    };
                    let v = rm.u64(at);
                    if v == 0 { continue; }
                    let bi = (v & 0xFFF) as usize; let off = (v >> 20) as usize;
                    let pool = (v >> 12) & 0xFF == 0 && bi >= 1 && m.blocks.get(bi - 1).is_some_and(|b| off < b.1 as usize && b.2 != usize::MAX);
                    let phash = if pool { Some(m.blocks[bi - 1].0) } else { None };
                    let real = rm.meta_off(v);
                    let amb = pool && real.is_some();
                    let k = format!("{}ty{:02x} pool={} pool_hash_ok={:?} real={} real_blockstart={}", if amb { format!("AMB struct {:08x} member {:08x} ", sd.hash, mem.hash) } else { String::new() }, mem.ty, pool as u8, phash.map(|x| exp == 0 || x == exp), real.is_some() as u8, real.is_some_and(|x| starts.contains(&x)) as u8);
                    *h.entry(k).or_insert(0) += 1;
                }
            }
        }
        Some(h)
    }).collect();
    let mut t = BTreeMap::new();
    for h in r { for (k, v) in h { *t.entry(k).or_insert(0) += v; } }
    for (k, v) in t { println!("{v:9} {k}"); }
    Ok(())
}
