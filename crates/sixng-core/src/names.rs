//! Name dictionary for 32-bit name hashes.
//!
//! Shader names, shader parameter names and meta (0x7a) field names are all
//! joaat of the lower-case name (`meta7a::joaat`). The names themselves are
//! read from the user's own install by `sixng names` (`build`) and saved in
//! `<work>/names/names.json`; no game strings ship with the tool. The
//! built-in list below holds only names that were found by searching word
//! combinations against hashes (they appear nowhere in the game files).
//!
//! Sources, by the order they are preferred in:
//! * `common`: identifiers in the text and XML entries of `common.rpf`
//!   (the shader list, meta field names, enum values, material names);
//! * `shaders`: NUL-terminated identifiers in `shaders.rpf` (technique,
//!   sampler, texture and variable names);
//! * `derived`: the built-in list.
use crate::meta7a::joaat;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Names found by searching word combinations (verified only by the hash
/// match; each name is a plausible texture or variable name).
/// Case-sensitive field names (joaat of the name as written) confirmed in
/// PSO schemas and the vehicle meta resource: GTA V handling and
/// vehicles.meta field names.
pub const DERIVED_EXACT: &[&str] = &["fMass", "fInitialDragCoeff", "fDriveBiasFront", "fDriveBiasRear", "fBrakeForce", "fHandBrakeForce", "fSteeringLock", "fLowSpeedTractionLossMult", "fSuspensionCompDamp", "fSuspensionReboundDamp", "fSuspensionUpperLimit", "fSuspensionLowerLimit", "fSuspensionRaise", "fAntiRollBarForce", "fCollisionDamageMult", "fWeaponDamageMult", "fDeformationDamageMult", "fEngineDamageMult", "fPetrolTankVolume", "fOilVolume", "fPetrolConsumptionRate", "fStoppieBalancePoint", "fWheelieBalancePoint", "fBackEndPopUpBuildingImpulseMult", "fBackEndPopUpCarImpulseMult", "fBackEndPopUpMaxDeltaSpeed", "fSideSlipMult", "fPitchMult", "fYawMult", "fRollMult", "fLiftMult", "fMoveRes", "fThrust", "fThrustFallOff", "fWeaponDamageScaledToVehHealthMult", "fWheelMass", "fGearRatios", "fFinalDriveRatio", "Wheels", "Lights", "Sirens", "modelName", "metallic", "colors", "Color", "Name", "Type", "Key", "Item", "Flags", "Value", "Data", "Items", "Index", "Entries", "Count", "dirtLevelMax", "gameName", "buoyancySphereOffset", "minSeatHeight", "envEffScaleMin", "NmBraceOverrideSet", "layout", "type", "frequency", "plateType", "audioNameHash", "animConvRoofWindowsAffected", "vehicleMakeName", "damageMapScale", "bumpersNeedToCollideWithMap", "AllowJoyriding", "maxNum", "pretendOccupantsScale", "animConvRoofName", "animConvRoofDictName", "explosionInfo", "wheelScaleRear", "dirtLevelMin", "vehicleClass", "swankness", "needsRopeTexture", "ptfxAssetName", "wheelScale", "vfxInfoName", "visibleSpawnDistScale", "wheelType", "rewards", "envEffScaleMin2", "doorsWithCollisionWhenClosed", "defaultBodyHealth", "envEffScaleMax", "lodDistances", "txdName", "AllowSundayDriving", "driveableDoors", "HDTextureDist", "envEffScaleMax2", "steerWheelMult", "buoyancySphereSizeScale", "weaponForceMult", "pOverrideRagdollThreshold", "identicalModelSpawnDistance", "maxNumOfSameColor", "handlingId", "damageOffsetScale",
    // map, light, vehicle, handling and material fields resolved against the
    // hashes in the format notes
    "timeFlags", "lightType", "handling", "slots", "options", "probability", "ParentName", "HandlingData", "StateName",
    // script-doc fields (01393259 element / attribute) and the head-skin States list; joaat of the name, 2026-11-23
    "valueHash", "contentType", "childIndex", "States"];

pub const DERIVED: &[&str] = &[
    // texture parameters of the minigame pack's water and interior shaders (joaat search, 2026-11-23)
    "watersurfacedetailtexture", "surfacedetailnormal0", "surfacedetailnormal1", "surfacedetailnormal2", "surfacedetailiridescence", "interiordiffusemaskmap", "interiorlayerdiffusemap",
    // shader constants of the 4-layer ground shaders (dictionary search, 2026-11-22)
    "blendsoftness", "bumpiness", "fresnel", "puddlemaskscale", "tintblendamount", "tintbump", "tintbumpuvscalebias", "tintintensity", "tintroughnessbias", "verttintintensity",
    "anisotex", "anisotexture", "blendshapedata", "bloomsource", "bloomtarget", "boxblurtexture", "bumptex", "bumptexture", "bumptexture2",
    "bumptexture_layer0", "bumptexture_layer1", "bumptexture_layer2", "bumptexture_layer3", "cableconstants", "clearcoattex", "clearcoattexture",
    "cloudmaplut", "cloudvolume", "colorlut", "depthbuffer", "depthmap", "depthtexture", "detailtexture", "diffractiontex", "diffusesampler",
    "diffusetex", "diffusetex2", "diffusetexanimatedblend", "diffusetexture", "diffusetexture_layer0", "diffusetexture_layer1",
    "diffusetexture_layer2", "diffusetexture_layer3", "dirtdiffusetex", "dirtnoisetex", "distancemap", "dusttexture", "emissiveanimatedtexture",
    "emissivedistortiontexture", "emissivetex", "emissivetexture", "exposuretexture", "extinctionlut", "flowtexture", "foamtexture", "fogtexture",
    "furnoisetexture", "g_skyparaboloid", "g_skyradiancecache", "gradtexture", "grassmasktexture", "heightmapdata", "heightmaptexture",
    "heighttexture", "interactionfoam", "interiordiffusemap", "iridescencetex", "iridescencetexture", "layer0tex", "layer1tex", "lightningtexture", "limbo",
    "liquidextinctionlut", "liquidtintmask", "liquidtintpalette", "lookuptexture", "lutdata", "lutmask", "lyr0diffusetex", "lyr0normaltex",
    "lyr1diffusetex", "lyr1normaltex", "lyr2diffusetex", "lyr2normaltex", "lyr3diffusetex", "lyr3normaltex", "masktex", "masktex2", "materialatex",
    "materialbtexture", "materialbtexture2", "materialctexture", "materialid", "materialpalettedata", "mipmapsource", "noisetexture", "puddlebump",
    "puddlemask", "rainmap", "refractionbumptexture", "retroreflectiontex", "retroreflectiontexture", "rope", "rope_prev", "roughnessmap",
    "scratchtexture", "shadow_globals", "skyparaboloid", "sourcetex", "sourcetexture", "sourcetexture1", "spectexture", "spectexture2",
    "speculartex", "speculartex2", "speculartexture", "speculartexture2", "standardwrinkletexture", "stenciltexture", "texture0", "texture1",
    "texture2", "timesteptexture", "tintbumptexture", "tintpalettetex", "tinttexture", "trailmaptexture", "translucencytex", "translucencytexture",
    "velocitybuffer", "visibilitybuffer", "volumelutdata", "waterfalllayer1diffuse", "waterfalllayer1normal", "waterheighttexture",
    "wrinkleocclusionmask",
    // per-layer sets: the same suffix matches every layer's hash
    "lyr0materialatex", "lyr1materialatex", "lyr2materialatex", "lyr3materialatex", "lyr0materialbtex", "lyr1materialbtex", "lyr2materialbtex",
    "lyr3materialbtex", "lyr2controltex", "lyr3controltex", "surfacedetailtexture0", "surfacedetailtexture1", "surfacedetailtexture2",
];

/// Where a hash is looked up: decides which name wins when two names share
/// a hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    Shader,
    Param,
    Field,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Dict {
    pub version: u32,
    /// joaat of the lower-case name (8 hex) -> [(name, source)]
    pub names: BTreeMap<String, Vec<(String, String)>>,
    /// Case-sensitive joaat (names with capitals only), used for meta field
    /// names, most of which hash their original spelling.
    #[serde(default)]
    pub exact: BTreeMap<String, Vec<(String, String)>>,
}

fn order(ctx: Ctx) -> [&'static str; 3] {
    match ctx {
        Ctx::Shader => ["common", "shaders", "derived"],
        Ctx::Param => ["derived", "shaders", "common"],
        Ctx::Field => ["common", "shaders", "derived"],
    }
}

struct Loaded {
    map: HashMap<u32, Vec<(String, String)>>,
    exact: HashMap<u32, Vec<(String, String)>>,
}

/// joaat without lower-casing.
pub fn joaat_exact(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.bytes() {
        h = h.wrapping_add(c as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

static WORK: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static DICT: std::sync::RwLock<Option<std::sync::Arc<Loaded>>> = std::sync::RwLock::new(None);

/// Tell the lookups where the work folder is (call once at start-up). Without
/// it, or before `sixng names` has run, only the built-in names resolve.
pub fn set_work_dir(work: &Path) {
    let _ = WORK.set(work.to_path_buf());
}

/// The work directory set by `set_work_dir` (via `export::Workspace::new`).
pub fn work_dir() -> Option<PathBuf> {
    WORK.get().cloned()
}

pub fn dict_path(work: &Path) -> PathBuf {
    work.join("names").join("names.json")
}

/// Drop the loaded dictionary so the next lookup reads `names.json` again
/// (after `build`).
pub fn reload() {
    *DICT.write().unwrap() = None;
}

fn loaded() -> std::sync::Arc<Loaded> {
    if let Some(l) = DICT.read().unwrap().as_ref() {
        return l.clone();
    }
    let l = std::sync::Arc::new({
        let mut map: HashMap<u32, Vec<(String, String)>> = HashMap::new();
        let mut exact: HashMap<u32, Vec<(String, String)>> = HashMap::new();
        if let Some(d) = WORK.get().and_then(|w| std::fs::read(dict_path(w)).ok()).and_then(|b| serde_json::from_slice::<Dict>(&b).ok()) {
            for (src, dst) in [(d.names, &mut map), (d.exact, &mut exact)] {
                for (h, v) in src {
                    if let Ok(h) = u32::from_str_radix(&h, 16) {
                        dst.insert(h, v);
                    }
                }
            }
        }
        for n in DERIVED_EXACT {
            let v = exact.entry(joaat_exact(n)).or_default();
            if !v.iter().any(|(x, _)| x == n) {
                v.push((n.to_string(), "derived".into()));
            }
        }
        for n in DERIVED {
            let v = map.entry(joaat(n)).or_default();
            if !v.iter().any(|(x, _)| x == n) {
                v.push((n.to_string(), "derived".into()));
            }
        }
        Loaded { map, exact }
    });
    *DICT.write().unwrap() = Some(l.clone());
    l
}

/// The best name for a hash in a context, if any.
pub fn lookup(h: u32, ctx: Ctx) -> Option<String> {
    let l = loaded();
    if ctx == Ctx::Field {
        if let Some(v) = l.exact.get(&h) {
            for src in order(ctx) {
                if let Some((n, _)) = v.iter().find(|(_, s)| s == src) {
                    return Some(n.clone());
                }
            }
        }
    }
    let v = l.map.get(&h)?;
    for src in order(ctx) {
        if let Some((n, _)) = v.iter().find(|(_, s)| s == src) {
            return Some(n.clone());
        }
    }
    v.first().map(|(n, _)| n.clone())
}

/// Lookup from an 8-digit hex string, as stored in exports.
pub fn lookup_hex(h: &str, ctx: Ctx) -> Option<String> {
    lookup(u32::from_str_radix(h.trim(), 16).ok()?, ctx)
}

pub fn shader(h: &str) -> Option<String> {
    lookup_hex(h, Ctx::Shader)
}

pub fn param(h: &str) -> Option<String> {
    lookup_hex(h, Ctx::Param)
}

/// Every candidate name for a hash, with its source.
pub fn all(h: u32) -> Vec<(String, String)> {
    let l = loaded();
    let mut v = l.map.get(&h).cloned().unwrap_or_default();
    for (n, s) in l.exact.get(&h).cloned().unwrap_or_default() {
        v.push((n, format!("{s} (case-sensitive)")));
    }
    v
}

fn is_ident(s: &[u8]) -> bool {
    s.len() >= 3 && s.len() <= 96 && (s[0].is_ascii_alphabetic() || s[0] == b'_') && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Identifiers of a text buffer (letters, digits, `_`; 3 to 96 characters).
fn idents(b: &[u8], out: &mut std::collections::HashSet<String>) {
    for w in b.split(|c| !(c.is_ascii_alphanumeric() || *c == b'_')) {
        if is_ident(w) && w.iter().any(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase()) {
            out.insert(String::from_utf8_lossy(w).into_owned());
        }
    }
}

/// NUL-terminated identifiers of a binary file, streamed in 4 MiB pieces.
fn file_cstrings(path: &Path, out: &mut std::collections::HashSet<String>) -> Result<()> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; (4 << 20) + 128];
    let mut carry = 0usize;
    loop {
        let n = f.read(&mut buf[carry..carry + (4 << 20)])?;
        let len = carry + n;
        let b = &buf[..len];
        // only words followed by NUL and preceded by NUL count
        let mut last_nul = None;
        for (i, &c) in b.iter().enumerate() {
            if c == 0 {
                if let Some(s) = last_nul {
                    let w = &b[s + 1..i];
                    if is_ident(w) && w.len() >= 4 && w.iter().filter(|c| c.is_ascii_lowercase()).count() >= 3 {
                        out.insert(String::from_utf8_lossy(w).into_owned());
                    }
                }
                last_nul = Some(i);
            }
        }
        if n == 0 {
            break;
        }
        let keep = len.min(97);
        buf.copy_within(len - keep..len, 0);
        carry = keep;
    }
    Ok(())
}

/// Build the dictionary from the install and save it. `index_dir` holds the
/// archive indexes (`common.rpf` must be indexed).
pub fn build(game: &Path, index_dir: &Path, work: &Path) -> Result<serde_json::Value> {
    let mut by_src: Vec<(&str, std::collections::HashSet<String>)> = Vec::new();
    // common.rpf: text and XML entries
    let mut common = std::collections::HashSet::new();
    let ix = crate::scan::load_index(&crate::scan::index_path(index_dir, "common.rpf")).context("common.rpf is not indexed (run `sixng index common`)")?;
    let arch = game.join("common.rpf");
    let mut read = 0usize;
    for e in &ix.entries {
        if matches!(e.kind, crate::classify::Kind::Xml | crate::classify::Kind::Text | crate::classify::Kind::Bin) {
            if let Ok(b) = crate::scan::read_entry(&arch, e) {
                idents(&b, &mut common);
                read += 1;
            }
        }
    }
    by_src.push(("common", common));
    let mut sh = std::collections::HashSet::new();
    let sp = game.join("shaders.rpf");
    if sp.exists() {
        file_cstrings(&sp, &mut sh)?;
    }
    by_src.push(("shaders", sh));
    let mut d = Dict { version: 1, names: BTreeMap::new(), exact: BTreeMap::new() };
    let mut counts = serde_json::Map::new();
    for (src, set) in &by_src {
        counts.insert(src.to_string(), set.len().into());
        let mut v: Vec<&String> = set.iter().collect();
        // the lower-case spelling first: shader and parameter names are
        // lower case, and it then represents the hash
        v.sort_by_key(|n| (n.bytes().any(|c| c.is_ascii_uppercase()), n.as_str()));
        for n in v {
            let e = d.names.entry(format!("{:08x}", joaat(n))).or_default();
            // one entry per lower-case spelling and source
            if !e.iter().any(|(x, s)| x.eq_ignore_ascii_case(n) && s == src) {
                e.push((n.clone(), src.to_string()));
            }
            if n.bytes().any(|c| c.is_ascii_uppercase()) {
                d.exact.entry(format!("{:08x}", joaat_exact(n))).or_default().push((n.clone(), src.to_string()));
            }
        }
    }
    let p = dict_path(work);
    std::fs::create_dir_all(p.parent().unwrap())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&d)?)?;
    std::fs::rename(&tmp, &p)?;
    reload();
    Ok(serde_json::json!({"out": p, "hashes": d.names.len(), "case_sensitive_hashes": d.exact.len(), "identifiers": counts, "common_entries_read": read, "derived": DERIVED.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_hashes() {
        // shader and parameter names: joaat of the lower-case name
        assert_eq!(joaat("standard"), 0xbcf0_3fc1);
        assert_eq!(joaat("diffusetex"), 0xc8e8_c282);
        assert_eq!(joaat("BumpTex"), 0x1e77_bf4c);
        // case-sensitive form differs once capitals are involved
        assert_ne!(joaat_exact("BumpTex"), joaat("BumpTex"));
        assert_eq!(joaat_exact("bumptex"), joaat("bumptex"));
        assert_eq!(joaat_exact("timeFlags"), 0x8609_d52c);
        assert_eq!(joaat_exact("lightType"), 0x1cbb_be30);
        assert_eq!(joaat_exact("handling"), 0xfb1f_80ff);
        assert_eq!(joaat_exact("slots"), 0x59a5_9c3e);
        assert_eq!(joaat_exact("options"), 0xa12f_f3a2);
        assert_eq!(joaat_exact("probability"), 0xdc73_2774);
        assert_eq!(joaat_exact("ParentName"), 0x49a0_1a86);
        assert_eq!(joaat_exact("HandlingData"), 0x41c8_dec7);
        assert_eq!(joaat_exact("StateName"), 0x3ead_dcd7);
        assert_eq!(joaat_exact("valueHash"), 0x817e_d7a4);
        assert_eq!(joaat_exact("contentType"), 0x2a8e_416f);
        assert_eq!(joaat_exact("childIndex"), 0x0129_6aed);
        assert_eq!(joaat_exact("States"), 0xebac_f18c);
        assert_eq!(lookup(0x49a0_1a86, Ctx::Field).as_deref(), Some("ParentName"));
        assert_eq!(lookup(0x817e_d7a4, Ctx::Field).as_deref(), Some("valueHash"));
        // derived names resolve without a dictionary file
        assert_eq!(lookup(0xc8e8_c282, Ctx::Param).as_deref(), Some("diffusetex"));
    }
}
