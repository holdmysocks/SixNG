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
//!   joining the audio pack names with the identifiers above and with the
//!   snake-case words of data.rpf's text tables (song titles, animal names;
//!   `<work>/names/text_words.json`, used for this search only), and
//!   matching the bank container hashes that `sixng audio-meta` records
//!   ([`find_bank_names`]). Stream names found on the way
//!   ([`find_stream_names`]) go to `<work>/index/audio_stream_names_audio.json`.
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
    // shader constants checked against the shader archive's constant buffers (2026-11-25): hardalphablend in
    // 327 effects (standard_alpha, ped shaders), blendintensity / blendoffset / puddleheightbias in the prop
    // batch shaders, grassopacity in the 2- and 4-layer ground shaders
    "blendintensity", "blendoffset", "hardalphablend", "grassopacity", "puddleheightbias",
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
    // snake-case words of data.rpf's text tables (song titles, animal and
    // place names), for the bank and stream name searches
    let text = harvest_text_words(game, index_dir).map(|(tables, words)| -> Result<serde_json::Value> {
        let p = text_words_path(work);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, serde_json::to_vec(&words)?)?;
        Ok(serde_json::json!({"tables": tables, "words": words.len()}))
    }).transpose()?;
    // audio bank names, when `audio-meta` has recorded the banks' containers
    let banks = match (crate::audiometa::load_containers(work, "audio.rpf"), crate::audio::load_index(work, "audio.rpf")) {
        (Some(c), Ok(ix)) => {
            let streams: BTreeMap<u64, Vec<u32>> = ix.banks.iter().map(|b| (b.off, b.streams.iter().map(|s| s.id).collect())).collect();
            Some(name_audio(&mut d, work, "audio.rpf", &crate::audiometa::bank_container(&c), &streams)?)
        }
        _ => None,
    };
    let p = dict_path(work);
    save_dict(&p, &d)?;
    Ok(serde_json::json!({"out": p, "hashes": d.names.len(), "case_sensitive_hashes": d.exact.len(), "identifiers": counts, "common_entries_read": read, "audio_entries_read": audio_read, "mount_paths": paths.len(), "text_words": text, "bank_names": banks, "derived": DERIVED.len()}))
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

/// The lower-case identifiers of `d` plus `extra` (bank and stream name
/// parts) and the dotted audio pack names (source `audio`), both sorted.
fn audio_words(d: &Dict, extra: &[String]) -> (Vec<String>, Vec<String>) {
    let mut packs = std::collections::BTreeSet::new();
    let mut words = std::collections::BTreeSet::new();
    for v in d.names.values() {
        for (n, src) in v {
            let l = n.to_ascii_lowercase();
            if src == "audio" && l.contains('.') && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.') {
                packs.insert(l);
            } else if src != "banks" && is_ident(l.as_bytes()) {
                words.insert(l);
            }
        }
    }
    words.extend(extra.iter().cloned());
    (packs.into_iter().collect(), words.into_iter().collect())
}

/// A Latin letter with its accent dropped (`é` -> `e`, `ñ` -> `n`, `ß` ->
/// `s`), for the Latin-1 and Latin Extended-A ranges; other characters as
/// they are.
fn fold(c: char) -> char {
    const L1: &str = "AAAAAAACEEEEIIIIDNOOOOOxOUUUUYTsaaaaaaaceeeeiiiidnooooo/ouuuuyty";
    const EA: &str = "AaAaAaCcCcCcCcDdDdEeEeEeEeEeGgGgGgGgHhHhIiIiIiIiIiJjJjKkkLlLlLlLlLlNnNnNnnNnOoOoOoOoRrRrRrSsSsSsSsTtTtTtUuUuUuUuUuUuWwYyYZzZzZzs";
    let u = c as u32;
    match u {
        0xc0..=0xff => L1.as_bytes()[(u - 0xc0) as usize] as char,
        0x100..=0x17f => EA.as_bytes().get((u - 0x100) as usize).map_or(c, |b| *b as char),
        _ => c,
    }
}

/// Snake-case words from a text string (song titles, animal names, place
/// names): lower case, accents dropped, each run of other characters made
/// one `_`; once as written and once with apostrophes dropped first
/// (`don't` -> `dont`), and the same for each side of ` - `. Words of 3 to
/// 64 characters and at most 4 parts; longer text is sentences, which only
/// add chance matches.
pub fn text_words(s: &str, out: &mut std::collections::BTreeSet<String>) {
    let t: String = plain_text(s).chars().map(|c| fold(c).to_ascii_lowercase()).collect();
    let mut parts = vec![t.as_str()];
    if t.contains(" - ") {
        parts.extend(t.split(" - "));
    }
    for p in parts {
        for drop_apos in [false, true] {
            let mut w = String::new();
            for c in p.chars().filter(|c| !(drop_apos && (*c == '\'' || *c == '\u{2019}'))) {
                if c.is_ascii_alphanumeric() {
                    w.push(c);
                } else if !w.is_empty() && !w.ends_with('_') {
                    w.push('_');
                }
            }
            let w = w.trim_end_matches('_');
            if (3..=64).contains(&w.len()) && w.matches('_').count() <= 3 {
                out.insert(w.to_string());
            }
        }
    }
}

fn plain_text(s: &str) -> String {
    crate::text::plain(s)
}

/// `<work>/names/text_words.json`: the snake-case words of the text tables
/// (`text_words`), written by `sixng names`, used by the bank and stream
/// name searches only (they are not names by themselves).
pub fn text_words_path(work: &Path) -> PathBuf {
    work.join("names").join("text_words.json")
}

fn load_text_words(work: &Path) -> Vec<String> {
    std::fs::read(text_words_path(work)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The text words of every text table of data.rpf (`sixng text`'s tables:
/// entries of sub type 7a), when it is indexed. Strings over 80 characters
/// are skipped.
fn harvest_text_words(game: &Path, index_dir: &Path) -> Option<(usize, std::collections::BTreeSet<String>)> {
    use rayon::prelude::*;
    let ix = crate::scan::load_index(&crate::scan::index_path(index_dir, "data.rpf")).ok()?;
    let arch = game.join("data.rpf");
    let sets: Vec<(usize, std::collections::BTreeSet<String>)> = ix
        .entries
        .par_iter()
        .filter(|e| e.sub == "7a" && e.usize <= 64 << 20)
        .filter_map(|e| {
            let b = crate::scan::read_entry(&arch, e).ok()?;
            if !crate::text::is_text_table(&b) {
                return None;
            }
            let t = crate::text::parse(&b).ok()?;
            let mut out = std::collections::BTreeSet::new();
            for s in t.values().filter(|s| s.chars().count() <= 80) {
                text_words(s, &mut out);
            }
            Some((1, out))
        })
        .collect();
    let mut all = std::collections::BTreeSet::new();
    let mut n = 0;
    for (k, s) in sets {
        n += k;
        all.extend(s);
    }
    Some((n, all))
}

/// `name` without a variation suffix (`_01` .. `_99`, `_a` .. `_z`).
fn stem(name: &str) -> &str {
    let b = name.as_bytes();
    let n = b.len();
    if n > 3 && b[n - 3] == b'_' && b[n - 2].is_ascii_digit() && b[n - 1].is_ascii_digit() {
        &name[..n - 3]
    } else if n > 2 && b[n - 2] == b'_' && b[n - 1].is_ascii_lowercase() {
        &name[..n - 2]
    } else {
        name
    }
}

/// Stream names. A stream id is the low 29 bits of joaat of the wave name,
/// which is mostly an identifier plus a variation (`<name>_01`,
/// `<name>_a`). Every identifier of `d` is tried as is and with `_01` ..
/// `_99` and `_a` .. `_z`, and a name is kept when two or more names of
/// the same stem match streams of the same bank.
///
/// That alone is not enough. With 29 bits, a chance match of one variation
/// makes the next ones likely to match too, so wrong stems match runs of
/// streams. On audio.rpf, shuffled words still give about a third as many
/// runs as the real word list. So a stem is tried only when its own joaat
/// (32 bits) is the name hash of an audio metadata object (`objects`):
/// sounds are mostly named like their waves. The same search with shuffled
/// words then keeps 23 bank stems, against 1,236 for the real list.
/// `streams` is bank offset -> stream ids; returns bank offset -> stream id
/// -> name.
pub fn find_stream_names(d: &Dict, extra: &[String], streams: &BTreeMap<u64, Vec<u32>>, objects: &std::collections::HashSet<u32>) -> BTreeMap<u64, BTreeMap<u32, String>> {
    let mut by_id: HashMap<u32, Vec<u64>> = HashMap::new();
    for (b, ids) in streams {
        for &s in ids {
            by_id.entry(s & 0x1fff_ffff).or_default().push(*b);
        }
    }
    let (_, words) = audio_words(d, extra);
    let words: Vec<&String> = words.iter().filter(|w| objects.contains(&joaat_fin(joaat_step(0, w.as_bytes())))).collect();
    let mut raw: BTreeMap<u64, BTreeMap<u32, String>> = BTreeMap::new();
    let mut hit = |name: String, h: u32| {
        if let Some(bs) = by_id.get(&(h & 0x1fff_ffff)) {
            for b in bs {
                raw.entry(*b).or_default().entry(h & 0x1fff_ffff).or_insert_with(|| name.clone());
            }
        }
    };
    for w in &words {
        let h0 = joaat_step(0, w.as_bytes());
        hit(w.to_string(), joaat_fin(h0));
        for n in 1..=99u32 {
            let suf = format!("_{n:02}");
            hit(format!("{w}{suf}"), joaat_fin(joaat_step(h0, suf.as_bytes())));
        }
        for c in b'a'..=b'z' {
            hit(format!("{w}_{}", c as char), joaat_fin(joaat_step(h0, &[b'_', c])));
        }
    }
    raw.into_iter()
        .filter_map(|(b, m)| {
            let mut per_stem: HashMap<&str, usize> = HashMap::new();
            for n in m.values() {
                *per_stem.entry(stem(n)).or_default() += 1;
            }
            let keep: BTreeMap<u32, String> = m.iter().filter(|(_, n)| per_stem[stem(n)] >= 2).map(|(i, n)| (*i, n.clone())).collect();
            (!keep.is_empty()).then_some((b, keep))
        })
        .collect()
}

/// Audio bank names. A bank's container hash is joaat of
/// `<pack>/<bank>` in lower case (`core.resident/gun_pistol`), where the
/// pack is one of the dotted audio pack names and the bank name is an
/// identifier. `banks` is bank offset -> container hash.
///
/// Three searches:
/// * every pack with every identifier of `d`. Banks of one pack lie next to
///   each other in the archive, so a match is kept only when the nearest
///   matched bank before or after it (by offset) has the same pack; this
///   drops the chance matches (about 1 in 36 on the game's audio.rpf);
/// * per bank, every pack with the leading and trailing word runs of its
///   stream names' stems (`stream_names`; `distant_vehicles` from
///   `distant_vehicles_cars_pass_by_01`). Only that bank's container is
///   tested, so chance matches are rare and no neighbour is needed;
/// * per bank still unnamed, the pack its bank-list group (`groups`:
///   container -> group hash) gives, with every identifier, plain and with
///   variation suffixes ([`pack_search`]).
///
/// Returns (bank offset, container, name), by offset.
pub fn find_bank_names(d: &Dict, extra: &[String], banks: &BTreeMap<u64, u32>, stream_names: &BTreeMap<u64, BTreeMap<u32, String>>, groups: &BTreeMap<u32, u32>) -> Vec<(u64, u32, String)> {
    let want: std::collections::HashSet<u32> = banks.values().copied().collect();
    let (packs, words) = audio_words(d, extra);
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
    let mut out: BTreeMap<u64, (u32, String)> = BTreeMap::new();
    for (i, (off, h, names)) in matched.iter().enumerate() {
        let near: Vec<std::collections::HashSet<String>> = [i.checked_sub(1), Some(i + 1)].into_iter().flatten().filter_map(|j| matched.get(j)).map(|m| m.2.iter().map(|n| pack(n)).collect()).collect();
        if let Some(n) = names.iter().find(|n| near.iter().any(|s| s.contains(&pack(n)))) {
            out.insert(*off, (*h, n.clone()));
        }
    }
    for (off, h) in banks {
        if out.contains_key(off) {
            continue;
        }
        let Some(sn) = stream_names.get(off) else { continue };
        let mut parts = std::collections::BTreeSet::new();
        for n in sn.values() {
            let w: Vec<&str> = stem(n).split('_').collect();
            for k in 1..=w.len() {
                parts.insert(w[..k].join("_"));
                parts.insert(w[w.len() - k..].join("_"));
            }
        }
        'found: for p in &packs {
            let h0 = joaat_step(joaat_step(0, p.as_bytes()), b"/");
            for w in &parts {
                if joaat_fin(joaat_step(h0, w.as_bytes())) == *h {
                    out.insert(*off, (*h, format!("{p}/{w}")));
                    break 'found;
                }
            }
        }
    }
    // banks whose pack the bank list's group gives
    let gnames = find_group_names(&packs, &words, &groups.values().copied().collect());
    for (off, h, n) in pack_search(&packs, &words, banks, groups, &gnames, &out) {
        out.entry(off).or_insert((h, n));
    }
    out.into_iter().map(|(o, (h, n))| (o, h, n)).collect()
}

/// Names of bank-list groups (`groups`: group hashes). A group is named
/// after the bank's audio pack without `core.`, alone or with one more
/// `.`-part (`props`, `resident.combat`); every pack with every identifier
/// of letters and `_` is tried, and the shortest match kept. On audio.rpf
/// 48 of 64 groups get a name. The parents of each pack (`speech` of
/// `core.speech.ped`) and the identifiers alone are tried too: the story
/// pack's two largest groups are `speech.scripted` and `cutscene`, whose
/// packs are not in the sound files.
pub fn find_group_names(packs: &[String], words: &[String], groups: &std::collections::HashSet<u32>) -> HashMap<u32, String> {
    let mut out: HashMap<u32, String> = HashMap::new();
    let mut put = |h: u32, n: String| {
        if groups.contains(&h) && out.get(&h).is_none_or(|o| (n.len(), &n) < (o.len(), o)) {
            out.insert(h, n);
        }
    };
    let words: Vec<&String> = words.iter().filter(|w| w.len() <= 24 && w.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')).collect();
    for w in &words {
        put(joaat_fin(joaat_step(0, w.as_bytes())), w.to_string());
    }
    let mut prefixes: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for p in packs.iter().filter_map(|p| p.strip_prefix("core.")) {
        prefixes.insert(p);
        prefixes.extend(p.match_indices('.').map(|(i, _)| &p[..i]));
    }
    for p in prefixes {
        let h0 = joaat_step(0, p.as_bytes());
        put(joaat_fin(h0), p.to_string());
        let h1 = joaat_step(h0, b".");
        for w in &words {
            put(joaat_fin(joaat_step(h1, w.as_bytes())), format!("{p}.{w}"));
        }
    }
    out
}

/// The pack of each group: the one most of its named banks (`named`: bank
/// offset -> (container, name)) share when 90 % or more of them do (a group
/// name does not always match its pack: `resident.policescanner` holds
/// `core.policescanner` banks), else the longest pack its name starts with
/// (`resident.combat` -> `core.resident`).
fn group_packs(packs: &[String], banks: &BTreeMap<u64, u32>, groups: &BTreeMap<u32, u32>, gnames: &HashMap<u32, String>, named: &BTreeMap<u64, (u32, String)>) -> HashMap<u32, String> {
    let mut seen: HashMap<u32, HashMap<&str, usize>> = HashMap::new();
    for (off, (_, n)) in named {
        if let Some(g) = banks.get(off).and_then(|c| groups.get(c)) {
            *seen.entry(*g).or_default().entry(n.split('/').next().unwrap_or("")).or_default() += 1;
        }
    }
    let mut out = HashMap::new();
    for g in groups.values().copied().collect::<std::collections::BTreeSet<_>>() {
        let from_named = seen.get(&g).and_then(|m| {
            let all: usize = m.values().sum();
            m.iter().max_by_key(|(p, k)| (**k, std::cmp::Reverse(**p))).filter(|(_, k)| **k * 10 >= all * 9).map(|(p, _)| p.to_string())
        });
        let from_name = gnames.get(&g).and_then(|n| {
            let full = format!("core.{n}");
            packs.iter().filter(|p| full == **p || full.starts_with(&format!("{p}."))).max_by_key(|p| p.len()).cloned()
        });
        if let Some(p) = from_named.or(from_name) {
            out.insert(g, p);
        }
    }
    out
}

/// Bank names within the pack of each unnamed bank's group
/// ([`group_packs`]): every identifier as is, and with `_00` .. `_99`,
/// `_1` .. `_9` and `_a` .. `_z` (not `00` .. `99` without `_`: there
/// the few runs found were joaat state collisions, a wrong stem whose
/// running hash equals the real one's, so every number after it matches). A plain match is kept; a
/// suffixed one when another bank of the same pack matches the same stem
/// with another suffix, or the stem is itself a bank of that pack
/// (`horse`, `horse_b`). On audio.rpf, the same search on containers xor a
/// constant finds no plain match and no kept suffixed match.
fn pack_search(packs: &[String], words: &[String], banks: &BTreeMap<u64, u32>, groups: &BTreeMap<u32, u32>, gnames: &HashMap<u32, String>, named: &BTreeMap<u64, (u32, String)>) -> Vec<(u64, u32, String)> {
    use rayon::prelude::*;
    let gp = group_packs(packs, banks, groups, gnames, named);
    let mut todo: BTreeMap<&str, HashMap<u32, u64>> = BTreeMap::new();
    for (off, h) in banks {
        if named.contains_key(off) {
            continue;
        }
        if let Some(p) = groups.get(h).and_then(|g| gp.get(g)) {
            todo.entry(p.as_str()).or_default().insert(*h, *off);
        }
    }
    let mut sufs: Vec<String> = Vec::new();
    sufs.extend((0..=99).map(|n| format!("_{n:02}")));
    sufs.extend((1..=9).map(|n| format!("_{n}")));
    sufs.extend((b'a'..=b'z').map(|c| format!("_{}", c as char)));
    let mut out = Vec::new();
    for (p, want) in todo {
        let h0 = joaat_step(joaat_step(0, p.as_bytes()), b"/");
        // (container, word, suffix index or none)
        let hits: Vec<(u32, &str, Option<usize>)> = words
            .par_iter()
            .flat_map_iter(|w| {
                let hw = joaat_step(h0, w.as_bytes());
                let mut v = Vec::new();
                let h = joaat_fin(hw);
                if want.contains_key(&h) {
                    v.push((h, w.as_str(), None));
                }
                for (i, s) in sufs.iter().enumerate() {
                    let h = joaat_fin(joaat_step(hw, s.as_bytes()));
                    if want.contains_key(&h) {
                        v.push((h, w.as_str(), Some(i)));
                    }
                }
                v
            })
            .collect();
        let mut done: HashMap<u32, String> = HashMap::new();
        let mut plain: std::collections::HashSet<String> = named.values().filter_map(|(_, n)| n.strip_prefix(p)?.strip_prefix('/').map(String::from)).collect();
        for (h, w, s) in &hits {
            if s.is_none() {
                done.entry(*h).or_insert_with(|| w.to_string());
                plain.insert(w.to_string());
            }
        }
        let mut per_stem: HashMap<&str, std::collections::HashSet<u32>> = HashMap::new();
        for (h, w, s) in &hits {
            if s.is_some() {
                per_stem.entry(w).or_default().insert(*h);
            }
        }
        for (h, w, s) in &hits {
            if let Some(i) = s {
                if per_stem[w].len() >= 2 || plain.contains(*w) {
                    done.entry(*h).or_insert_with(|| format!("{w}{}", sufs[*i]));
                }
            }
        }
        for (h, n) in done {
            out.push((want[&h], h, format!("{p}/{n}")));
        }
    }
    out
}

/// Replace the `banks` names of `d` with the bank names of `banks` (bank
/// offset -> container hash) and add the names of the bank-list groups.
/// Returns counts, and each named group with its bank-list entries.
pub fn set_bank_names(d: &mut Dict, extra: &[String], banks: &BTreeMap<u64, u32>, stream_names: &BTreeMap<u64, BTreeMap<u32, String>>, groups: &BTreeMap<u32, u32>) -> serde_json::Value {
    for v in d.names.values_mut() {
        v.retain(|(_, s)| s != "banks");
    }
    d.names.retain(|_, v| !v.is_empty());
    let found = find_bank_names(d, extra, banks, stream_names, groups);
    // the bank-list groups, by name, with their entry counts
    let (packs, words) = audio_words(d, extra);
    let gnames = find_group_names(&packs, &words, &groups.values().copied().collect());
    let mut entries: BTreeMap<u32, usize> = BTreeMap::new();
    for g in groups.values() {
        *entries.entry(*g).or_default() += 1;
    }
    let named_groups: BTreeMap<&str, usize> = gnames.iter().map(|(h, n)| (n.as_str(), entries.get(h).copied().unwrap_or(0))).collect();
    for (h, n) in gnames.iter().map(|(h, n)| (*h, n.clone())).chain(found.iter().map(|(_, h, n)| (*h, n.clone()))).collect::<Vec<_>>() {
        let e = d.names.entry(format!("{h:08x}")).or_default();
        if !e.iter().any(|(x, s)| *x == n && s == "banks") {
            e.push((n, "banks".into()));
        }
    }
    serde_json::json!({"banks": banks.len(), "named": found.len(), "banks_with_stream_names": stream_names.len(), "stream_names": stream_names.values().map(|m| m.len()).sum::<usize>(), "groups": entries.len(), "groups_named": named_groups})
}

/// `<work>/index/audio_stream_names_<archive>.json`: bank offset (hex) ->
/// stream id (hex) -> name.
pub fn stream_names_path(work: &Path, archive: &str) -> PathBuf {
    work.join("index").join(format!("audio_stream_names_{}.json", archive.trim_end_matches(".rpf").replace(['/', '\\'], "_")))
}

pub fn load_stream_names(work: &Path, archive: &str) -> BTreeMap<u64, BTreeMap<u32, String>> {
    let v: BTreeMap<String, BTreeMap<String, String>> = std::fs::read(stream_names_path(work, archive)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    v.into_iter().filter_map(|(b, m)| Some((u64::from_str_radix(&b, 16).ok()?, m.into_iter().filter_map(|(i, n)| Some((u32::from_str_radix(&i, 16).ok()?, n))).collect()))).collect()
}

/// Every stream name of an archive: the ones its banks store (in the audio
/// index), then the searched ones of [`stream_names_path`] for the rest.
pub fn all_stream_names(work: &Path, ix: &crate::audio::AudioIndex) -> BTreeMap<u64, BTreeMap<u32, String>> {
    let mut m = load_stream_names(work, &ix.archive);
    for b in ix.banks.iter().filter(|b| !b.names.is_empty()) {
        m.entry(b.off).or_default().extend(b.names.iter().map(|(i, n)| (*i, n.clone())));
    }
    m
}

/// Stream names and bank names for one archive into `d`, the stream names
/// saved beside the audio index.
fn name_audio(d: &mut Dict, work: &Path, archive: &str, banks: &BTreeMap<u64, u32>, streams: &BTreeMap<u64, Vec<u32>>) -> Result<serde_json::Value> {
    let extra = load_text_words(work);
    // the names the banks store themselves (name-tag chunks) come first
    let embedded: BTreeMap<u64, BTreeMap<u32, String>> = crate::audio::load_index(work, archive).map(|ix| ix.banks.into_iter().filter(|b| !b.names.is_empty()).map(|b| (b.off, b.names)).collect()).unwrap_or_default();
    let rest: BTreeMap<u64, Vec<u32>> = streams.iter().map(|(b, ids)| (*b, ids.iter().copied().filter(|i| !embedded.get(b).is_some_and(|m| m.contains_key(i))).collect())).collect();
    // then the search, for the other streams; without the metadata object
    // hashes (an older `audio-meta`) it is skipped
    let mut sn = crate::audiometa::load_objects(work, archive).map(|o| find_stream_names(d, &extra, &rest, &o)).unwrap_or_default();
    let searched: usize = sn.values().map(|m| m.len()).sum();
    for (b, m) in &embedded {
        sn.entry(*b).or_default().extend(m.iter().map(|(i, n)| (*i, n.clone())));
    }
    let v: BTreeMap<String, BTreeMap<String, &String>> = sn.iter().map(|(b, m)| (format!("{b:x}"), m.iter().map(|(i, n)| (format!("{i:08x}"), n)).collect())).collect();
    let p = stream_names_path(work, archive);
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, serde_json::to_vec(&v)?)?;
    let mut r = set_bank_names(d, &extra, banks, &sn, &crate::audiometa::load_groups(work, archive).unwrap_or_default());
    r["stream_names_stored"] = embedded.values().map(|m| m.len()).sum::<usize>().into();
    r["banks_with_stored_names"] = embedded.len().into();
    r["stream_names_searched"] = searched.into();
    Ok(r)
}

/// Name the banks of `banks` (bank offset -> container hash) and the
/// streams of `streams` (bank offset -> stream ids) of an archive: bank
/// names go into `<work>/names/names.json` (source `banks`; run `sixng
/// names` first), stream names into [`stream_names_path`].
pub fn add_bank_names(work: &Path, archive: &str, banks: &BTreeMap<u64, u32>, streams: &BTreeMap<u64, Vec<u32>>) -> Result<serde_json::Value> {
    let p = dict_path(work);
    let mut d: Dict = serde_json::from_slice(&std::fs::read(&p).with_context(|| format!("no name dictionary {} (run `sixng names`)", p.display()))?)?;
    let r = name_audio(&mut d, work, archive, banks, streams)?;
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
        let none = BTreeMap::new();
        let found: Vec<(u64, String)> = find_bank_names(&d, &[], &banks, &none, &BTreeMap::new()).into_iter().map(|(o, _, n)| (o, n)).collect();
        assert_eq!(found, [(0x1000, "pack.alpha/guns".into()), (0x2000, "pack.alpha/doors".into()), (0x3000, "pack.beta/rain".into()), (0x4000, "pack.beta/wind".into()), (0x6000, "pack.beta/doors".into()), (0x8000, "pack.beta/guns".into())]);
        let r = set_bank_names(&mut d, &[], &banks, &none, &BTreeMap::new());
        assert_eq!(r["named"], 6);
        assert_eq!(d.names[&format!("{:08x}", h("pack.beta/rain"))], [("pack.beta/rain".to_string(), "banks".to_string())]);
        // a second run replaces, not adds
        set_bank_names(&mut d, &[], &banks, &none, &BTreeMap::new());
        assert_eq!(d.names.values().flatten().filter(|(_, s)| s == "banks").count(), 6);
        // the lone bank is named from its own streams' stem: `zzz_far_off`
        // gives `zzz` (leading run), which joins pack.alpha to its container
        let sn: BTreeMap<u64, BTreeMap<u32, String>> = [(0x5000, [(1, "zzz_far_off_01".to_string())].into_iter().collect())].into_iter().collect();
        let found = find_bank_names(&d, &[], &banks, &sn, &BTreeMap::new());
        assert!(found.iter().any(|(o, _, n)| *o == 0x5000 && n == "pack.alpha/zzz"));
    }
    #[test]
    fn group_pack_search() {
        let mut d = Dict::default();
        let mut add = |n: &str, src: &str| d.names.entry(format!("{:08x}", joaat(n))).or_default().push((n.to_string(), src.to_string()));
        add("core.pack", "audio");
        add("core.other", "audio");
        for w in ["sub", "bell", "horn", "lone"] {
            add(w, "common");
        }
        let h = |n: &str| joaat(n);
        // far apart and between unknown banks, so only the group finds them
        let banks: BTreeMap<u64, u32> = [(0x1000, h("core.pack/bell")), (0x2000, 1), (0x3000, h("core.pack/horn_01")), (0x4000, 2), (0x5000, h("core.pack/horn_02")), (0x6000, 3), (0x7000, h("core.pack/lone_07")), (0x8000, h("core.pack/bell_b"))].into_iter().collect();
        let g = h("pack.sub");
        let groups: BTreeMap<u32, u32> = banks.values().map(|c| (*c, g)).collect();
        let (packs, words) = audio_words(&d, &[]);
        assert_eq!(find_group_names(&packs, &words, &[g].into_iter().collect())[&g], "pack.sub");
        // a bare word, and a word after a pack's parent
        let gs = [h("horn"), h("core.bell")];
        let gn = find_group_names(&["core.core.pack".to_string()], &words, &gs.into_iter().collect());
        assert_eq!((gn[&gs[0]].as_str(), gn[&gs[1]].as_str()), ("horn", "core.bell"));
        let found: Vec<(u64, String)> = find_bank_names(&d, &[], &banks, &BTreeMap::new(), &groups).into_iter().map(|(o, _, n)| (o, n)).collect();
        // `lone_07` has no second suffix and no plain bank: not kept
        assert_eq!(found, [(0x1000, "core.pack/bell".into()), (0x3000, "core.pack/horn_01".into()), (0x5000, "core.pack/horn_02".into()), (0x8000, "core.pack/bell_b".into())]);
        // the group's name is reported with its entries and joins the dictionary
        let r = set_bank_names(&mut d, &[], &banks, &BTreeMap::new(), &groups);
        assert_eq!((r["groups"].as_u64(), r["groups_named"]["pack.sub"].as_u64()), (Some(1), Some(8)));
        assert_eq!(d.names[&format!("{g:08x}")], [("pack.sub".to_string(), "banks".to_string())]);
    }
    #[test]
    fn text_word_forms() {
        let mut w = std::collections::BTreeSet::new();
        text_words("Some Artist - Don't Stop (Café Mix)", &mut w);
        text_words("Blue Bird", &mut w);
        text_words("~b~Red Fox~s~", &mut w);
        text_words("This sentence has far too many words in it", &mut w);
        text_words("Ab", &mut w);
        // `don_t_stop_cafe_mix` (apostrophe kept as a break) has 5 parts: dropped
        let v: Vec<&str> = w.iter().map(|s| s.as_str()).collect();
        assert_eq!(v, ["blue_bird", "dont_stop_cafe_mix", "red_fox", "some_artist"]);
        assert_eq!((fold('é'), fold('ñ'), fold('Ł'), fold('k')), ('e', 'n', 'L', 'k'));
    }
    #[test]
    fn stream_name_search() {
        assert_eq!((stem("rain_light_01"), stem("rain_light_b"), stem("rain_light"), stem("x_1")), ("rain_light", "rain_light", "rain_light", "x_1"));
        let mut d = Dict::default();
        for w in ["rain_light", "thunder", "lonely"] {
            d.names.entry(format!("{:08x}", joaat(w))).or_default().push((w.to_string(), "common".to_string()));
        }
        let id = |n: &str| joaat(n) & 0x1fff_ffff;
        // bank 1: three variations of one stem (kept) and one lone name
        // (dropped: one only); bank 2: two of a stem that is no metadata
        // object (dropped); bank 3: two of `thunder` (kept)
        let streams: BTreeMap<u64, Vec<u32>> = [(1, vec![id("rain_light_01"), id("rain_light_02"), id("rain_light_c"), id("lonely_07"), 12345]), (2, vec![id("lonely_01"), id("lonely_02")]), (3, vec![id("thunder_03"), id("thunder_05")])].into_iter().collect();
        let objects: std::collections::HashSet<u32> = [joaat("rain_light"), joaat("thunder")].into_iter().collect();
        let sn = find_stream_names(&d, &[], &streams, &objects);
        assert_eq!(sn.keys().copied().collect::<Vec<_>>(), [1, 3]);
        let mut v: Vec<&str> = sn[&1].values().map(|s| s.as_str()).collect();
        v.sort();
        assert_eq!(v, ["rain_light_01", "rain_light_02", "rain_light_c"]);
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
