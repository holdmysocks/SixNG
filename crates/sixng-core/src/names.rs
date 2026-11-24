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
//! * `derived`: the built-in list;
//! * `audio`: from `audio.rpf` (when indexed): identifiers of its text
//!   entries, NUL-delimited names in its metadata files (wave slot names),
//!   and the dotted audio pack names (`Core.RadioStation.news`);
//! * `mounts`: virtual file-system paths (`platform:/levels/…/x.rpf`,
//!   `common:/data/…`, `$(target)/data/…`, `audio:/config/…`) in the
//!   text and XML of `common.rpf` and `audio.rpf`, mostly the content
//!   files' mount lists. Each path adds itself, itself without the
//!   extension and its file name with the extension;
//! * `banks`: audio bank names (`core.resident/gun_pistol`), found by
//!   joining the audio pack names and identifiers above and matching the
//!   bank container hashes that `sixng audio-meta` records
//!   ([`find_bank_names`]).
//!
//! Optionally, `sixng names --namehash <dictionary.json>` imports a
//! [gta6-namehash](https://github.com/tarekmania/gta6-namehash) dictionary
//! (joaat of names from public GTA V lists, no game files) into
//! `<work>/names/namehash.json`. Its names are candidates: a lookup uses one
//! only when no source above names the hash, and `--lookup` lists them with
//! source `namehash`.
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
    "valueHash", "contentType", "childIndex", "States",
    // hash matches against public GTA V name lists: the variation sets'
    // paint-colour list, a vehicle configuration's body colour, the animation
    // rotation property, the minigame dialogue field next to MissionId, and
    // the wardrobe field that holds body-type words (the name fits loosely)
    "colours", "colour", "bonerotation", "BankName", "pedtype",
    // vehicle material library fields (surface ranges, detail sets, coat, wear
    // and colour records): case-sensitive joaat search over camelCase word
    // combinations, 2026-11-23. One- and two-word names, plus three-word names
    // that a sibling field confirms (0 / 1, Min / Max, A / B / C, Start / End)
    "active", "ageWearHash", "albedo", "albedoMultiply", "albedoOptions", "albedoPage", "albedoPalette", "albedoScreen", "albedoSelection",
    "albedoTag", "alphaThresholdMax", "alphaThresholdMin", "anisoCutoff", "anisoDetailTile", "anisoIndex", "anisoTile", "anisotropyIntensity",
    "archetype", "asset", "attachBone", "attachBoneId", "attachInfo", "attachOffsets", "baseBleachEnd", "baseBleachMax", "baseBleachMin",
    "baseBleachStart", "baseCoatRoughness", "blueMarginMax", "blueMarginMin", "blueScratchEnd", "blueScratchStart", "boneId", "boneInfos", "boneName",
    "boneTag", "branch", "clearCoatIntensity", "clearCoatRoughness", "colorBlue", "colorGreen", "colorRed", "componentName", "controlIntensityA",
    "controlIntensityB", "controlIntensityC", "controlSpeed", "cooldownMult", "damageScale", "dataCheck", "dataHash", "defaultAlbedo",
    "defaultPaintType", "defaultSpecular", "detailFadeRange", "detailIndex0", "detailIndex1", "detailRotation", "detailRoughnessStrength0",
    "detailRoughnessStrength1", "detailStrength0", "detailStrength1", "detailTile0", "detailTile1", "detailTiling0", "detailTiling1",
    "detailTintStrength0", "detailTintStrength1", "diffuseTexture", "diffuseTiling", "dirtLumMax", "dirtLumScale", "disableCollision",
    "displayDuration", "displayName", "dustIntensity", "dynamicBoneId", "edgeDensity", "edgeEnd", "edgeIntensity", "edgeStart", "edgeZ",
    "emissiveBlue", "emissiveCutoff", "emissiveGreen", "emissiveRed", "enableControl", "enableDiffuse", "endColumn", "entityMap", "fabricUser2",
    "fadeOrientation", "fadePack", "fadeRoll", "fadeScale", "filmThickness", "flakeCoverage", "flakeOrientation", "flakesCutoff", "flakeSize",
    "forceScale", "fxName", "fxType", "glassAlpha", "greenMarginMax", "greenMarginMin", "group", "id", "inclineComponentsScale", "index",
    "intensityMax", "intensityMin", "iorCutoff", "ladderFlags", "leftOffset", "lightGroups", "lineOffset", "liveryControls", "liveryEmissive",
    "liveryMaterials", "lockColor", "lockIntensity", "lodDistance", "lodLevels", "materialName", "materialPresetPalette", "materialPresets",
    "materialType", "maxBlue", "maxGreen", "maxIntensity", "maxRed", "maxTile", "maxTiling", "maxWear", "metalHigh", "metalLow", "minBlue",
    "minGreen", "minRandomScale", "minRed", "minTile", "minTiling", "minWear", "modSets", "offsetPosition", "offsetRandomMax", "offsetRandomMin",
    "offsetRotation", "paintHash", "paintPalette", "paintType", "partId", "positionOffset", "prefix", "primaryMaterial", "primerEnd",
    "primerGradientEnd", "primerGradientStart", "primerIntensityA", "primerIntensityB", "primerIntensityC", "primerPalette", "primerSets",
    "primerStart", "primerType", "rainHash", "randomRotation", "redMarginMax", "redMarginMin", "redScratchEnd", "redScratchStart",
    "reflectorData", "rimBoneId", "rimRatio", "rimSize", "rotationAxis", "rustEnd", "rustEndB", "rustEndC", "rustStart", "rustStartB", "rustStartC",
    "scannerHash", "secMotion", "seed", "sheenCutoff", "sheenFade", "sheenIntensity", "sheenReflectance", "slot", "spawnDistance",
    "specOptions", "specTag", "specularColor", "specularCutoff", "specularPalette", "startColumn", "sticker", "stickerPack", "stiffnessMultiplier",
    "string", "sunBleachEnd", "sunBleachScreen", "sunBleachStart", "surfaceIndex", "texData", "texDistances", "tileAmount",
    "timeMultiplier", "tintInfos", "tintPaletteRow", "tintRow", "topOffset", "ttName", "typeData", "tyreSize", "useAnisotropy", "useColor",
    "useEmissive", "useFlakes", "useIntensity", "useSpecularColor", "vehicleSpec", "vehicleType", "wearPresets", "wheelDetailScale", "wheelHash",
    "wheelId", "wheelInfo", "windowTint",
    // ped wardrobe meta fields (kit f795e8ba, kit state 37e52277, asset
    // 147fdab5, kit set a9d57751): case-sensitive joaat search over one- and
    // two-word PascalCase names against 26 field hashes, 2026-11-23
    "KitName", "TemplateName", "Palette", "Tint0", "Tint1", "Tint2", "GarmentPreset", "PropertyTags", "Asset", "KitLayer", "ToStateNames",
    "Drawable", "Albedo", "Material", "AssetName", "IncludedKits",
    // vehicle surface (9e4de959) reference fields: the two layer materials
    // (range records), layer 1's colour record and the detail set.
    // One- to three-word camelCase search ending in Hash, 2026-11-23
    "material0Hash", "material1Hash", "albedo1Hash", "detailMapsHash",
    // layered-material library (root 8419ab01, `matlib`) and the wardrobe's second preset field.
    // One- to three-word PascalCase search, 2026-11-23
    "Tiles", "TileName", "TileNameOverrides", "TilePresets", "TilePresetNames", "TransmissionProfiles", "TransmissionProfileName", "TransmissionProfileOverrides",
    "FloatParameters", "ColorParameters", "UIntParameters", "TextureNameParameters", "garmentPreset"];

pub const DERIVED: &[&str] = &[
    // parameter names of the layered-material library (`matlib`): lower-case joaat, domain-vocabulary search, 2026-11-23
    "absorptionalbedoblend", "albedovariationblendamount", "alphathreshold", "anisointensity", "anisooffset", "curvatureblendamount", "curvaturemultiplier",
    "curvatureroughnessblendamount", "detailnormalblendamount", "diffractionprofile", "dirtcontrol", "dirtmapblendamount", "dirttint", "emissivemultiplier",
    "engravingblendamount", "engravingbumpiness", "engravingcolor", "engravingheight", "engravingmetallic", "engravingroughness", "extinctionlut",
    "heightmapblendamount", "porositybias", "roughnessmapblendamount", "scatteringalbedoblend", "sheenblend", "sheencolor", "specularfresnel",
    "specularocclusionblendamount", "stitchesblendamount", "stitchtint", "tangentangleoffset", "thicknessanisotropy", "tileflags", "tilematerialmap", "tilemetallic",
    "tilenormalmap", "tileoffset", "tilepresetflags", "tilerotation", "tileroughnessmultiplier", "tilescale", "tilesheen", "tiletint", "tintablepatternflags",
    "tintablepatternmap", "tintablepatternopacity", "tintablepatternrotation", "tintablepatternscale", "tintablepatterntint", "tintmaskemissiveintensity",
    "tintmaskflags", "tintmaskheightopacity", "tintmaskmap", "tintmaskmetallic", "tintmaskmetallicopacity", "tintmaskopacity", "tintmaskrotation", "tintmaskroughness",
    "tintmaskroughnessopacity", "tintmaskscale", "tintmasktranslation", "tintmasktint", "translucencymapblendamount", "translucencymultiplier", "translucencyscale", "transmissionprofileflags",
    "transmissionroughness", "windscale",
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
    "lyr3materialbtex", "lyr2controltex", "lyr3controltex", "surfacedetailtexture0", "surfacedetailtexture1", "surfacedetailtexture2",    // ped wardrobe property tags: keys whose values fit the name (kittype =
    // groom, hairlength = l01..l09, slodstyle = generic / generic_lob), the
    // audio group, and hair style values. Lower-case joaat search, 2026-11-23
    "kittype", "hairlength", "slodstyle", "ethnicity", "dirtdefinition", "bodyshapeprimary", "goretype", "decorationpresets",
    "weatheringcategories", "outfitpedtype", "noasset", "audio_lower", "audio_accessories", "audio_hands", "audio_head", "audio_face",
    "audio_shoe", "audiofabric", "has_hair", "groom", "bob", "lob", "bun", "topknot", "cornrows", "generic_lob", "l01", "l05", "l06", "l07",
    "l08", "l09",
];

/// Where a hash is looked up: decides which name wins when two names share
/// a hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    Shader,
    Param,
    Field,
    /// Audio bank (container) and object names.
    Audio,
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

fn order(ctx: Ctx) -> [&'static str; 6] {
    match ctx {
        Ctx::Shader => ["common", "shaders", "derived", "audio", "mounts", "banks"],
        Ctx::Param => ["derived", "shaders", "common", "audio", "mounts", "banks"],
        Ctx::Field => ["common", "shaders", "derived", "audio", "mounts", "banks"],
        Ctx::Audio => ["banks", "audio", "mounts", "common", "shaders", "derived"],
    }
}

struct Loaded {
    map: HashMap<u32, Vec<(String, String)>>,
    exact: HashMap<u32, Vec<(String, String)>>,
    /// Imported gta6-namehash names, used only when `map` / `exact` have none.
    public_map: HashMap<u32, Vec<(String, String)>>,
    public_exact: HashMap<u32, Vec<(String, String)>>,
}

fn by_hash(src: BTreeMap<String, Vec<(String, String)>>) -> HashMap<u32, Vec<(String, String)>> {
    src.into_iter().filter_map(|(h, v)| u32::from_str_radix(&h, 16).ok().map(|h| (h, v))).collect()
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

pub fn namehash_path(work: &Path) -> PathBuf {
    work.join("names").join("namehash.json")
}

/// A gta6-namehash `dictionary.json` (hash -> names) as a [`Dict`] with
/// source `namehash`. Each name is re-hashed: it goes to `names` when the
/// joaat of its lower-case form is the key and to `exact` when the joaat of
/// its spelling is; names that match neither are dropped.
pub fn from_namehash(raw: &BTreeMap<String, Vec<String>>) -> Dict {
    let mut d = Dict { version: 1, names: BTreeMap::new(), exact: BTreeMap::new() };
    for (k, names) in raw {
        let Ok(h) = u32::from_str_radix(k, 16) else { continue };
        let key = format!("{h:08x}");
        // the lower-case spelling first, as in `build`
        let mut names: Vec<&String> = names.iter().collect();
        names.sort_by_key(|n| (n.bytes().any(|c| c.is_ascii_uppercase()), n.as_str()));
        for n in names {
            if joaat(n) == h {
                let e = d.names.entry(key.clone()).or_default();
                if !e.iter().any(|(x, _)| x.eq_ignore_ascii_case(n)) {
                    e.push((n.clone(), "namehash".into()));
                }
            }
            if n.bytes().any(|c| c.is_ascii_uppercase()) && joaat_exact(n) == h {
                d.exact.entry(key.clone()).or_default().push((n.clone(), "namehash".into()));
            }
        }
    }
    d
}

/// Import a gta6-namehash `dictionary.json` into `<work>/names/namehash.json`.
pub fn import_namehash(src: &Path, work: &Path) -> Result<serde_json::Value> {
    let b = std::fs::read(src).with_context(|| format!("reading {}", src.display()))?;
    let raw: BTreeMap<String, Vec<String>> = serde_json::from_slice(&b).context("not a gta6-namehash dictionary.json (hash -> list of names)")?;
    let d = from_namehash(&raw);
    let p = namehash_path(work);
    std::fs::create_dir_all(p.parent().unwrap())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&d)?)?;
    std::fs::rename(&tmp, &p)?;
    reload();
    Ok(serde_json::json!({"out": p, "hashes_in_file": raw.len(), "hashes": d.names.len(), "case_sensitive_hashes": d.exact.len()}))
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
        let p = WORK.get().and_then(|w| std::fs::read(namehash_path(w)).ok()).and_then(|b| serde_json::from_slice::<Dict>(&b).ok()).unwrap_or_default();
        Loaded { map, exact, public_map: by_hash(p.names), public_exact: by_hash(p.exact) }
    });
    *DICT.write().unwrap() = Some(l.clone());
    l
}

/// The best name for a hash in a context, if any. Imported gta6-namehash
/// names come last.
pub fn lookup(h: u32, ctx: Ctx) -> Option<String> {
    let l = loaded();
    own(&l, h, ctx).or_else(|| {
        let exact = if ctx == Ctx::Field { l.public_exact.get(&h) } else { None };
        exact.or_else(|| l.public_map.get(&h)).and_then(|v| v.first()).map(|(n, _)| n.clone())
    })
}

fn own(l: &Loaded, h: u32, ctx: Ctx) -> Option<String> {
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
    v.extend(l.public_map.get(&h).cloned().unwrap_or_default());
    for (n, s) in l.public_exact.get(&h).cloned().unwrap_or_default() {
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

fn path_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'/' | b'-' | b'%' | b'@')
}

/// Virtual file-system paths of a buffer: `<mount>:/<path>` (`platform:/`,
/// `common:/`, `audio:/` …, mount 2 to 24 letters, digits or `_`) and
/// `$(<mount>)/<path>`. Trailing `.` and `/` are dropped; a path needs at
/// least one character after the mount.
pub fn mount_paths(b: &[u8], out: &mut std::collections::BTreeSet<String>) {
    let mut push = |start: usize, body: usize| {
        let mut end = body;
        while end < b.len() && path_char(b[end]) {
            end += 1;
        }
        while end > body && matches!(b[end - 1], b'.' | b'/') {
            end -= 1;
        }
        if end > body {
            out.insert(String::from_utf8_lossy(&b[start..end]).into_owned());
        }
    };
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b':' && b[i + 1] == b'/' {
            let mut s = i;
            while s > 0 && word(b[s - 1]) {
                s -= 1;
            }
            if (2..=24).contains(&(i - s)) && b[s].is_ascii_alphabetic() && !b[i + 2..].starts_with(b"/") {
                push(s, i + 2);
            }
        } else if b[i] == b'$' && b[i + 1] == b'(' {
            let mut e = i + 2;
            while e < b.len() && word(b[e]) {
                e += 1;
            }
            if (2..=24).contains(&(e - i - 2)) && b[e..].starts_with(b")/") {
                push(i, e + 2);
            }
        }
        i += 1;
    }
}

/// The names a mount path adds: the path as written, without its extension,
/// and its file name with the extension (the file name without extension is
/// an identifier, which `idents` already finds).
pub fn path_names(p: &str) -> Vec<String> {
    let mut v = vec![p.to_string()];
    let file = p.rsplit('/').next().unwrap_or(p);
    if let Some(dot) = file.rfind('.').filter(|&d| d > 0) {
        v.push(p[..p.len() - file.len() + dot].to_string());
        v.push(file.to_string());
    }
    v
}

/// Dotted names on lines of their own (`Core.RadioStation.news`, the audio
/// pack list): 2 to 8 identifier parts of 2 or more characters joined by
/// `.`, nothing else on the line (or between NULs).
pub fn dotted_lines(b: &[u8], out: &mut std::collections::BTreeSet<String>) {
    for l in b.split(|c| *c == b'\n' || *c == b'\r' || *c == 0) {
        let l = l.trim_ascii();
        let parts: Vec<&[u8]> = l.split(|c| *c == b'.').collect();
        if (2..=8).contains(&parts.len()) && parts.iter().all(|p| p.len() >= 2 && p.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_') && p[0].is_ascii_alphabetic()) {
            out.insert(String::from_utf8_lossy(l).into_owned());
        }
    }
}

/// NUL-terminated identifiers of a binary buffer: the run of identifier
/// characters that ends at each NUL, whatever comes before it (the audio
/// metadata files keep names in NUL-padded fields after hashes and counts).
/// At least 4 characters, 3 of them letters, starting with a letter; names
/// shorter than 6 also need one case throughout (or a capital and then lower
/// case) and no digits, which keeps most chance letter runs of binary data
/// out (`pN8G`, `emJa`).
fn cstrings(b: &[u8], out: &mut std::collections::HashSet<String>) {
    let tidy = |w: &[u8]| w.len() >= 6 || (!w.iter().any(|c| c.is_ascii_digit()) && (w.iter().all(|c| !c.is_ascii_uppercase()) || w.iter().all(|c| !c.is_ascii_lowercase()) || w[1..].iter().all(|c| !c.is_ascii_uppercase())));
    for seg in b.split(|c| *c == 0) {
        let start = seg.iter().rposition(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).map_or(0, |i| i + 1);
        let w = &seg[start..];
        if is_ident(w) && w.len() >= 4 && w[0].is_ascii_alphabetic() && w.iter().filter(|c| c.is_ascii_alphabetic()).count() >= 3 && tidy(w) {
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
    // virtual file-system paths (the content files' mount lists), from
    // common.rpf and audio.rpf
    let mut paths = std::collections::BTreeSet::new();
    let ix = crate::scan::load_index(&crate::scan::index_path(index_dir, "common.rpf")).context("common.rpf is not indexed (run `sixng index common`)")?;
    let arch = game.join("common.rpf");
    let mut read = 0usize;
    for e in &ix.entries {
        if matches!(e.kind, crate::classify::Kind::Xml | crate::classify::Kind::Text | crate::classify::Kind::Bin) {
            if let Ok(b) = crate::scan::read_entry(&arch, e) {
                idents(&b, &mut common);
                mount_paths(&b, &mut paths);
                read += 1;
            }
        }
    }
    by_src.push(("common", common));
    // audio.rpf: identifiers of the text entries (config field names, pack
    // lists), NUL-delimited names in the metadata files (wave slots), the
    // dotted pack names and the `audio:/` paths
    let mut audio = std::collections::HashSet::new();
    let mut dotted = std::collections::BTreeSet::new();
    let mut audio_read = 0usize;
    if let Ok(ix) = crate::scan::load_index(&crate::scan::index_path(index_dir, "audio.rpf")) {
        let arch = game.join("audio.rpf");
        for e in &ix.entries {
            if matches!(e.kind, crate::classify::Kind::Xml | crate::classify::Kind::Text | crate::classify::Kind::Bin) {
                if let Ok(b) = crate::scan::read_entry(&arch, e) {
                    if e.kind == crate::classify::Kind::Bin {
                        cstrings(&b, &mut audio);
                    } else {
                        idents(&b, &mut audio);
                    }
                    dotted_lines(&b, &mut dotted);
                    mount_paths(&b, &mut paths);
                    audio_read += 1;
                }
            }
        }
    }
    audio.extend(dotted.iter().cloned());
    by_src.push(("audio", audio));
    let mounts: std::collections::HashSet<String> = paths.iter().flat_map(|p| path_names(p)).collect();
    by_src.push(("mounts", mounts));
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
    // audio bank names, when `audio-meta` has recorded the banks' containers
    let banks = crate::audiometa::load_containers(work, "audio.rpf").map(|c| set_bank_names(&mut d, &crate::audiometa::bank_container(&c)));
    let p = dict_path(work);
    save_dict(&p, &d)?;
    Ok(serde_json::json!({"out": p, "hashes": d.names.len(), "case_sensitive_hashes": d.exact.len(), "identifiers": counts, "common_entries_read": read, "audio_entries_read": audio_read, "mount_paths": paths.len(), "bank_names": banks, "derived": DERIVED.len()}))
}

fn save_dict(p: &Path, d: &Dict) -> Result<()> {
    std::fs::create_dir_all(p.parent().unwrap())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(d)?)?;
    std::fs::rename(&tmp, p)?;
    reload();
    Ok(())
}

fn joaat_step(mut h: u32, s: &[u8]) -> u32 {
    for &c in s {
        h = h.wrapping_add(c.to_ascii_lowercase() as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h
}

fn joaat_fin(mut h: u32) -> u32 {
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// Audio bank names. A bank's container hash is joaat of
/// `<pack>/<bank>` in lower case (`core.resident/gun_pistol`), where the
/// pack is one of the dotted audio pack names and the bank name is an
/// identifier. Every pack in `d` (source `audio`, dotted) is tried with
/// every identifier in `d`. `banks` is bank offset -> container hash.
///
/// Banks of one pack lie next to each other in the archive, so a match is
/// kept only when the nearest matched bank before or after it (by offset)
/// has the same pack; this drops the chance matches (about 1 in 36 on the
/// game's audio.rpf). Returns (bank offset, container, name), by offset.
pub fn find_bank_names(d: &Dict, banks: &BTreeMap<u64, u32>) -> Vec<(u64, u32, String)> {
    let want: std::collections::HashSet<u32> = banks.values().copied().collect();
    let mut packs: Vec<String> = Vec::new();
    let mut words: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for v in d.names.values() {
        for (n, src) in v {
            let l = n.to_ascii_lowercase();
            if src == "audio" && l.contains('.') && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.') {
                packs.push(l);
            } else if is_ident(l.as_bytes()) {
                words.insert(l);
            }
        }
    }
    packs.sort();
    packs.dedup();
    // container -> candidate names
    let mut cand: HashMap<u32, Vec<String>> = HashMap::new();
    for p in &packs {
        let h0 = joaat_step(joaat_step(0, p.as_bytes()), b"/");
        for w in &words {
            let h = joaat_fin(joaat_step(h0, w.as_bytes()));
            if want.contains(&h) {
                cand.entry(h).or_default().push(format!("{p}/{w}"));
            }
        }
    }
    let pack = |n: &str| n.split('/').next().unwrap_or("").to_string();
    let matched: Vec<(u64, u32, &Vec<String>)> = banks.iter().filter_map(|(o, h)| cand.get(h).map(|v| (*o, *h, v))).collect();
    let mut out = Vec::new();
    for (i, (off, h, names)) in matched.iter().enumerate() {
        let near: Vec<std::collections::HashSet<String>> = [i.checked_sub(1), Some(i + 1)].into_iter().flatten().filter_map(|j| matched.get(j)).map(|m| m.2.iter().map(|n| pack(n)).collect()).collect();
        if let Some(n) = names.iter().find(|n| near.iter().any(|s| s.contains(&pack(n)))) {
            out.push((*off, *h, n.clone()));
        }
    }
    out
}

/// Replace the `banks` names of `d` with the bank names of `banks` (bank
/// offset -> container hash). Returns (banks with a container, banks named).
pub fn set_bank_names(d: &mut Dict, banks: &BTreeMap<u64, u32>) -> serde_json::Value {
    for v in d.names.values_mut() {
        v.retain(|(_, s)| s != "banks");
    }
    d.names.retain(|_, v| !v.is_empty());
    let found = find_bank_names(d, banks);
    for (_, h, n) in &found {
        let e = d.names.entry(format!("{h:08x}")).or_default();
        if !e.iter().any(|(x, s)| x == n && s == "banks") {
            e.push((n.clone(), "banks".into()));
        }
    }
    serde_json::json!({"banks": banks.len(), "named": found.len()})
}

/// Name the banks of `banks` (bank offset -> container hash) in
/// `<work>/names/names.json` (after `sixng names`; source `banks`).
pub fn add_bank_names(work: &Path, banks: &BTreeMap<u64, u32>) -> Result<serde_json::Value> {
    let p = dict_path(work);
    let mut d: Dict = serde_json::from_slice(&std::fs::read(&p).with_context(|| format!("no name dictionary {} (run `sixng names`)", p.display()))?)?;
    let r = set_bank_names(&mut d, banks);
    save_dict(&p, &d)?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namehash_import() {
        let raw: BTreeMap<String, Vec<String>> = [
            ("c897af02", vec!["Features"]),
            ("61e1e572", vec!["Features", "features"]),
            ("079baba7", vec!["attachBone"]),
            ("12345678", vec!["nomatch"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
        .collect();
        let d = from_namehash(&raw);
        // case-sensitive match only
        assert_eq!(d.exact["c897af02"], vec![("Features".to_string(), "namehash".to_string())]);
        assert!(!d.names.contains_key("c897af02"));
        // lower-case match, one entry per lower-case spelling
        assert_eq!(d.names["61e1e572"], vec![("features".to_string(), "namehash".to_string())]);
        assert!(!d.exact.contains_key("61e1e572"));
        // camelCase field name
        assert_eq!(joaat_exact("attachBone"), 0x079b_aba7);
        assert_eq!(d.exact["079baba7"][0].0, "attachBone");
        // names whose hash is not the key are dropped
        assert!(!d.names.contains_key("12345678") && !d.exact.contains_key("12345678"));
    }
    #[test]
    fn bank_name_search() {
        let mut d = Dict::default();
        let mut add = |n: &str, src: &str| d.names.entry(format!("{:08x}", joaat(n))).or_default().push((n.to_string(), src.to_string()));
        add("Pack.Alpha", "audio");
        add("pack.beta", "audio");
        for w in ["guns", "doors", "rain", "wind", "zzz"] {
            add(w, "common");
        }
        // partial hashing matches joaat of the joined name
        assert_eq!(joaat_fin(joaat_step(joaat_step(0, b"Pack.Alpha"), b"/guns")), joaat("pack.alpha/guns"));
        let h = |n: &str| joaat(n);
        // banks in archive order: two of pack.alpha, two of pack.beta, then
        // a lone pack.alpha bank among pack.beta ones (dropped), an unknown
        // one (skipped over), and another pack.beta bank
        let banks: BTreeMap<u64, u32> = [
            (0x1000, h("pack.alpha/guns")),
            (0x2000, h("pack.alpha/doors")),
            (0x3000, h("pack.beta/rain")),
            (0x4000, h("pack.beta/wind")),
            (0x5000, h("pack.alpha/zzz")),
            (0x6000, h("pack.beta/doors")),
            (0x7000, 0x1234_5678),
            (0x8000, h("pack.beta/guns")),
        ]
        .into_iter()
        .collect();
        let found: Vec<(u64, String)> = find_bank_names(&d, &banks).into_iter().map(|(o, _, n)| (o, n)).collect();
        assert_eq!(found, [(0x1000, "pack.alpha/guns".into()), (0x2000, "pack.alpha/doors".into()), (0x3000, "pack.beta/rain".into()), (0x4000, "pack.beta/wind".into()), (0x6000, "pack.beta/doors".into()), (0x8000, "pack.beta/guns".into())]);
        let r = set_bank_names(&mut d, &banks);
        assert_eq!(r["named"], 6);
        assert_eq!(d.names[&format!("{:08x}", h("pack.beta/rain"))], [("pack.beta/rain".to_string(), "banks".to_string())]);
        // a second run replaces, not adds
        set_bank_names(&mut d, &banks);
        assert_eq!(d.names.values().flatten().filter(|(_, s)| s == "banks").count(), 6);
    }
    #[test]
    fn mount_path_harvest() {
        let xml = b"<Item><filename>platform:/levels/world/props/px.ityp</filename></Item>\n<id>common:/data/levels/world/area.meta</id>\n<filename>$(target)/data/lang/lang_a.rpf</filename> audio:/sfx/ http://example.com/x a:/b \0audio:/config/sounds.dat\0";
        let mut p = std::collections::BTreeSet::new();
        mount_paths(xml, &mut p);
        let v: Vec<&str> = p.iter().map(|s| s.as_str()).collect();
        // `audio:/sfx/` loses its trailing slash; `http://` (double slash)
        // and the one-letter mount are not paths
        assert_eq!(v, ["$(target)/data/lang/lang_a.rpf", "audio:/config/sounds.dat", "audio:/sfx", "common:/data/levels/world/area.meta", "platform:/levels/world/props/px.ityp"]);
        assert_eq!(path_names("platform:/levels/world/props/px.ityp"), ["platform:/levels/world/props/px.ityp", "platform:/levels/world/props/px", "px.ityp"]);
        assert_eq!(path_names("audio:/sfx"), ["audio:/sfx"]);
        let mut d = std::collections::BTreeSet::new();
        dotted_lines(b"Pack.Group.item\r\nPack.Other\nd.Q\nBase:file_a:Kind\n\0Group.FieldName\0", &mut d);
        assert_eq!(d.iter().map(|s| s.as_str()).collect::<Vec<_>>(), ["Group.FieldName", "Pack.Group.item", "Pack.Other"]);
        let mut c = std::collections::HashSet::new();
        // a name after a hash in a NUL-padded field; runs that end before a
        // non-identifier byte, are short, start with a digit, or are short
        // with digits or mixed case are not names
        cstrings(b"\0SLOT_NAME_01\0tixN#<\0e82\0\xa0\x21\xd6{SOME_NAME\0\0\0\x0c808_NAME\0\x01pN8G\0\x01emJa\0\x01Lion\0OTHER_SLOT_2\0", &mut c);
        let mut c: Vec<_> = c.into_iter().collect();
        c.sort();
        assert_eq!(c, ["Lion", "OTHER_SLOT_2", "SLOT_NAME_01", "SOME_NAME"]);
    }
    #[test]
    fn known_hashes() {
        // shader and parameter names: joaat of the lower-case name
        assert_eq!(joaat("standard"), 0xbcf0_3fc1);
        // layered-material parameters and library fields
        assert_eq!((joaat("tiletint"), joaat("tilescale"), joaat("tintmaskmap"), joaat("stitchtint")), (0xf574_261b, 0x01ae_45d3, 0xd2b5_98e5, 0x1c85_bdf6));
        assert_eq!((joaat_exact("TilePresetNames"), joaat_exact("TileNameOverrides"), joaat_exact("ColorParameters")), (0x4a1e_bc36, 0x4bee_7340, 0xfc24_d9c5));
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
        assert_eq!(joaat_exact("colours"), 0x8c86_6829);
        assert_eq!(joaat_exact("bonerotation"), 0x98e1_750c);
        assert_eq!(joaat_exact("BankName"), 0x8d6b_17bd);
        assert_eq!(joaat_exact("pedtype"), 0x3a57_a0fc);
        assert_eq!(joaat_exact("colour"), 0x5bdc_13e3);
        assert_eq!(lookup(0x8c86_6829, Ctx::Field).as_deref(), Some("colours"));
        assert_eq!(lookup(0x5bdc_13e3, Ctx::Field).as_deref(), Some("colour"));
        assert_eq!(lookup(0x49a0_1a86, Ctx::Field).as_deref(), Some("ParentName"));
        assert_eq!(lookup(0x817e_d7a4, Ctx::Field).as_deref(), Some("valueHash"));
        // vehicle material library fields
        assert_eq!(joaat_exact("metalLow"), 0xf763_652d);
        assert_eq!(lookup(0xf763_652d, Ctx::Field).as_deref(), Some("metalLow"));
        assert_eq!(joaat_exact("metalHigh"), 0x09dd_33c1);
        assert_eq!(lookup(0x09dd_33c1, Ctx::Field).as_deref(), Some("metalHigh"));
        assert_eq!(joaat_exact("albedoMultiply"), 0x4b19_d0cb);
        assert_eq!(lookup(0x4b19_d0cb, Ctx::Field).as_deref(), Some("albedoMultiply"));
        assert_eq!(joaat_exact("detailTintStrength0"), 0xfcb9_94fa);
        assert_eq!(lookup(0xfcb9_94fa, Ctx::Field).as_deref(), Some("detailTintStrength0"));
        assert_eq!(joaat_exact("albedoSelection"), 0x9bdd_4c51);
        assert_eq!(lookup(0x9bdd_4c51, Ctx::Field).as_deref(), Some("albedoSelection"));
        assert_eq!(joaat_exact("albedoPage"), 0x0971_4f5c);
        assert_eq!(lookup(0x0971_4f5c, Ctx::Field).as_deref(), Some("albedoPage"));
        // ped wardrobe fields and property tag keys
        assert_eq!(joaat_exact("KitName"), 0x6512_1749);
        assert_eq!(lookup(0x6512_1749, Ctx::Field).as_deref(), Some("KitName"));
        assert_eq!(lookup(0x5ecf_9331, Ctx::Field).as_deref(), Some("Palette"));
        assert_eq!(lookup(0xc4d7_5d79, Ctx::Field).as_deref(), Some("Tint0"));
        assert_eq!(lookup(0xab5d_2a85, Ctx::Field).as_deref(), Some("Tint2"));
        assert_eq!(lookup(0x12f8_5293, Ctx::Field).as_deref(), Some("PropertyTags"));
        assert_eq!(lookup(0x41ac_f8eb, Ctx::Field).as_deref(), Some("Drawable"));
        assert_eq!(lookup(0xfb5b_b20d, Ctx::Field).as_deref(), Some("IncludedKits"));
        assert_eq!(lookup(0x2d49_7cdb, Ctx::Field).as_deref(), Some("kittype"));
        assert_eq!(lookup(0xd35f_809f, Ctx::Field).as_deref(), Some("hairlength"));
        assert_eq!(lookup(0x6004_f281, Ctx::Field).as_deref(), Some("l08"));
        assert_eq!(lookup(0xdf32_4ad0, Ctx::Field).as_deref(), Some("cornrows"));
        // vehicle surface reference fields
        assert_eq!(joaat_exact("material0Hash"), 0x2cc7_2f7c);
        assert_eq!(lookup(0x2cc7_2f7c, Ctx::Field).as_deref(), Some("material0Hash"));
        assert_eq!(lookup(0x1747_70cc, Ctx::Field).as_deref(), Some("material1Hash"));
        assert_eq!(lookup(0x5cff_e5e0, Ctx::Field).as_deref(), Some("albedo1Hash"));
        assert_eq!(lookup(0x6c3d_dabc, Ctx::Field).as_deref(), Some("detailMapsHash"));
        // derived names resolve without a dictionary file
        assert_eq!(lookup(0xc8e8_c282, Ctx::Param).as_deref(), Some("diffusetex"));
    }
}
