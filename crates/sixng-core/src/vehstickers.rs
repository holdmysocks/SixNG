//! Vehicle liveries as data: which stickers a brand puts where.
//!
//! A livery is a "brand": a vehicle variation record (`fc1c7000`) such as
//! `BENSON_BRAND_BAUMHARDERDAIRY`. Like any variation it gives slots with
//! weighted part options and can build on parent variations (`03ae1cb0`).
//! Some of its parts list **sticker entries** (`86cdf3df`) in their
//! modifier list (`61e1e572`). An entry names
//!
//! - a sticker record (`ed9cf5df`, vehicle library): a texture by its
//!   64-bit id in the vehicle texture stores, colours, an aspect class;
//! - a layout group and a layout in it (one PSO file of data.rpf, structs
//!   `b0dd20cb` and `e2d9c73a`): the sticker zones it covers, a position, a
//!   size, a rotation in degrees and the UV set to use.
//!
//! A vertex's sticker zone is the fourth byte of the mesh's second colour
//! stream (GLB `_COLOR1`). The two-layer vehicle shader draws a sticker
//! where its transformed UV is between 0 and 1; the size acts as a scale
//! (a larger number is a smaller sticker).
//!
//! Where a sticker lands (`game_uv`, read from the game's packer, rva
//! 0x37766a0 at 0x3777c97): the shader's transform is a 3 x 3 matrix
//! applied to the mesh UV as a row vector, built as: translate by the
//! layout's position, rotate about (0.5, 0.5) by its rotation, scale about
//! (0.5, 0.5) by its size divided by a random factor from the sticker
//! record. The shader draws where the result is between 0 and 1. The
//! layout's UV mode k (1..4) picks the mesh's UV set k - 1 (the paint
//! shader reads the record's first byte at 00d78-00dcc); most use the
//! third set. A layout names zones only; on a vehicle built from parts they
//! may lie on a fitted part (the Benson's box) rather than the body.
//!
//! Not decoded: why some layouts name zones no vertex of the vehicle or
//! its parts carries (Buffalo gang liveries: 93-99; Rhode: 252, 253);
//! what the layout's `Location`, `80c88a97` and `9f8b8007`
//! mean (the game packs `80c88a97` beside the UV mode), and how the game
//! picks a brand for a vehicle.

use crate::meta7a::{self, Meta};
use crate::resource::Resource;
use anyhow::Result;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

const VARIATION: u32 = 0xfc1c_7000;
const OWNER: u32 = 0xfb1f_80ff;
const PARENTS: u32 = 0x03ae_1cb0;
const PART: u32 = 0x0cae_f606;
const PART_LIST: u32 = 0x61e1_e572;
const ENTRY: u32 = 0x86cd_f3df;
const ENTRY_GROUP: u32 = 0xb13f_2c2f;
const STICKER: u32 = 0xed9c_f5df;
const STICKER_TEXTURE: u32 = 0x3950_a760;
/// `colorRed`, `colorGreen`, `colorBlue` (hashed with their capitals): the
/// colours of the texture's red, green and blue mask channels.
const STICKER_COLOURS: [u32; 3] = [0x9b5d_cd73, 0xb575_2ae0, 0x9e67_9616];
/// A fourth colour, the same value on 5,531 of 5,650 records; its use is
/// not known.
const STICKER_OTHER_COLOUR: u32 = 0x9dbb_d803;
/// True on the records whose texture is a three-channel colour mask (an
/// opaque picture whose red, green and blue are the cover of `colorRed`,
/// `colorGreen` and `colorBlue`); false on pictures with colours and an
/// alpha channel of their own.
const STICKER_MASK: u32 = 0x54ec_9205;
const GROUP: u32 = 0xb0dd_20cb;
const GROUP_LAYOUTS: u32 = 0xc19c_0de4;
/// The list of groups in the layout file's root record.
const ROOT_GROUPS: u32 = 0x3274_d53f;
const LAYOUT_ZONES: u32 = 0x20b7_57b6;
const LAYOUT_POS: [u32; 2] = [0x26ba_b388, 0x79dd_59cc];
/// The size across and down: the game reads `9d2ff2b1` (+0xc) as the scale
/// of U and `8eed562c` (+0xe) as that of V.
const LAYOUT_SIZE: [u32; 2] = [0x9d2f_f2b1, 0x8eed_562c];
/// A sticker record's random ranges (f16, +0x38 and +0x3a): the size is
/// divided by lerp(`9a8a7565`, 1, r) and the rotation moves by up to
/// `27cf98d0` degrees either way, r drawn per vehicle.
const STICKER_SCALE_LO: u32 = 0x9a8a_7565;
const STICKER_TURN: u32 = 0x27cf_98d0;
const LAYOUT_ASPECT: u32 = 0x041e_bd1d;
const LAYOUT_UV: u32 = 0x4d76_3aa5;
const LAYOUT_OTHER: [u32; 2] = [0x80c8_8a97, 0x9f8b_8007];

fn get<'v>(v: &'v Value, h: u32) -> Option<&'v Value> {
    v.get(meta7a::field_name(h))
}

fn named<'v>(v: &'v Value, name: &str) -> Option<&'v Value> {
    get(v, meta7a::joaat(name))
}

/// A field of the layout file: its field names are hashed with their
/// capitals (`Name`, `Location`, `Rotation`), unlike the meta resources'.
fn cased<'v>(v: &'v Value, name: &str) -> Option<&'v Value> {
    get(v, crate::names::joaat_exact(name)).or_else(|| named(v, name))
}

fn u(v: Option<&Value>) -> u32 {
    v.and_then(|x| x.as_u64()).unwrap_or(0) as u32
}

fn list(v: Option<&Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default()
}

fn hex(h: u32) -> String {
    format!("{h:08x}")
}

fn name_of(h: u32) -> Option<String> {
    crate::names::all(h).into_iter().next().map(|x| x.0)
}

/// One resource's records: variations, part modifier lists, sticker
/// entries and sticker records, by name.
#[derive(Default)]
struct Records {
    variations: Vec<Value>,
    parts: HashMap<u32, Vec<u32>>,
    entries: HashMap<u32, Value>,
    stickers: HashMap<u32, Value>,
}

fn records(m: &Meta) -> Records {
    let dump = |h: u32, depth: u32| m.objects(h).into_iter().filter_map(|o| m.object(h, o, depth)).collect::<Vec<_>>();
    let by_name = |h: u32| dump(h, 0).into_iter().map(|v| (u(named(&v, "name")), v)).collect::<HashMap<u32, Value>>();
    Records {
        variations: dump(VARIATION, 3),
        parts: dump(PART, 0).iter().map(|p| (u(named(p, "name")), list(get(p, PART_LIST)))).filter(|(_, l)| !l.is_empty()).collect(),
        entries: by_name(ENTRY),
        stickers: by_name(STICKER),
    }
}

fn metas(ws: &crate::export::Workspace, archive: &str) -> Result<Vec<Records>> {
    let a = ws.resolve(archive)?;
    let ix = ws.load_index(&a)?;
    Ok(ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Resource && e.sub == "7a")
        .filter_map(|e| {
            let r = Resource::load_sys(&a.path, e).ok()?;
            let m = Meta::parse(&r)?;
            let has = |h: u32| m.structs.contains_key(&h);
            (has(VARIATION) || has(ENTRY) || has(STICKER)).then(|| records(&m))
        })
        .collect())
}

/// The layout groups of data.rpf: group name -> layout name -> layout.
fn layouts(ws: &crate::export::Workspace) -> Result<HashMap<u32, HashMap<u32, Value>>> {
    let a = ws.resolve("data")?;
    let ix = ws.load_index(&a)?;
    let found: Vec<Vec<Value>> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Pso)
        .filter_map(|e| {
            let p = crate::pso::parse(&crate::scan::read_entry(&a.path, e).ok()?)?;
            if !p.structs.contains_key(&GROUP) {
                return None;
            }
            let m = p.meta();
            // the groups are the elements of the root record's list
            let roots: Vec<u32> = p.structs.iter().filter(|(_, sd)| sd.members.iter().any(|x| x.hash == ROOT_GROUPS)).map(|(h, _)| *h).collect();
            let mut groups = Vec::new();
            for &h in &roots {
                for o in m.objects(h) {
                    if let Some(r) = m.object(h, o, 4) {
                        groups.extend(get(&r, ROOT_GROUPS).and_then(|x| x.as_array()).cloned().unwrap_or_default());
                    }
                }
            }
            Some(groups)
        })
        .collect();
    let mut out: HashMap<u32, HashMap<u32, Value>> = HashMap::new();
    for g in found.into_iter().flatten() {
        let e = out.entry(u(cased(&g, "Name"))).or_default();
        for l in get(&g, GROUP_LAYOUTS).and_then(|x| x.as_array()).into_iter().flatten() {
            e.entry(u(cased(l, "Name"))).or_insert_with(|| l.clone());
        }
    }
    Ok(out)
}

fn layout_json(l: &Value) -> Value {
    let f = |h: u32| get(l, h).and_then(|x| x.as_f64()).unwrap_or(0.0);
    json!({
        "zones": list(get(l, LAYOUT_ZONES)),
        "position": [f(LAYOUT_POS[0]), f(LAYOUT_POS[1])],
        "size": [f(LAYOUT_SIZE[0]), f(LAYOUT_SIZE[1])],
        "rotation": cased(l, "Rotation").and_then(|x| x.as_f64()).unwrap_or(0.0),
        "uv_mode": u(get(l, LAYOUT_UV)),
        "aspect": u(get(l, LAYOUT_ASPECT)),
        "location": hex(u(cased(l, "Location"))),
        "other": [u(get(l, LAYOUT_OTHER[0])), u(get(l, LAYOUT_OTHER[1]))],
    })
}

/// The game's UV transform of a placed sticker: (a, b, c, d, ou, ov), the
/// shader drawing it where u = U*a + V*b + ou and v = U*c + V*d + ov lie
/// in 0..1 (U, V: the mesh UV of the layout's set). `r`: the game's
/// per-vehicle random draws (0..1) for the scale and the turn; 0.5 is the
/// middle of both ranges (no turn).
pub fn game_uv(pos: [f32; 2], size: [f32; 2], degrees: f32, scale_lo: f32, turn: f32, r: [f32; 2]) -> [f32; 6] {
    let k = scale_lo + (1.0 - scale_lo) * r[0];
    let (sx, sy) = (size[0] / k, size[1] / k);
    let a = (degrees + turn * (2.0 * r[1] - 1.0)).to_radians();
    let (s, c) = a.sin_cos();
    let (px, py) = (pos[0] - 0.5, pos[1] - 0.5);
    [sx * c, -sx * s, sy * s, sy * c, sx * (c * px - s * py) + 0.5, sy * (s * px + c * py) + 0.5]
}

/// A texture id as stored in a sticker record (hex, with or without `0x`).
pub fn parse_texture_id(s: &str) -> Option<u64> {
    u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// Every sticker record of the vehicle library and the vehicle packs, one
/// entry per record name: `sticker` (name hash), `name` (when the game's
/// name list has it), `texture` (64-bit id in the vehicle texture stores),
/// `colours` (the record's `colorRed`, `colorGreen` and `colorBlue`, ARGB
/// as stored), `mask` (the picture is a colour mask: its red, green and
/// blue each take one of the colours), `aspect` and `record` (the record's
/// fields as read).
/// Sorted by name, records without a name last.
pub fn sticker_list(ws: &crate::export::Workspace) -> Result<Vec<Value>> {
    let mut seen: BTreeMap<u32, Value> = BTreeMap::new();
    for r in metas(ws, "data")?.iter().chain(metas(ws, "packs")?.iter()) {
        for (k, s) in &r.stickers {
            seen.entry(*k).or_insert_with(|| {
                json!({
                    "sticker": hex(*k),
                    "name": name_of(*k),
                    "texture": get(s, STICKER_TEXTURE).and_then(|x| x.as_str()),
                    "colours": [hex(u(named(s, "colorRed"))), hex(u(named(s, "colorGreen"))), hex(u(named(s, "colorBlue")))],
                    "mask": get(s, STICKER_MASK).and_then(|x| x.as_bool()).unwrap_or(false),
                    "aspect": u(named(s, "aspectRatio")),
                    "record": s,
                })
            });
        }
    }
    let mut out: Vec<Value> = seen.into_values().collect();
    out.sort_by(|a, b| (a["name"].is_null(), a["name"].as_str(), a["sticker"].as_str()).cmp(&(b["name"].is_null(), b["name"].as_str(), b["sticker"].as_str())));
    Ok(out)
}

/// Decode one sticker texture, by its 64-bit id, from the texture stores to
/// `<dir>/<id as 16 hex digits>.png`. A file that is there is kept, so `dir`
/// works as a cache. Returns the file and the picture's size.
pub fn sticker_png(game: &std::path::Path, id: u64, dir: &std::path::Path) -> Result<(std::path::PathBuf, [u32; 2])> {
    let (store, t) = crate::ttpf::global(game).get(id).ok_or_else(|| anyhow::anyhow!("texture {id:016x} is in no texture store"))?;
    let (p, st) = crate::ttpf::write_png(store, t, dir, false)?;
    Ok((p, [st.width, st.height]))
}

/// Write `<out>/vehicles/stickers.json` (the sticker list) and, with
/// `textures`, every sticker texture as `<out>/vehicles/stickers/<id>.png`.
/// `only`: a filter on the sticker's name or name hash.
pub fn export_stickers(ws: &crate::export::Workspace, out: &std::path::Path, textures: bool, only: Option<&regex::Regex>) -> Result<Value> {
    let mut list = sticker_list(ws)?;
    if let Some(re) = only {
        list.retain(|s| re.is_match(s["name"].as_str().unwrap_or("")) || re.is_match(s["sticker"].as_str().unwrap_or("")) || re.is_match(s["texture"].as_str().unwrap_or("")));
    }
    let dir = out.join("vehicles");
    std::fs::create_dir_all(&dir)?;
    let mut ids: Vec<u64> = list.iter().filter_map(|s| parse_texture_id(s["texture"].as_str()?)).filter(|id| *id != 0).collect();
    ids.sort_unstable();
    ids.dedup();
    let (mut written, mut failed) = (0usize, Vec::new());
    if textures {
        let tdir = dir.join("stickers");
        let res: Vec<(u64, Result<[u32; 2]>)> = ids.par_iter().map(|id| (*id, sticker_png(&ws.game, *id, &tdir).map(|r| r.1))).collect();
        let sizes: HashMap<u64, [u32; 2]> = res.iter().filter_map(|(id, r)| r.as_ref().ok().map(|s| (*id, *s))).collect();
        written = sizes.len();
        failed = res.iter().filter_map(|(id, r)| r.as_ref().err().map(|e| json!({"texture": format!("{id:016x}"), "error": format!("{e:#}")}))).collect();
        for s in &mut list {
            if let Some(sz) = s["texture"].as_str().and_then(parse_texture_id).and_then(|id| sizes.get(&id)) {
                s["size"] = json!(sz);
                s["png"] = json!(format!("stickers/{:016x}.png", parse_texture_id(s["texture"].as_str().unwrap_or("")).unwrap_or(0)));
            }
        }
    }
    let stats = json!({"stickers": list.len(), "named": list.iter().filter(|s| s["name"].is_string()).count(), "textures": ids.len(), "written": written, "failed": failed.len()});
    let doc = json!({
        "format": 1,
        "note": "Sticker records of the vehicle library and packs. texture = 64-bit id in the vehicle texture stores (data/ttdb/vehicles); colours = colorRed, colorGreen, colorBlue (ARGB as stored); mask = the picture is an opaque three-channel colour mask (red, green and blue are the cover of the three colours), else a picture with its own colours and alpha; aspect = the record's aspect class; record = the record's fields as read. png and size are set when the textures were written.",
        "stats": stats,
        "failed": failed,
        "stickers": list,
    });
    crate::atomic::write(dir.join("stickers.json"), serde_json::to_vec(&doc)?)?;
    Ok(stats)
}

/// Per vehicle its brands, each with the stickers it places (see `build`),
/// and the counts of layout groups, layouts and sticker records read.
/// `models`: joaat -> model name; a vehicle not in it is keyed by its
/// name hash (8 hex digits).
pub fn liveries(ws: &crate::export::Workspace, models: &HashMap<u32, String>) -> Result<(BTreeMap<String, Vec<Value>>, [usize; 3])> {
    let library = metas(ws, "data")?;
    let packs = metas(ws, "packs")?;
    let groups = layouts(ws)?;
    // library records by name (parents, shared parts, stickers)
    let mut lib_var: HashMap<u32, &Value> = HashMap::new();
    let mut lib_parts: HashMap<u32, &Vec<u32>> = HashMap::new();
    let mut lib_entries: HashMap<u32, &Value> = HashMap::new();
    let mut stickers: HashMap<u32, &Value> = HashMap::new();
    for r in &library {
        lib_var.extend(r.variations.iter().map(|v| (u(named(v, "name")), v)));
        lib_parts.extend(r.parts.iter().map(|(k, v)| (*k, v)));
        lib_entries.extend(r.entries.iter().map(|(k, v)| (*k, v)));
        stickers.extend(r.stickers.iter().map(|(k, v)| (*k, v)));
    }
    let mut vehicles: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for r in &packs {
        let own: HashMap<u32, &Value> = r.variations.iter().map(|v| (u(named(v, "name")), v)).collect();
        for v in &r.variations {
            let owner = u(get(v, OWNER));
            if owner == 0 {
                continue;
            }
            // the variation's parts: its parents' first (a slot of the
            // child replaces the parent's), then its own
            let mut chain: Vec<&Value> = Vec::new();
            let mut todo = vec![v];
            while let Some(x) = todo.pop().filter(|_| chain.len() < 16) {
                chain.push(x);
                todo.extend(list(get(x, PARENTS)).iter().filter_map(|p| own.get(p).or_else(|| lib_var.get(p)).copied()));
            }
            let mut slots: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for x in chain.iter().rev() {
                for s in named(x, "slots").and_then(|s| s.as_array()).into_iter().flatten() {
                    let mods: Vec<u32> = named(s, "options").and_then(|o| o.as_array()).into_iter().flatten().map(|o| u(named(o, "mod"))).collect();
                    slots.insert(u(named(s, "slot")), mods);
                }
            }
            let mut placed: Vec<Value> = Vec::new();
            for part in slots.values().flatten() {
                let Some(mods) = r.parts.get(part).or_else(|| lib_parts.get(part).copied()) else { continue };
                for e in mods.iter().filter_map(|m| r.entries.get(m).or_else(|| lib_entries.get(m).copied())) {
                    let (sticker, group, layout) = (u(named(e, "sticker")), u(get(e, ENTRY_GROUP)), u(named(e, "layout")));
                    let s = r.stickers.get(&sticker).or_else(|| stickers.get(&sticker).copied());
                    let l = groups.get(&group).and_then(|g| g.get(&layout));
                    let f = |v: Option<&Value>, h: u32| v.and_then(|v| get(v, h)).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    let uv = l.map(|l| {
                        let lo = s.and_then(|s| get(s, STICKER_SCALE_LO)).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
                        let deg = cased(l, "Rotation").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                        game_uv([f(Some(l), LAYOUT_POS[0]), f(Some(l), LAYOUT_POS[1])], [f(Some(l), LAYOUT_SIZE[0]), f(Some(l), LAYOUT_SIZE[1])], deg, lo, f(s, STICKER_TURN), [0.5, 0.5])
                    });
                    placed.push(json!({
                        "part": hex(*part),
                        "sticker": hex(sticker),
                        "texture": s.and_then(|s| get(s, STICKER_TEXTURE)).and_then(|x| x.as_str()),
                        "colours": s.map(|s| STICKER_COLOURS.iter().map(|h| hex(u(get(s, *h)))).collect::<Vec<_>>()),
                        "other_colour": s.map(|s| hex(u(get(s, STICKER_OTHER_COLOUR)))),
                        "group": hex(group),
                        "layout_name": hex(layout),
                        "layout": l.map(layout_json),
                        "mask": s.and_then(|s| get(s, STICKER_MASK)).and_then(|x| x.as_bool()),
                        "scale_lo": s.and_then(|s| get(s, STICKER_SCALE_LO)).and_then(|x| x.as_f64()),
                        "turn": s.and_then(|s| get(s, STICKER_TURN)).and_then(|x| x.as_f64()),
                        "uv": uv,
                    }));
                }
            }
            if placed.is_empty() {
                continue;
            }
            let name = u(named(v, "name"));
            let model = models.get(&owner).cloned().unwrap_or_else(|| hex(owner));
            vehicles.entry(model).or_default().push(json!({"brand": hex(name), "name": name_of(name), "stickers": placed}));
        }
    }
    for b in vehicles.values_mut() {
        b.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()).then(a["brand"].as_str().cmp(&b["brand"].as_str())));
        b.dedup_by(|a, b| a["brand"] == b["brand"]);
    }
    Ok((vehicles, [groups.len(), groups.values().map(|g| g.len()).sum::<usize>(), stickers.len()]))
}

/// Write `<out>/vehicles/liveries.json`: per vehicle its brands, each with
/// the stickers it places. `models`: joaat -> model name.
pub fn build(ws: &crate::export::Workspace, models: &HashMap<u32, String>, out: &std::path::Path) -> Result<Value> {
    let (vehicles, [n_groups, n_layouts, n_records]) = liveries(ws, models)?;
    // counted once per vehicle: a vehicle's records can sit in two packs
    let all = || vehicles.values().flatten();
    let n_brands = all().count();
    let n_stickers: usize = all().map(|b| b["stickers"].as_array().map_or(0, |s| s.len())).sum();
    let n_unplaced = all().flat_map(|b| b["stickers"].as_array().into_iter().flatten()).filter(|s| s["layout"].is_null()).count();
    let stats = json!({"vehicles": vehicles.len(), "brands": n_brands, "stickers": n_stickers, "without_layout": n_unplaced, "layout_groups": n_groups, "layouts": n_layouts, "sticker_records": n_records});
    let doc = json!({
        "format": 3,
        "note": "Vehicle liveries. Per vehicle its brands (variation records whose parts place stickers), each with its stickers: the part that carries the entry, the sticker record, its texture (64-bit id in the vehicle texture stores), colours (ARGB as stored: colorRed, colorGreen and colorBlue, the colours of the texture's red, green and blue mask channels; 00000000 where a channel has none) and other_colour (a fourth stored colour, use not known), and the layout: zones = the sticker zones it covers (a vertex's zone is the fourth byte of the mesh's second colour stream, GLB _COLOR1), position and size as stored, rotation in degrees, uv_mode (k = 1..4: the mesh's UV set k - 1, as the paint shader picks it; 3, the third set, for most), aspect (0 square, 1 two to one, 2 four to one). size is (U, V): the game reads 9d2ff2b1 as U's and 8eed562c as V's. uv = the game's transform (a, b, c, d, ou, ov): the sticker is drawn where u = U*a + V*b + ou and v = U*c + V*d + ov lie in 0..1, built as translate by the position, rotate about (0.5, 0.5), scale about (0.5, 0.5) by size / lerp(scale_lo, 1, r), the rotation moved by turn * (2r' - 1) degrees; r and r' are per-vehicle random draws, taken as 0.5 here (game packer rva 0x37766a0). mask: the picture is a colour mask. location and other are not understood. A brand also lists the stickers of its parent variations. How the game picks a brand for a vehicle is not known.",
        "stats": stats,
        "vehicles": vehicles,
    });
    let dir = out.join("vehicles");
    std::fs::create_dir_all(&dir)?;
    crate::atomic::write(dir.join("liveries.json"), serde_json::to_vec(&doc)?)?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    #[test]
    fn texture_ids_parse_as_hex() {
        assert_eq!(super::parse_texture_id("112afea77897b738"), Some(0x112a_fea7_7897_b738));
        assert_eq!(super::parse_texture_id(" 0x10 "), Some(16));
        assert_eq!(super::parse_texture_id("zz"), None);
    }

    #[test]
    fn game_uv_translates_rotates_and_scales_about_the_middle() {
        let near = |a: [f32; 6], b: [f32; 6]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        // nothing moved: the mesh UV itself
        assert!(near(super::game_uv([0.0, 0.0], [1.0, 1.0], 0.0, 1.0, 0.0, [0.5, 0.5]), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
        // the sticker's middle (0.5, 0.5) lies at mesh UV 0.5 - position,
        // whatever the size and turn
        let m = super::game_uv([0.2, -0.1], [3.0, 2.0], 30.0, 1.0, 0.0, [0.5, 0.5]);
        let (u, v) = (0.3, 0.6);
        assert!((u * m[0] + v * m[1] + m[4] - 0.5).abs() < 1e-5 && (u * m[2] + v * m[3] + m[5] - 0.5).abs() < 1e-5);
        // a quarter turn, size 2 in U: U goes to v, V to -u, twice as fast in u
        assert!(near(super::game_uv([0.0, 0.0], [2.0, 1.0], 90.0, 1.0, 0.0, [0.5, 0.5]), [0.0, -2.0, 1.0, 0.0, 1.5, 0.0]));
        // the random factor: size / lerp(lo, 1, r); the turn: +-turn
        assert!(near(super::game_uv([0.0, 0.0], [1.0, 1.0], 0.0, 0.5, 0.0, [0.0, 0.5]), [2.0, 0.0, 0.0, 2.0, -0.5, -0.5]));
        assert!(near(super::game_uv([0.0, 0.0], [1.0, 1.0], 0.0, 1.0, 90.0, [0.5, 1.0]), super::game_uv([0.0, 0.0], [1.0, 1.0], 90.0, 1.0, 0.0, [0.5, 0.5])));
    }
}
