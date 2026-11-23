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
    /// Clothing colour (`pedtint`): the palette and tint rows the item or a
    /// parent sets; none when no item of the chain stores one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<PartTint>,
    /// The drawable the tag map names outright (a head, a hair or brow
    /// placeholder, a garment whose tag is its name prefix with the fit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub named: Option<String>,
    /// Asset GUID (`41acf8eb`) of the chosen state, first along the chain:
    /// for hair, brows, beards and lashes the strand-groom asset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// Flat colour of the chosen state (`46d26d93`, ARGB), when not zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 4]>,
    /// Colour word of the item's colour tag (`e60617ae`), first along the
    /// chain: black, blue, khaki … (`pedtint::COLOUR_WORDS`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour_tag: Option<&'static str>,
    /// The state the variation asks for (`StateName`), as hex.
    pub state: String,
    /// Colour and normal textures (`ead733fc`, `Normal`) of the chosen state
    /// of each chain item that names a colour texture, item first: a head's
    /// face. Many are in no store (the game builds body skin at run time).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skins: Vec<(String, String)>,
    /// Drawables that use the item's textures, with the number of textures
    /// shared (`pick` takes the best; `pedlook` prefers the model's own fit).
    #[serde(skip)]
    pub candidates: BTreeMap<String, usize>,
}

/// A part's tint, for `outfit --tint <drawable>=<spec>`.
#[derive(Debug, Clone, Serialize)]
pub struct PartTint {
    /// `<palette hex>:<row 0>.<row 1>.<row 2>`.
    pub spec: String,
    pub palette: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_name: Option<&'static str>,
    /// Rows of tint 0, 1, 2 (255 = off); the export applies tint 0.
    pub rows: [u8; 3],
    /// The item of the chain that set the bytes.
    pub from_item: String,
    /// That item names no palette, so the default one is used.
    pub default_palette: bool,
}

/// The tint of an item chain (item first, then parents).
pub fn chain_tint(chain: &[&Value]) -> Option<PartTint> {
    let f = crate::meta7a::field_name;
    let u = |c: &Value, h: u32| c[f(h)].as_u64().unwrap_or(0);
    let items: Vec<crate::pedtint::ChainItem> = chain.iter().map(|c| (u(c, 0x6512_1749) as u32, u(c, 0x5ecf_9331) as u32, [0xc4d7_5d79, 0xf4fb_3dec, 0xab5d_2a85].map(|h| u(c, h).min(255) as u8))).collect();
    let t = crate::pedtint::resolve(&items)?;
    Some(PartTint { spec: t.spec(), palette: format!("{:08x}", t.palette), palette_name: crate::pedtint::palette_name(t.palette), rows: t.rows, from_item: format!("{:08x}", t.item), default_palette: t.default_palette })
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
    /// Ids the variation lists that are not items of this resource: other
    /// variations it includes (an outfit names its head set, body set and
    /// body-shape preset this way).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<String>,
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

/// Three-character ped body-class token in a drawable name (e.g. `fr1`, `mr1`,
/// `fc1`, `mb1`, `mk1`, `mi1`).
fn body_class(name: &str) -> Option<&str> {
    name.split('_').find(|t| {
        let b = t.as_bytes();
        b.len() == 3 && matches!(b[0], b'f' | b'm') && matches!(b[1], b'r' | b'b' | b'o' | b'c' | b'k' | b'i' | b'0') && b[2].is_ascii_digit()
    })
}

/// The drawable that uses most of an item's textures (`_full` before cull
/// and LOD copies, then by name).
pub fn pick(counts: &BTreeMap<String, usize>) -> Option<String> {
    pick_for(counts, None, None)
}

/// Pick the best drawable for an item, preferring an explicit tag name when
/// present, and breaking texture-count ties by head prefix (for eyes/eyecaps),
/// ped body class (`fr1`, `mr1`, …), `_full`, then name.
pub fn pick_for(counts: &BTreeMap<String, usize>, ped_class: Option<&str>, head_prefix: Option<&str>) -> Option<String> {
    counts
        .iter()
        .max_by(|a, b| {
            let hp = |n: &str| head_prefix.is_some_and(|p| n.starts_with(p));
            let bc = |n: &str| ped_class.is_some_and(|c| body_class(n) == Some(c));
            a.1.cmp(b.1)
                .then(hp(a.0).cmp(&hp(b.0)))
                .then(bc(a.0).cmp(&bc(b.0)))
                .then(a.0.ends_with("_full").cmp(&b.0.ends_with("_full")))
                .then(b.0.cmp(a.0))
        })
        .map(|(n, _)| n.clone())
}

fn pick_with_name(counts: &BTreeMap<String, usize>, name: Option<&(String, String)>, ped_class: Option<&str>, head_prefix: Option<&str>) -> Option<String> {
    if let Some((prefix, fallback)) = name {
        let matched: BTreeMap<String, usize> = counts.iter().filter(|(k, _)| k.starts_with(prefix.as_str())).map(|(k, &v)| (k.clone(), v)).collect();
        return pick_for(&matched, ped_class, head_prefix).or_else(|| Some(fallback.clone()));
    }
    pick_for(counts, ped_class, head_prefix)
}

fn item_chain<'a>(it: &'a Value, items: &'a HashMap<u64, Value>) -> Vec<&'a Value> {
    let f = crate::meta7a::field_name;
    let mut chain = vec![it];
    while chain.len() < 8 {
        let last = chain[chain.len() - 1];
        let next = [0x080e_ec39u32, 0xe13e_baf8].iter().filter_map(|&k| last[f(k)].as_u64()).filter(|&x| x != 0).filter_map(|x| items.get(&x)).find(|n| !chain.iter().any(|c| std::ptr::eq(*c, *n)));
        match next {
            Some(n) => chain.push(n),
            None => break,
        }
    }
    chain
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
    // Infer the ped's body class (e.g. `fr1`, `mr1`) from any tag-resolved item
    // names in this resource, so texture-only ties on shared skin/clothing maps
    // stay on the right body class.
    let mut class_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for it in items.values() {
        for &k in &[0x4bfd_27ddu32, 0x023e_5a2f, 0x2a8c_db1d] {
            for x in tags(it, k) {
                if let Some(p) = pre.get(&x).filter(|p| p.0.contains('_')) {
                    if let Some(bc) = body_class(&p.0) {
                        *class_counts.entry(bc).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let ped_class: Option<String> = class_counts.into_iter().max_by_key(|&(_, n)| n).map(|(c, _)| c.to_string());

    let mut out = Vec::new();
    for v in get(0xa9d5_7751) {
        // the items the variation lists (with the state it asks for), and the
        // ids that are not items of this resource: variations it includes
        let mut includes = Vec::new();
        let mut states: Vec<u64> = Vec::new();
        let mut links: Vec<&Value> = Vec::new();
        for ln in v[f(0xfb5b_b20d)].as_array().into_iter().flatten() {
            let Some(id) = ln[f(0x6512_1749)].as_u64() else { continue };
            match items.get(&id) {
                Some(it) => {
                    links.push(it);
                    states.push(ln["StateName"].as_u64().unwrap_or(0));
                }
                None => includes.push(format!("{id:08x}")),
            }
        }
        // If this variation is a head set, capture the head's stem (walking
        // parent chains) so eyes / eyecaps / lashes tie-break to the same head
        // rather than alphabetically.
        let head_prefix: Option<String> = links.iter().find_map(|it| {
            item_chain(it, &items).into_iter().find_map(|c| {
                tags(c, 0x4bfd_27dd).into_iter().find_map(|x| {
                    let p = pre.get(&x)?;
                    let stem = p.0.strip_suffix("_full").unwrap_or(&p.0);
                    Some(format!("{stem}_"))
                })
            })
        });
        let mut parts = Vec::new();
        for (it, want_state) in links.into_iter().zip(states) {
            let chain = item_chain(it, &items);
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
            let drawable = pick_with_name(&counts, name, ped_class.as_deref(), head_prefix.as_deref());
            let named = name.filter(|p| p.0.split('_').any(crate::outfit::is_fit)).map(|p| p.1.clone());
            // the state the variation asks for, else the first, per chain item
            let state = |c: &Value| -> Option<Value> {
                let st = c[f(0xebac_f18c)].as_array()?;
                st.iter().find(|s| s["StateName"].as_u64() == Some(want_state)).or_else(|| st.first()).cloned()
            };
            let guid = chain.iter().filter_map(|c| state(c)).find_map(|s| s[f(0xe7cb_9e1b)][f(0x41ac_f8eb)].as_str().filter(|g| g.chars().any(|ch| ch != '0')).map(str::to_string));
            let skins: Vec<(String, String)> = chain
                .iter()
                .filter_map(|c| state(c))
                .filter_map(|s| {
                    let e = &s[f(0xe7cb_9e1b)];
                    let alb = e[f(0xead7_33fc)].as_str().filter(|g| g.chars().any(|ch| ch != '0'))?;
                    Some((alb.to_string(), e["Normal"].as_str().unwrap_or("0").to_string()))
                })
                .collect();
            let colour_tag = chain.iter().find_map(|c| tags(c, 0xe606_17ae).into_iter().find_map(crate::pedtint::colour_word)).map(|x| x.0);
            let colour = chain.iter().filter_map(|c| state(c)).find_map(|s| s[f(0x46d2_6d93)].as_u64().filter(|&x| x != 0)).map(|x| (x as u32).to_be_bytes());
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
            let tint = chain_tint(&chain);
            parts.push(Part { item: format!("{:08x}", it[f(0x6512_1749)].as_u64().unwrap_or(0)), category, name: name.map(|p| p.0.clone()), drawable, tex_remap, tags: tg, tint, named, guid, colour, colour_tag, state: format!("{want_state:08x}"), skins, candidates: counts });
        }
        let mut tags = BTreeMap::new();
        tag_words(&v, pre, &mut tags);
        out.push(Variation { id: format!("{:08x}", v[f(0x6512_1749)].as_u64().unwrap_or(0)), scale: v["Scale"].clone(), tags, parts, includes });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn chain_tint_reads_the_item_fields() {
        let f = crate::meta7a::field_name;
        let item = |id: u64, pal: u64, t: [u64; 3]| serde_json::json!({f(0x6512_1749): id, f(0x5ecf_9331): pal, f(0xc4d7_5d79): t[0], f(0xf4fb_3dec): t[1], f(0xab5d_2a85): t[2]});
        let (leaf, base) = (item(0xaf83_57ac, 0, [255; 3]), item(0x3274_9a86, 0xa7c8_d8e0, [0, 156, 6]));
        let t = super::chain_tint(&[&leaf, &base]).unwrap();
        assert_eq!((t.spec.as_str(), t.palette_name, t.from_item.as_str(), t.default_palette), ("a7c8d8e0:0.156.6", Some("ped_tint_generic_clean"), "32749a86", false));
        assert!(super::chain_tint(&[&leaf]).is_none());
    }

    #[test]
    fn pick_prefers_most_textures_then_full() {
        let c = [("top_fr1_000_cull_00".to_string(), 3), ("top_fr1_000_full".to_string(), 3), ("brows_mi1_000_full".to_string(), 1)].into_iter().collect();
        assert_eq!(super::pick(&c).as_deref(), Some("top_fr1_000_full"));
    }

    #[test]
    fn pick_for_breaks_ties_by_head_prefix_and_body_class() {
        let body = [("hand_fc1_000_full".to_string(), 2), ("hand_fr1_000_full".to_string(), 2)].into_iter().collect();
        assert_eq!(super::pick_for(&body, Some("fr1"), None).as_deref(), Some("hand_fr1_000_full"));

        let eyes = [("cs_a_head_fr1_000_eyes_full".to_string(), 2), ("head_fr1_001_y_eyes_full".to_string(), 2)].into_iter().collect();
        assert_eq!(super::pick_for(&eyes, Some("fr1"), Some("head_fr1_001_y_")).as_deref(), Some("head_fr1_001_y_eyes_full"));
    }

    #[test]
    fn pick_with_name_prefers_tag_prefix_over_shared_dummy_texture() {
        let dummy = [("brows_mi1_000_full".to_string(), 1), ("hair_fr1_000_full".to_string(), 1)].into_iter().collect();
        let tag = ("hair_fr1_000".to_string(), "hair_fr1_000_full".to_string());
        assert_eq!(super::pick_with_name(&dummy, Some(&tag), Some("fr1"), None).as_deref(), Some("hair_fr1_000_full"));

        let tied = [("top_fr1_027_slim_full".to_string(), 2), ("top_fr1_028_slim_full".to_string(), 2)].into_iter().collect();
        let tag28 = ("top_fr1_028".to_string(), "top_fr1_028_slim_full".to_string());
        assert_eq!(super::pick_with_name(&tied, Some(&tag28), Some("fr1"), None).as_deref(), Some("top_fr1_028_slim_full"));
    }

    #[test]
    fn prefixes_resolve_garment_names() {
        let m = super::prefixes(["tanktopcasual_fr1_028_disheveled_un_cull_00", "tanktopcasual_fr1_028_disheveled_un_full"].into_iter());
        let h = crate::names::joaat_exact("tanktopcasual_fr1_028");
        assert_eq!(m[&h], ("tanktopcasual_fr1_028".to_string(), "tanktopcasual_fr1_028_disheveled_un_full".to_string()));
    }
}

/// Per-model variation lists (a packs PSO: map of model name hash -> items
/// `a5ea6310` {`fc7eaa76` variation id, `fee182a6` outfit type, Scale}). For
/// the arcade zombies they list exactly the clothing sets, each with its own
/// body scale; the outfit type is shared between models (the female and male
/// tactical sets have the same one). Head sets are not listed. Returns
/// variation id -> (model hash, outfit type, scale).
pub fn variation_lists(pso_json: &Value) -> HashMap<u32, (u32, u32, f64)> {
    let mut out = HashMap::new();
    for (k, objs) in pso_json["objects"].as_object().into_iter().flatten() {
        if !k.starts_with("78262a67") {
            continue;
        }
        for o in objs.as_array().into_iter().flatten() {
            let Some(model) = o["Key"].as_u64() else { continue };
            for it in o["Item"].as_array().into_iter().flatten() {
                if let (Some(v), Some(c)) = (it[crate::meta7a::field_name(0xfc7e_aa76)].as_u64(), it[crate::meta7a::field_name(0xfee1_82a6)].as_u64()) {
                    out.insert(v as u32, (model as u32, c as u32, it["Scale"].as_f64().unwrap_or(1.0)));
                }
            }
        }
    }
    out
}

/// Dirt presets from common.rpf's graphics definitions (`<CGraphicsDefinitionsData>`,
/// plain XML): per `DirtDefinitions` item its scalar settings, the cloth and
/// skin modifiers, the colour (`ColorLookup` resolved from `CannedColors`, RGB
/// 0..255) and the alpha mask (`AlphaMasksLookup` resolved from `AlphaChannels`:
/// dirt-mask texture channels with probabilities). Keyed by preset name.
pub fn dirt_presets(xml: &str) -> HashMap<String, Value> {
    use regex::Regex;
    let section = |name: &str| -> &str {
        let (a, b) = (format!("<{name}>"), format!("</{name}>"));
        xml.find(&a).and_then(|i| xml[i..].find(&b).map(|j| &xml[i + a.len()..i + j])).unwrap_or("")
    };
    let num = |s: &str| s.parse::<f64>().ok();
    let colour_re = Regex::new(r#"(?s)<Item\s+key="([^"]+)"\s+w="([^"]+)"\s+x="([^"]+)"\s+y="([^"]+)"\s+z="([^"]+)""#).unwrap();
    let colours: HashMap<&str, Value> = colour_re.captures_iter(section("CannedColors")).map(|c| (c.get(1).unwrap().as_str(), serde_json::json!([num(&c[3]), num(&c[4]), num(&c[5])]))).collect();
    let item_re = Regex::new(r#"(?s)<Item key="([^"]+)">(.*?)\n    </Item>"#).unwrap();
    let chan_re = Regex::new(r#"(?s)<Probability value="([^"]+)" />\s*<Channel value="([^"]+)" />"#).unwrap();
    let masks: HashMap<&str, Value> = item_re
        .captures_iter(section("AlphaChannels"))
        .map(|c| (c.get(1).unwrap().as_str(), serde_json::json!(chan_re.captures_iter(c.get(2).unwrap().as_str()).map(|m| serde_json::json!({"channel": num(&m[2]), "probability": num(&m[1])})).collect::<Vec<_>>())))
        .collect();
    let val_re = Regex::new(r#"<(\w+) value="([^"]+)" />"#).unwrap();
    let block_re = Regex::new(r#"(?s)<(\w+DirtModifiers)>(.*?)</\w+DirtModifiers>"#).unwrap();
    let text_re = Regex::new(r#"<(AlphaMasksLookup|ColorLookup)>([^<]*)</"#).unwrap();
    let mut out = HashMap::new();
    for c in item_re.captures_iter(section("DirtDefinitions")) {
        let body = c.get(2).unwrap().as_str();
        let mut m = serde_json::Map::new();
        let mut rest = body.to_string();
        for b in block_re.captures_iter(body) {
            let v: serde_json::Map<String, Value> = val_re.captures_iter(&b[2]).map(|x| (x[1].to_string(), serde_json::json!(num(&x[2])))).collect();
            m.insert(b[1].to_string(), Value::Object(v));
            rest = rest.replace(&b[0], "");
        }
        for x in val_re.captures_iter(&rest) {
            let v = match &x[2] {
                "true" => serde_json::json!(true),
                "false" => serde_json::json!(false),
                s => serde_json::json!(num(s)),
            };
            m.insert(x[1].to_string(), v);
        }
        for t in text_re.captures_iter(body) {
            m.insert(t[1].to_string(), serde_json::json!(&t[2]));
            match &t[1] {
                "ColorLookup" => m.insert("color_rgb".into(), colours.get(&t[2]).cloned().unwrap_or(Value::Null)),
                _ => m.insert("mask_channels".into(), masks.get(&t[2]).cloned().unwrap_or(Value::Null)),
            };
        }
        out.insert(c[1].to_string(), Value::Object(m));
    }
    out
}

#[cfg(test)]
mod dirt_tests {
    #[test]
    fn dirt_preset_resolves_colour_and_mask() {
        let xml = "<CGraphicsDefinitionsData>\n  <CannedColors>\n    <Item\n      key=\"Blood_Red\"\n      w=\"-1.000000\"\n      x=\"86.000000\"\n      y=\"20.000000\"\n      z=\"24.000000\"\n     />\n  </CannedColors>\n  <AlphaChannels>\n    <Item key=\"DIRT_MASK_FILTHY\">\n      <Name>DIRT_MASK_FILTHY</Name>\n      <Channels>\n        <Item>\n          <Probability value=\"50.000000\" />\n          <Channel value=\"2\" />\n        </Item>\n      </Channels>\n    </Item>\n  </AlphaChannels>\n  <DirtDefinitions>\n    <Item key=\"AM_Dirtdefinition_BloodEffect\">\n      <AlphaCurve value=\"1.500000\" />\n      <SkinDirtModifiers>\n        <DirtBoostStrength value=\"0.500000\" />\n      </SkinDirtModifiers>\n      <AlphaMasksLookup>DIRT_MASK_FILTHY</AlphaMasksLookup>\n      <ColorLookup>Blood_Red</ColorLookup>\n      <IgnoreZoneColor value=\"true\" />\n    </Item>\n  </DirtDefinitions>\n</CGraphicsDefinitionsData>\n";
        let p = super::dirt_presets(xml);
        let b = &p["AM_Dirtdefinition_BloodEffect"];
        assert_eq!(b["color_rgb"], serde_json::json!([86.0, 20.0, 24.0]));
        assert_eq!(b["mask_channels"][0]["channel"], serde_json::json!(2.0));
        assert_eq!(b["SkinDirtModifiers"]["DirtBoostStrength"], serde_json::json!(0.5));
        assert_eq!(b["AlphaCurve"], serde_json::json!(1.5));
        assert_eq!(b["IgnoreZoneColor"], serde_json::json!(true));
    }
}
