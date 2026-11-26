//! Research: body skin textures of characters.
//! bodyskin item <game> <work> <wardrobe offset hex> <item hex>...   the items (and their parent chain) as JSON
//! bodyskin dupes <game> <work>                                       referenced textures found through the de-duplication table
//! bodyskin survey <game> <work> [model regex]                       one JSON line per character: its body skin parts,
//!                                                                    the textures the items name and the drawable binds,
//!                                                                    and which of them are in a store
use sixng_core::{meta7a::Meta, resource::Resource};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[2], &a[3]);
    let ar = ws.resolve("packs")?;
    let ix = ws.load_index(&ar)?;
    let f = sixng_core::meta7a::field_name;
    match a[1].as_str() {
        "item" => {
            let off = u64::from_str_radix(&a[4], 16)?;
            let e = ix.entries.iter().find(|e| e.off == off).expect("entry");
            let r = Resource::load_meta(&ar.path, e)?;
            let m = Meta::parse(&r).expect("meta");
            let items: HashMap<u64, serde_json::Value> = m.objects(0xf795_e8ba).into_iter().filter_map(|o| m.object(0xf795_e8ba, o, 6)).map(|v| (v[f(0x6512_1749)].as_u64().unwrap_or(0), v)).collect();
            for h in &a[5..] {
                let mut id = u64::from_str_radix(h, 16)?;
                for _ in 0..8 {
                    let Some(it) = items.get(&id) else { break };
                    println!("{}", serde_json::to_string_pretty(it)?);
                    match [0x080e_ec39u32, 0xe13e_baf8].iter().filter_map(|&k| it[f(k)].as_u64()).find(|&x| x != 0 && items.contains_key(&x)) {
                        Some(n) if n != id => id = n,
                        _ => break,
                    }
                    println!("---- parent {id:08x}");
                }
            }
        }
        "survey" => {
            let set = sixng_core::ttpf::global(&ws.game);
            let tu = sixng_core::texusers::TexUsers::load(&sixng_core::texusers::path(&ws))?;
            let mut by_name: HashMap<&str, Vec<u32>> = HashMap::new();
            for (mi, ms) in tu.material_models.iter().enumerate() {
                for &m in ms {
                    by_name.entry(tu.models[m as usize].1.as_str()).or_default().push(mi as u32);
                }
            }
            let re = regex::Regex::new(a.get(4).map(|s| s.as_str()).unwrap_or("."))?;
            let mut names: Vec<String> = sixng_core::outfit::characters(&ix).into_iter().filter(|c| c.kind == "character").map(|c| c.name).collect();
            let own: std::collections::HashSet<String> = names.iter().cloned().collect();
            names.extend(sixng_core::pedlook::listed_models(&ws.game).unwrap_or_default().into_iter().map(|m| m.name).filter(|n| !own.contains(n) && !sixng_core::outfit::is_animal(n)));
            let stored = |h: u64| set.get(h).map(|(s, t)| format!("{} {}x{} f{}", s.label(), t.width, t.height, t.dxgi_format));
            for name in names.into_iter().filter(|n| re.is_match(n)) {
                let l = match sixng_core::pedlook::look(&ws.game, &ar, &ix, &name) {
                    Ok(l) => l,
                    Err(e) => {
                        println!("{}", serde_json::json!({"model": name, "error": e}));
                        continue;
                    }
                };
                let o = &l.outfits[0];
                let head = o.parts.iter().find(|p| p.slot == "head").map(|p| p.drawable.clone());
                let parts: Vec<_> = o
                    .parts
                    .iter()
                    .filter(|p| p.slot == "body upper" || p.slot == "body lower")
                    .map(|p| {
                        let skins: Vec<_> = p.skins.iter().map(|(al, nr)| {
                            let al = u64::from_str_radix(al, 16).unwrap_or(0);
                            let nr = u64::from_str_radix(nr, 16).unwrap_or(0);
                            serde_json::json!({"albedo": format!("{al:016x}"), "albedo_stored": stored(al), "normal": format!("{nr:016x}"), "normal_stored": stored(nr)})
                        }).collect();
                        let mut mats: Vec<_> = by_name.get(p.drawable.as_str()).cloned().unwrap_or_default();
                        mats.sort();
                        mats.dedup();
                        let mats: Vec<_> = mats.iter().map(|&mi| {
                            let m = &tu.materials[mi as usize];
                            serde_json::json!({"shader": m.shader, "slots": m.slots.iter().map(|(p, h)| serde_json::json!([p, format!("{h:016x}"), stored(*h)])).collect::<Vec<_>>()})
                        }).collect();
                        serde_json::json!({"slot": p.slot, "drawable": p.drawable, "item": p.item, "found_by": p.found_by, "skins": skins, "materials": mats})
                    })
                    .collect();
                println!("{}", serde_json::json!({"model": name, "fit": l.fit, "head": head, "parts": parts}));
            }
        }
        "dupes" => {
            // referenced textures (every model material) found before and after the de-duplication table
            let set = sixng_core::ttpf::global(&ws.game);
            let tu = sixng_core::texusers::TexUsers::load(&sixng_core::texusers::path(&ws))?;
            let d = set.dupes().map(|d| d.len()).unwrap_or(0);
            let (mut n, mut stored, mut via) = (0, 0, 0);
            let mut by_shader: std::collections::BTreeMap<String, usize> = Default::default();
            for (h, mats) in &tu.by_texture {
                n += 1;
                if set.get_stored(*h).is_some() {
                    stored += 1;
                } else if set.get(*h).is_some() {
                    via += 1;
                    *by_shader.entry(tu.materials[mats[0] as usize].shader.clone()).or_default() += 1;
                }
            }
            let mut top: Vec<_> = by_shader.into_iter().collect();
            top.sort_by_key(|x| std::cmp::Reverse(x.1));
            top.truncate(25);
            println!("{}", serde_json::json!({"table_pairs": d, "referenced": n, "stored": stored, "found_through_table": via, "still_missing": n - stored - via, "by_shader": top}));
        }
        _ => anyhow::bail!("item | survey | dupes"),
    }
    Ok(())
}
