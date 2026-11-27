//! Spec maps of ped skin: the `speculartex` and `speculartex2` textures of
//! the body, hand and feet meshes (and of any other skin a wardrobe item
//! names), from the game's material assets.
//!
//! The skin meshes (`hand_*` = upper body, `feet_*` = lower body) use the
//! `ped_standard_skin` shader. Every main mesh of every fit binds a colour
//! map and two spec maps (`speculartex` = slot `c6f46e79`, `speculartex2` =
//! slot `739e3d0d`) that are in no texture store: placeholders. Only the
//! normal map is stored. The wardrobe item gives the real textures: each
//! texture state of an item has an `Asset` (struct `147fdab5`) with
//! `Albedo`, `Normal` and `Material`, a 64-bit material asset id.
//!
//! The material assets are packs.rpf PSO files (47 of them), each an asset
//! set (root struct `1edee47e`) with hash maps of material records (struct
//! `28689f25`: `guid` and a keyed texture list `53760af2`). Two keys hold
//! the spec maps:
//! * `179eb44a` -> `speculartex`
//! * `4cf41ef4` -> `speculartex2`
//!
//! Checked on the stored first-person hands: the record of
//! `hand_fb1_000_nude_cull_fp`'s material names the very two textures that
//! mesh binds. Other keys of the list (`7afafb01`, `aaccd95c`, `c8126724`,
//! `aeadd0ac`, `c849b74a`) hold colour, normal and wrinkle textures; they
//! are not used here.
//!
//! A skin is picked by its colour texture, so the table maps each wardrobe
//! `Albedo` to its `Material` (`by_albedo`), and each material id to its
//! two spec maps (`materials`). A colour texture that states name with more
//! than one material (shared defaults, such as a jewellery colour map used
//! by about 300 materials) does not say which material is meant, so it is
//! left out and counted in `conflicts`. Built once from the packs.rpf index (meta
//! resources and PSO files) and cached as `<work>/index/skin_spec.json`.
//!
//! The channels, from the skin pixel program (`PS_DeferredPedTexturedNC`):
//! * `speculartex` R: translucency mask, `saturate(R - c)` written to a
//!   G-buffer target of its own (non-zero on thin parts such as fingers).
//! * `speculartex` G: roughness. Overlay layers blend it toward their own
//!   value, the detail map adjusts it, and the G-buffer stores 1 - roughness.
//! * `speculartex` B: occlusion, `lerp(1, B, strength)`, stored in 6 bits.
//! * `speculartex2` R: detail strength mask.
//! * `speculartex2` G: skin region (`G * 16`, high nibble 0-15). A
//!   per-draw table maps the region to a layer of the detail-normal
//!   and detail-control atlases.
//!
//! The maps are carried as they are (export material extras, viewer slots).

use crate::meta7a::{field_name, Meta};
use crate::resource::Resource;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const CACHE_VERSION: u32 = 2;
/// Wardrobe item (meta resources).
const ITEM: u32 = 0xf795_e8ba;
/// Material record (PSO asset sets).
const MATERIAL: u32 = 0x2868_9f25;
/// Keyed texture list of a material record.
const TEXTURES: u32 = 0x5376_0af2;
/// Keys of the spec maps in a material record's texture list.
pub const KEY_SPEC: u32 = 0x179e_b44a;
pub const KEY_SPEC2: u32 = 0x4cf4_1ef4;
/// The shader slots the two maps go in.
pub const SLOT_SPEC: &str = "c6f46e79";
pub const SLOT_SPEC2: &str = "739e3d0d";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SkinSpec {
    version: u32,
    /// Material asset id -> [speculartex, speculartex2] (0 = none).
    pub materials: BTreeMap<u64, [u64; 2]>,
    /// Wardrobe colour texture -> the material asset id its state names.
    pub by_albedo: BTreeMap<u64, u64>,
    /// Colour textures whose states name more than one material with spec
    /// maps (left out of `by_albedo`).
    #[serde(default)]
    pub conflicts: usize,
}

/// The spec maps of a skin: material asset id and the two textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub material: u64,
    pub spec: u64,
    pub spec2: u64,
}

impl SkinSpec {
    /// The spec maps for the skin with this colour texture.
    pub fn of_albedo(&self, albedo: u64) -> Option<Spec> {
        let material = *self.by_albedo.get(&albedo)?;
        let [spec, spec2] = *self.materials.get(&material)?;
        Some(Spec { material, spec, spec2 }).filter(|s| s.spec != 0 || s.spec2 != 0)
    }
}

fn hex(v: &Value) -> u64 {
    v.as_str().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0)
}

/// The spec pair of one material record (its keyed texture list).
pub fn record_spec(rec: &Value) -> [u64; 2] {
    let mut out = [0u64; 2];
    for e in rec[field_name(TEXTURES).as_str()].as_array().into_iter().flatten() {
        match e["Key"].as_u64().map(|k| k as u32) {
            Some(KEY_SPEC) => out[0] = hex(&e["Item"]),
            Some(KEY_SPEC2) => out[1] = hex(&e["Item"]),
            _ => {}
        }
    }
    out
}

/// Every material record in a decoded PSO (`Pso::to_json`), with its spec pair.
pub fn walk_materials(v: &Value, out: &mut Vec<(u64, [u64; 2])>) {
    match v {
        Value::Object(o) => {
            if o.get("_struct").and_then(Value::as_str).is_some_and(|s| s.ends_with(&format!("{MATERIAL:08x}"))) {
                let g = hex(&v["guid"]);
                let s = record_spec(v);
                if g != 0 && s != [0, 0] {
                    out.push((g, s));
                }
            }
            o.values().for_each(|x| walk_materials(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| walk_materials(x, out)),
        _ => {}
    }
}

/// Every texture state `Asset` in a decoded wardrobe item: (albedo, material).
pub fn walk_assets(v: &Value, out: &mut Vec<(u64, u64)>) {
    match v {
        Value::Object(o) => {
            let (a, m) = (hex(&v[field_name(0xead7_33fc).as_str()]), hex(&v[field_name(crate::names::joaat_exact("Material")).as_str()]));
            if a != 0 && m != 0 {
                out.push((a, m));
            }
            o.values().for_each(|x| walk_assets(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| walk_assets(x, out)),
        _ => {}
    }
}

/// Link each colour texture to the one material (with spec maps) its states
/// name. Colour textures named with more than one such material are left
/// out; their count is returned too.
fn link(assets: impl Iterator<Item = (u64, u64)>, materials: &BTreeMap<u64, [u64; 2]>) -> (BTreeMap<u64, u64>, usize) {
    let mut all: BTreeMap<u64, std::collections::BTreeSet<u64>> = BTreeMap::new();
    for (a, m) in assets.filter(|(_, m)| materials.contains_key(m)) {
        all.entry(a).or_default().insert(m);
    }
    let conflicts = all.values().filter(|m| m.len() > 1).count();
    let by_albedo = all.into_iter().filter_map(|(a, m)| (m.len() == 1).then(|| (a, *m.first().unwrap()))).collect();
    (by_albedo, conflicts)
}

fn build(packs: &Path, ix: &crate::scan::ArchiveIndex) -> SkinSpec {
    let found: Vec<(Vec<(u64, [u64; 2])>, Vec<(u64, u64)>)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a" || e.kind == crate::classify::Kind::Pso)
        .map(|e| {
            let (mut mats, mut assets) = (Vec::new(), Vec::new());
            if e.kind == crate::classify::Kind::Pso {
                let Some(p) = crate::scan::read_entry(packs, e).ok().and_then(|d| crate::pso::parse(&d)) else { return (mats, assets) };
                // only the asset sets: the others are large and hold none
                if p.structs.contains_key(&MATERIAL) {
                    walk_materials(&p.to_json(), &mut mats);
                }
            } else {
                let Ok(r) = Resource::load_meta(packs, e) else { return (mats, assets) };
                let Some(m) = Meta::parse(&r) else { return (mats, assets) };
                for o in m.objects(ITEM) {
                    if let Some(it) = m.object(ITEM, o, 4) {
                        walk_assets(&it, &mut assets);
                    }
                }
            }
            (mats, assets)
        })
        .collect();
    let mut t = SkinSpec { version: CACHE_VERSION, ..Default::default() };
    for (mats, _) in &found {
        for &(g, s) in mats {
            t.materials.entry(g).or_insert(s);
        }
    }
    let (by_albedo, conflicts) = link(found.iter().flat_map(|(_, a)| a.iter().copied()), &t.materials);
    t.by_albedo = by_albedo;
    t.conflicts = conflicts;
    t
}

/// The skin spec table (built from the packs.rpf index on first use, then
/// cached in the work directory).
pub fn get(game_root: &Path) -> Result<&'static SkinSpec, String> {
    static T: OnceLock<Result<SkinSpec, String>> = OnceLock::new();
    T.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let cache = work.join("index").join("skin_spec.json");
        let ixp = crate::scan::index_path(&work.join("index"), "packs.rpf");
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<SkinSpec>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return Ok(t);
                }
            }
        }
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("packs.rpf index ({}): {e}; index packs.rpf first", ixp.display()))?;
        let t = build(&game_root.join("packs.rpf"), &ix);
        if t.materials.is_empty() {
            return Err("no material assets with spec maps found in packs.rpf".into());
        }
        if let Ok(b) = serde_json::to_vec(&t) {
            let _ = crate::atomic::write(&cache, b);
        }
        Ok(t)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_hashes() {
        assert_eq!(KEY_SPEC, 396_276_810);
        assert_eq!(KEY_SPEC2, 1_291_067_124);
        // the wardrobe asset fields as the meta decoder names them (the
        // material id is the case-sensitive hash of `Material`)
        assert_eq!(field_name(crate::names::joaat_exact("Material")), "Material");
    }

    #[test]
    fn shared_colour_maps_are_left_out() {
        let mats = BTreeMap::from([(10, [1, 2]), (11, [3, 4]), (12, [5, 6])]);
        // 1 -> one material (twice), 2 -> two materials, 3 -> a material without spec maps
        let (by, n) = link([(1, 10), (1, 10), (2, 11), (2, 12), (3, 99)].into_iter(), &mats);
        assert_eq!(by, BTreeMap::from([(1, 10)]));
        assert_eq!(n, 1);
    }

    #[test]
    fn material_records_give_their_spec_pair() {
        // a record as `Pso::to_json` writes it (the female upper-body skin's)
        let k = field_name(TEXTURES);
        let rec = json!({"_struct": "28689f25", "guid": "79a6a00eabdb393e", k.clone(): [
            {"Item": "603b8715c4624666", "Key": KEY_SPEC, "_struct": "00000103"},
            {"Item": "79e9d1c0a266e986", "Key": KEY_SPEC2, "_struct": "00000103"},
        ]});
        let colour = json!({"_struct": "28689f25", "guid": "1111111111111111", k: [{"Item": "2222222222222222", "Key": 2_063_268_609u32}]});
        let doc = json!({"root": null, "objects": {"e532cef000000101": [{"93c495af": [{"Item": rec, "Key": "79a6a00eabdb393e"}, {"Item": colour, "Key": "1111111111111111"}]}]}});
        let mut out = Vec::new();
        walk_materials(&doc, &mut out);
        assert_eq!(out, vec![(0x79a6_a00e_abdb_393e, [0x603b_8715_c462_4666, 0x79e9_d1c0_a266_e986])]);
    }

    #[test]
    fn wardrobe_assets_link_albedo_to_material() {
        let asset = |a: &str, m: &str| json!({"_struct": "147fdab5", field_name(0xead7_33fc): a, field_name(crate::names::joaat_exact("Material")): m, "Normal": "9d337dabca636906"});
        let item = json!({"states": [{"Asset": asset("cb95198f8a74cb70", "79a6a00eabdb393e")}, {"Asset": asset("f108653387cdfddd", "0000000000000000")}]});
        let mut out = Vec::new();
        walk_assets(&item, &mut out);
        assert_eq!(out, vec![(0xcb95_198f_8a74_cb70, 0x79a6_a00e_abdb_393e)]);
        let mut t = SkinSpec::default();
        t.materials.insert(0x79a6_a00e_abdb_393e, [1, 2]);
        t.by_albedo.insert(0xcb95_198f_8a74_cb70, 0x79a6_a00e_abdb_393e);
        assert_eq!(t.of_albedo(0xcb95_198f_8a74_cb70), Some(Spec { material: 0x79a6_a00e_abdb_393e, spec: 1, spec2: 2 }));
        assert_eq!(t.of_albedo(5), None);
    }
}
