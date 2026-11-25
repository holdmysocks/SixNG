//! Research: type / include flags of the children of the map collision
//! composites (bound resources of an archive), with each child's material
//! ids, one JSON line per (type, include) pair.
//! mapboundflags <game> <work> [archive]
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(a.get(3).map(|s| s.as_str()).unwrap_or("levels"))?;
    let ix = ws.load_index(&ar)?;
    type Acc = BTreeMap<(u32, u32, u32, u32), (usize, BTreeMap<u16, usize>, usize)>;
    let acc: Mutex<Acc> = Mutex::new(BTreeMap::new());
    ix.entries.par_iter().filter(|e| e.name.is_none() && e.kind == sixng_core::classify::Kind::RageObj).for_each(|e| {
        let Ok(r) = sixng_core::resource::Resource::load_sys(&ar.path, e) else { return };
        if !sixng_core::bounds::is_bound(&r) || r.sys[0x10] != 6 {
            return;
        }
        let n = r.sys.len();
        let count = r.u16(0x98) as usize;
        let (Some(kids), Some(fl)) = (r.sys_off(r.u64(0x70)), r.sys_off(r.u64(0x88))) else { return };
        if count == 0 || count > 100_000 || kids + 8 * count > n || fl + 16 * count > n {
            return;
        }
        let mut local: Acc = BTreeMap::new();
        for i in 0..count {
            let f = fl + 16 * i;
            let key = (r.u32(f), r.u32(f + 4), r.u32(f + 8), r.u32(f + 12));
            let slot = local.entry(key).or_default();
            slot.0 += 1;
            if let Some(c) = r.sys_off(r.u64(kids + 8 * i)) {
                if c + 0x140 <= n && r.sys[c + 0x10] == 5 {
                    slot.2 += r.u32(c + 0xD4) as usize;
                    let nm = r.sys[c + 0x120] as usize;
                    if let Some(m) = r.sys_off(r.u64(c + 0xF0)).filter(|m| m + 8 * nm <= n) {
                        for k in 0..nm {
                            *slot.1.entry(r.u16(m + 8 * k)).or_default() += 1;
                        }
                    }
                }
            }
        }
        let mut g = acc.lock().unwrap();
        for (k, v) in local {
            let s = g.entry(k).or_default();
            s.0 += v.0;
            s.2 += v.2;
            for (m, c) in v.1 {
                *s.1.entry(m).or_default() += c;
            }
        }
    });
    for ((t, i, x, y), (c, mats, polys)) in acc.into_inner().unwrap() {
        let mut m: Vec<_> = mats.into_iter().collect();
        m.sort_by(|a, b| b.1.cmp(&a.1));
        m.truncate(12);
        println!("{}", serde_json::json!({"type": format!("{t:08x}"), "include": format!("{i:08x}"), "w2": format!("{x:08x}"), "w3": format!("{y:08x}"), "children": c, "polygons": polys, "materials": m.iter().map(|(k, v)| (format!("{k:04x}"), v)).collect::<Vec<_>>()}));
    }
    Ok(())
}
