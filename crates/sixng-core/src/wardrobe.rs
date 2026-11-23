//! Ped wardrobe meta resources (packs.rpf, 2,713 resources with struct
//! `f795e8ba`): per ped model, the variations it can wear and the parts each
//! one uses. Records link by u32 id, not by pointer:
//!
//! ```text
//! a9d57751 variation: 65121749 id, Scale, fb5bb20d list of {65121749 item id, StateName}
//! f795e8ba item:      65121749 id, Category (slot: eyes, hair, hats …),
//!                     080eec39 / e13ebaf8 parent item (a colour variant points at
//!                     its base garment), 12f85293 tag map {Key, Item[u32]},
//!                     ebacf18c list of 37e52277 texture sets (64-bit texture hashes)
//! tag keys:           4bfd27dd head drawable, 023e5a2f hair / brows / body hair,
//!                     2a8cdb1d garment name (or a shape word), a9473ab4 style
//!                     (Casual, sports, swim …), e60617ae colour, fe330559 length
//! ```
//!
//! A tag value is the joaat of a drawable name prefix (`tanktopcasual_fr1_028`
//! for `tanktopcasual_fr1_028_disheveled_un_full`), but most garment names are
//! not recovered. The texture hashes of an item (and its parents) do lead to
//! the drawables, through the texture-users index: an item's drawable is the
//! model that uses most of its textures. Checked on the arcade zombie
//! resources: 84 of 118 variations get parts; a variation is either a
//! clothing set or a head set (head, eyes, teeth), so a full look is one of
//! each.
use crate::{meta7a::Meta, texusers::TexUsers};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub item: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Name from the tag map (head, hair, garment), when it resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The drawable to use (`outfit --parts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawable: Option<String>,
    /// Texture overrides of this item (`outfit --tex-remap`): the drawable's
    /// own texture -> the item's, for the diffuse (`ead733fc`) and normal
    /// (`Normal`) maps of its first texture set, where the drawable has one
    /// texture in that slot.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tex_remap: Vec<(String, String)>,
    /// Tag map entries whose values resolve (tag key hex -> words), along the
    /// chain, base item first: style, colour, length, dirt / blood presets
    /// (`AM_Dirtdefinition_BloodEffect`) …
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, Vec<String>>,
}

/// Resolve a tag map (`12f85293`) into `out`: key hex -> resolved values.
fn tag_words(v: &Value, pre: &HashMap<u32, (String, String)>, out: &mut BTreeMap<String, Vec<String>>) {
    for t in v[crate::meta7a::field_name(0x12f8_5293)].as_array().into_iter().flatten() {
        let Some(k) = t["Key"].as_u64() else { continue };
        let words: Vec<String> = t["Item"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_u64())
            .filter_map(|x| pre.get(&(x as u32)).map(|p| p.0.clone()).or_else(|| crate::names::lookup(x as u32, crate::names::Ctx::Field).filter(|n| !n.is_empty())))
            .collect();
        if !words.is_empty() {
            out.insert(format!("{k:08x}"), words);
        }
    }
}

/// The item's override textures: (slot, hash) from the first texture set
/// (`ebacf18c` / `e7cb9e1b`) along the chain that has any.
fn set_textures(chain: &[&Value]) -> Vec<(&'static str, u64)> {
    let f = crate::meta7a::field_name;
    for c in chain {
        for s in c[f(0xebac_f18c)].as_array().into_iter().flatten() {
            let e = &s[f(0xe7cb_9e1b)];
            let v: Vec<(&str, u64)> = [("diffusetex", e[f(0xead7_33fc)].as_str()), ("bumptex", e["Normal"].as_str())]
                .into_iter()
                .filter_map(|(k, x)| Some((k, u64::from_str_radix(x?, 16).ok().filter(|&h| h != 0)?)))
                .collect();
            if !v.is_empty() {
                return v;
            }
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, Serialize)]
pub struct Variation {
    pub id: String,
    pub scale: Value,
    /// The variation's own tags (Style, BodyShapePreset …), as for parts.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, Vec<String>>,
    pub parts: Vec<Part>,
}

/// joaat of every `_`-separated prefix of the drawable names -> (prefix, a
/// drawable with that prefix, `_full` ones first).
pub fn prefixes<'a>(names: impl Iterator<Item = &'a str>) -> HashMap<u32, (String, String)> {
    let mut m: HashMap<u32, (String, String)> = HashMap::new();
    for n in names {
        let parts: Vec<&str> = n.split('_').collect();
        for i in 1..=parts.len() {
            let p = parts[..i].join("_");
            let e = m.entry(crate::names::joaat_exact(&p)).or_insert_with(|| (p.clone(), n.to_string()));
            if !e.1.ends_with("_full") && n.ends_with("_full") {
                e.1 = n.to_string();
            }
        }
    }
    m
}

/// The drawable that uses most of an item's textures (`_full` before cull
/// and LOD copies, then by name).
pub fn pick(counts: &BTreeMap<String, usize>) -> Option<String> {
    counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.ends_with("_full").cmp(&a.0.ends_with("_full")).reverse()).then(b.0.cmp(a.0))).map(|(n, _)| n.clone())
}

fn hashes64(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| hashes64(x, out)),
        Value::Object(m) => m.values().for_each(|x| hashes64(x, out)),
        Value::String(s) if s.len() == 16 => out.extend(u64::from_str_radix(s, 16).ok().filter(|&h| h != 0)),
        _ => {}
    }
}

/// Decode the variations of one wardrobe resource. `archive` restricts the
/// texture-linked models to that archive's entries (`packs.rpf`).
pub fn variations(m: &Meta, pre: &HashMap<u32, (String, String)>, tu: &TexUsers, archive: &str) -> Vec<Variation> {
    let f = crate::meta7a::field_name;
    let get = |h: u32| -> Vec<Value> { m.objects(h).into_iter().filter_map(|o| m.object(h, o, 0)).collect() };
    let items: HashMap<u64, Value> = get(0xf795_e8ba).into_iter().filter_map(|v| Some((v[f(0x6512_1749)].as_u64()?, v))).collect();
    let tags = |v: &Value, key: u32| -> Vec<u32> {
        v[f(0x12f8_5293)].as_array().into_iter().flatten().filter(|t| t["Key"].as_u64() == Some(key as u64)).flat_map(|t| t["Item"].as_array().cloned().unwrap_or_default()).filter_map(|x| x.as_u64().map(|x| x as u32)).collect()
    };
    // model name -> its materials (for the override slots)
    let mut model_mats: HashMap<&str, Vec<u32>> = HashMap::new();
    for (mi, ms) in tu.material_models.iter().enumerate() {
        for &m in ms {
            let (id, name) = &tu.models[m as usize];
            if id.starts_with(archive) {
                model_mats.entry(name.as_str()).or_default().push(mi as u32);
            }
        }
    }
    let mut out = Vec::new();
    for v in get(0xa9d5_7751) {
        let mut parts = Vec::new();
        for ln in v[f(0xfb5b_b20d)].as_array().into_iter().flatten() {
            let Some(it) = ln[f(0x6512_1749)].as_u64().and_then(|i| items.get(&i)) else { continue };
            let mut chain = vec![it];
            while chain.len() < 8 {
                let last = chain[chain.len() - 1];
                let next = [0x080e_ec39u32, 0xe13e_baf8].iter().filter_map(|&k| last[f(k)].as_u64()).filter(|&x| x != 0).filter_map(|x| items.get(&x)).find(|n| !chain.iter().any(|c| std::ptr::eq(*c, *n)));
                match next {
                    Some(n) => chain.push(n),
                    None => break,
                }
            }
            let mut tex = Vec::new();
            chain.iter().for_each(|c| hashes64(c, &mut tex));
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for h in tex {
                let mut models: Vec<u32> = tu.users(h).iter().flat_map(|&mi| tu.material_models[mi as usize].iter().copied()).collect();
                models.sort();
                models.dedup();
                for mi in models {
                    let (id, name) = &tu.models[mi as usize];
                    if id.starts_with(archive) && !name.is_empty() {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
            }
            let name = chain.iter().find_map(|c| [0x4bfd_27ddu32, 0x023e_5a2f, 0x2a8c_db1d].iter().flat_map(|&k| tags(c, k)).find_map(|x| pre.get(&x).filter(|p| p.0.contains('_'))));
            let category = chain.iter().find_map(|c| c["Category"].as_u64().filter(|&x| x != 0)).map(|h| crate::names::lookup(h as u32, crate::names::Ctx::Field).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{h:08x}")));
            let drawable = pick(&counts).or_else(|| name.map(|p| p.1.clone()));
            let mut tex_remap = Vec::new();
            if let Some(mats) = drawable.as_deref().and_then(|d| model_mats.get(d)) {
                for (slot, new) in set_textures(&chain) {
                    let mut own: Vec<u64> = mats.iter().flat_map(|&m| tu.materials[m as usize].slots.iter().filter(|s| s.0 == slot).map(|s| s.1)).collect();
                    own.sort();
                    own.dedup();
                    if let [old] = own[..] {
                        if old != new {
                            tex_remap.push((format!("{old:016x}"), format!("{new:016x}")));
                        }
                    }
                }
            }
            let mut tg = BTreeMap::new();
            chain.iter().rev().for_each(|c| tag_words(c, pre, &mut tg));
            parts.push(Part { item: format!("{:08x}", it[f(0x6512_1749)].as_u64().unwrap_or(0)), category, name: name.map(|p| p.0.clone()), drawable, tex_remap, tags: tg });
        }
        let mut tags = BTreeMap::new();
        tag_words(&v, pre, &mut tags);
        out.push(Variation { id: format!("{:08x}", v[f(0x6512_1749)].as_u64().unwrap_or(0)), scale: v["Scale"].clone(), tags, parts });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn pick_prefers_most_textures_then_full() {
        let c = [("top_fr1_000_cull_00".to_string(), 3), ("top_fr1_000_full".to_string(), 3), ("brows_mi1_000_full".to_string(), 1)].into_iter().collect();
        assert_eq!(super::pick(&c).as_deref(), Some("top_fr1_000_full"));
    }

    #[test]
    fn prefixes_resolve_garment_names() {
        let m = super::prefixes(["tanktopcasual_fr1_028_disheveled_un_cull_00", "tanktopcasual_fr1_028_disheveled_un_full"].into_iter());
        let h = crate::names::joaat_exact("tanktopcasual_fr1_028");
        assert_eq!(m[&h], ("tanktopcasual_fr1_028".to_string(), "tanktopcasual_fr1_028_disheveled_un_full".to_string()));
    }
}
