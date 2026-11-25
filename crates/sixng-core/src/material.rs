//! Material classification without shader names.
//!
//! Shader and parameter names are hashed with an unknown function, so
//! classes come from two structural sources measured over
//! 254 K drawables (`sixng shaders` census):
//! - the shader record's flag bytes at +0x38..+0x40. Byte 4 is a render-bucket
//!   bit mask (0x01 opaque, 0x02 alpha, 0x04 decal, 0x08 cutout, 0x40 water);
//!   byte 1 repeats it as an index and sets 0x80 for instanced / batch-LOD
//!   draws.
//! - known shader families, identified from the models that use them.
use serde_json::{json, Value};

/// (shader name hash, shader name, class, family description). The names are
/// the game's effect names (`sixng shader-bindings`); a test checks each one
/// against its hash. A family is `emissive` only when its shader lights
/// something: an `emissivetex` input, or a colour map that is the light
/// itself (`signs_emissive`, which has no other input).
const FAMILIES: &[(&str, &str, &str, &str)] = &[
    // vehicles
    ("4aa38282", "standard_vehicle_2lyr", "car_paint", "vehicle body (paint is a runtime colour; colour slot = engine-default white)"),
    ("476ac73a", "standard_vehicle_2lyr_lod", "car_paint", "vehicle body, low LOD"),
    ("03b27476", "standard_vehicle_2lyr_reflector", "opaque", "vehicle lamp reflectors"),
    ("4d2b05e7", "standard_vehicle_2lyr_cutout", "car_paint", "vehicle body, cutout"),
    ("c5cdbdd2", "standard_vehicle_2lyr_wheel_inst", "car_paint", "wheel rims (the export gives them a rim stand-in)"),
    ("389206d1", "standard_vehicle_alpha", "glass", "vehicle lamp lenses and clear covers (opacity in the colour alpha)"),
    ("29648098", "standard_vehicle_emissive_animated_bones", "emissive", "vehicle lamps"),
    ("5a693bbe", "standard_vehicle_emissive_layer", "emissive", "vehicle lamps and lit panels"),
    ("99ad6fc9", "standard_vehicle_2lyr_tyre_inst", "opaque", "vehicle tyre"),
    ("fa9cc069", "standard_vehicle_glass", "window", "vehicle windows"),
    ("4dc0ac6a", "standard_vehicle_2lyr_alpha", "window", "vehicle windows (helicopters, boats)"),
    ("bd71bdbf", "windshield_wipermask", "window", "windscreen wiper sweep mask: a fan over the glass that marks where the wipers clear rain; not a visible surface"),
    ("921b19e5", "standard_vehicle_2lyr_signs", "decal", "vehicle livery / sign decal"),
    // glass and liquids
    ("298c4c0f", "standard_glass_breakable_dirt", "glass", "exterior window panes"),
    ("653582a4", "standard_glass_breakable_sticker", "glass", "exterior glass"),
    ("6a817597", "standard_glass_breakable", "glass", "interior glass, curtains, doors"),
    ("656a1cdb", "standard_alpha_liquid_open_vessel", "glass", "bottles and drinking glasses"),
    ("12839ca7", "standard_alpha_liquid_open_vessel_opaque_liquid", "glass", "drinking glasses with liquid"),
    ("9ceb543e", "standard_alpha_liquid_opaque_liquid", "glass", "cups with liquid"),
    ("9f8ae601", "standard_alpha_liquid", "glass", "bottles"),
    ("0357215a", "standard_alpha_liquid_opaque_vessel", "glass", "cups"),
    // plain forward alpha: no emissive input
    ("e3ba3129", "standard_alpha", "alpha", "forward alpha: building doors and glazing, signs, many props"),
    ("d9ec330c", "standard_alpha_inst", "alpha", "forward alpha, instanced (light-bulb glass among others)"),
    ("d5a93b6f", "standard_alpha_secmo", "alpha", "forward alpha: glass, sheer cloth"),
    ("5b775918", "standard_anisotropy_dirt_alpha", "alpha", "forward alpha with anisotropic highlights and a dirt layer (phone screens among others)"),
    // emissive
    ("faec4fc8", "signs_emissive", "emissive", "digital signs / screens (one colour map, which is the light)"),
    ("60643f2e", "signs_emissive_inst", "emissive", "digital signs, instanced (traffic-light counters among others)"),
    ("f5ba53da", "standard_emissive_lightmap_v2", "emissive", "baked lightmap overlay (emissivetex)"),
    ("3bc8ac60", "standard_emissive_lightmap", "emissive", "baked lightmap overlay (emissivetex)"),
    ("3a09c775", "standard_emissive_layer_pxm_inst", "emissive", "emissive layer, parallax, instanced: LED lights and panels (emissivetex)"),
    ("db8b21bd", "standard_emissive_layer_alpha", "emissive", "forward alpha with an emissive layer: glass railings, door glass (emissivetex)"),
    // water
    ("7f3a6aa9", "water_micro_geom", "water", "water surface"),
    ("97596162", "standard_alpha_water", "water", "fountain / VFX water"),
    ("735d2de6", "standard_alpha_waterfall", "water", "waterfall VFX"),
    // terrain / multilayer
    ("61e09df7", "standard_uber_4lyr_dpm_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("360e396b", "standard_uber_4lyr_spx_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("b21f1ba5", "standard_uber_4lyr_pxm_hbb_fixuv1", "terrain_multilayer", "landscape blend (17 params)"),
    ("0ad54cb9", "standard_uber_4lyr_dpm_dm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("27e5c49d", "standard_uber_4lyr_pxm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("61fad45c", "standard_uber_4lyr_dpm_dfm_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("40497278", "standard_uber_4lyr_dpm_dfm_dm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("4a579bbf", "standard_uber_4lyr_pxm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("76fcff28", "standard_uber_4lyr_spx_dm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("88a32075", "standard_uber_4lyr", "terrain_multilayer", "landscape blend (17 params)"),
    ("388f8fb6", "standard_uber_4lyr_dfm_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("f8e1f225", "standard_uber_4lyr_dfm_dm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("d5bdc029", "standard_uber_4lyr_hbo_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("6f968246", "standard_uber_4lyr_mud_dm_hbb", "terrain_multilayer", "landscape blend (17 params)"),
    ("1ae17fd6", "standard_uber_4lyr_mud_dm_hbb_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("5a71e7e1", "standard_uber_4lyr_pxm", "terrain_multilayer", "landscape blend (17 params)"),
    ("10098c2c", "standard_uber_4lyr_pxm_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("7543cefc", "standard_uber_4lyr_uv3", "terrain_multilayer", "landscape blend (17 params)"),
    ("77d8ab27", "standard_2lyr", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("1251cae5", "standard_2lyr_pxm_ground", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("cd8ceb5e", "standard_2lyr_ground", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("4f0b809b", "standard_2lyr_pxm_decal_ground", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("b217b263", "standard_2lyr_pxm", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    // other
    ("255417c3", "trees_standard_wind", "cutout", "vegetation"),
    ("e05f1a42", "distance_map", "decal", "road signs"),
    ("89a1cf72", "ped_standard_eye", "opaque", "ped eyes"),
    ("379efcef", "ped_standard_head", "opaque", "ped head / skin"),
];

/// What each known texture parameter slot holds. Roles marked "likely" come
/// from formats and channel statistics, not from names.
pub const SLOTS: &[(&str, &str)] = &[
    ("c8e8c282", "base_color (sRGB)"),
    ("1e77bf4c", "normal (BC1, XY in RG, DirectX green)"),
    ("84c69350", "normal (BC5, DirectX green)"),
    ("58b6e707", "normal (BC5, DirectX green)"),
    ("4d56d7fe", "normal atlas (vehicle, BC1)"),
    ("2bb2ede8", "normal atlas (vehicle, BC1)"),
    ("739e3d0d", "packed material data, linear (likely roughness/metal/AO; channel order unverified)"),
    ("c6f46e79", "packed material data, linear (G/B near 0.5: likely detail-normal or mask pair)"),
    ("0df47048", "secondary colour / detail albedo (sRGB)"),
    ("7485dafb", "colour with opacity in alpha (sRGB)"),
    ("f151c063", "vehicle decal layer (RGBA)"),
    ("d37026cb", "vehicle dirt mask"),
    ("640111c6", "vehicle dirt mask"),
    ("b9f4bdac", "vehicle grime"),
    ("84d22f74", "vehicle scratches"),
    ("fb03ae36", "vehicle mask pack (green-dominant)"),
    ("19df25d7", "vehicle material palette (128x256 RGBA8 colour bands, one texture shared by the two-layer vehicle shaders; how it is indexed is not decoded, so it is never a base colour)"),
    ("8e0d63ee", "emissive map (emissivetex: the lit parts of a lamp or panel)"),
    ("2c8d7182", "tint palette (embedded in the model: rows of RGBA8 colours, picked per instance by the placement tint row)"),
];

pub fn slot_role(param_hash: &str) -> Option<&'static str> {
    SLOTS.iter().find(|s| s.0 == param_hash).map(|s| s.1).or_else(|| u32::from_str_radix(param_hash, 16).ok().and_then(crate::headskin::extra_role))
}

/// Classify a shader. `flags` = the 8 record bytes at +0x38.
pub fn classify(name_hash: &str, flags: &[u8]) -> Value {
    let b1 = flags.get(1).copied().unwrap_or(0);
    let b4 = flags.get(4).copied().unwrap_or(0);
    let bucket = match b4 {
        x if x & 0x40 != 0 => "water",
        x if x & 0x04 != 0 => "decal",
        x if x & 0x02 != 0 => "alpha",
        x if x & 0x08 != 0 => "cutout",
        x if x & 0x01 != 0 => "opaque",
        _ => "unknown",
    };
    let fam = FAMILIES.iter().find(|f| f.0 == name_hash);
    // Emissive has no bucket bit: vehicle shaders outside the table are
    // emissive by their recovered name (`standard_vehicle_emissive…`). Map
    // shaders named `…emissive…` keep their bucket class here (the glTF
    // export lights them by name).
    let name = crate::names::shader(name_hash);
    let named_emissive = fam.is_none() && name.as_deref().is_some_and(|n| n.starts_with("standard_vehicle") && n.contains("emissive"));
    let class = fam.map(|f| f.2).unwrap_or(if named_emissive { "emissive" } else { bucket });
    // Parts whose colour is not in a texture (it comes from the vehicle
    // material palette, not decoded): a stand-in surface for converters.
    let stand_in = match (name_hash, name.as_deref()) {
        ("99ad6fc9", _) => Some("rubber"),
        (_, Some(n)) if n.starts_with("standard_vehicle") && n.contains("tyre") => Some("rubber"),
        ("03b27476", _) => Some("chrome"),
        // `weapon`: no colour map (two packed data maps and a normal map);
        // the colour comes from a material palette that is not decoded
        ("e68a4a98", _) => Some("gunmetal"),
        // wiper sweep mask: drawn by the game only into its rain mask
        ("bd71bdbf", _) => Some("hidden"),
        // vehicle_water_stencil: a lid over a boat's hull opening that the
        // game draws only into the stencil buffer, to keep water out
        ("fc34f08b", _) => Some("hidden"),
        _ => None,
    };
    // Known opaque families keep OPAQUE even when the bucket says alpha.
    let alpha_mode = match (class, bucket) {
        ("opaque" | "car_paint", "cutout") => "MASK",
        ("opaque" | "car_paint" | "terrain_multilayer", _) => "OPAQUE",
        ("glass" | "window" | "water" | "alpha" | "decal", _) => "BLEND",
        (_, "alpha" | "decal" | "water") => "BLEND",
        (_, "cutout") => "MASK",
        _ => "OPAQUE",
    };
    // Decal-bucket materials (road markings, grime, signage, terrain blend
    // layers) are coplanar with the surface below them. No per-material
    // priority is known in the shader record (bytes 2-3 are a u16, likely
    // the constant-buffer size), so the order comes from the class:
    // terrain blend layers first (0), then markings / grime / signage (1).
    // bias_m is a recommended offset along the normal (or an equivalent
    // depth bias) for converters.
    let decal = if bucket == "decal" {
        let order: u8 = if class == "terrain_multilayer" { 0 } else { 1 };
        json!({"bias_m": 0.004 + 0.002 * order as f64, "order": order, "order_source": "class: terrain blend layers 0, other decals 1 (no per-material priority found in the record)", "depth_write": alpha_mode == "OPAQUE" || alpha_mode == "MASK"})
    } else {
        Value::Null
    };
    json!({
        "class": class,
        "render_bucket": bucket,
        "decal": decal,
        "alpha_mode": alpha_mode,
        "instanced_or_batch_lod": b1 & 0x80 != 0,
        "family": fam.map(|f| f.3),
        "shader": fam.map(|f| f.1.to_string()).or(name.clone()),
        "stand_in": stand_in,
        "source": if fam.is_some() { "known shader family + record flags" } else if named_emissive { "shader name (emissive) + record flags" } else { "record flags (+0x38 byte 4)" },
        "record_flags": flags.iter().map(|x| format!("{x:02x}")).collect::<String>(),
    })
}

/// Paint block for car-paint materials: what a converter needs to rebuild it.
pub fn paint_block() -> Value {
    json!({
        "colour": null,
        "colour_source": "runtime (per-vehicle colour tables not decoded); the converter chooses the colour",
        "suggested_default_linear_rgb": [0.55, 0.55, 0.55],
        "masks": {"decal": "f151c063", "dirt": ["d37026cb", "640111c6"], "grime": "b9f4bdac", "scratches": "84d22f74", "mask_pack": "fb03ae36"},
        "csp_hint": "smCarPaint: txDiffuse = white * paint colour, use dirt/scratch masks for txMaps where wanted; clear coat on",
        "beamng_hint": "instanceDiffuse + colorPaletteMap; base colour white so the paint colour shows",
    })
}

/// joaat("emissivemultiplier"): the material constant that scales how bright
/// its emissive map (or lit colour map) is.
pub const EMISSIVE_MULTIPLIER_PARAM: &str = "5eebed48";

/// The colour of the light (float3, linear), the constant right after
/// `emissivemultiplier` in the emissive shaders' buffer. Its name is not
/// recovered. Neon tubes carry their colour here (light blue, red, green,
/// yellow) over a white emissive map; LEDs with the colour in the map have
/// white.
pub const EMISSIVE_TINT_PARAM: &str = "7ab0fd74";

/// The game's `emissivemultiplier` per glTF unit of emissive strength. The
/// values read like luminance in nits (5 on glass railings, about 600 on a
/// bulb, about 3200 on an LED sign), so 1.0 in the glTF is 100 nits: about
/// the white of a display, which a viewer without exposure control shows
/// as full white.
pub const EMISSIVE_NITS_PER_UNIT: f32 = 100.0;

/// Highest glTF emissive strength written (10 000 nits).
pub const EMISSIVE_MAX_STRENGTH: f32 = 100.0;

/// How bright a lit material is in the glTF, from its `emissivemultiplier`
/// (None when the material has none or it did not read).
///
/// `strength` = multiplier / [`EMISSIVE_NITS_PER_UNIT`], at most
/// [`EMISSIVE_MAX_STRENGTH`]. Up to 1 it goes into `emissiveFactor`; above
/// 1 the factor stays 1 and the rest goes into
/// `KHR_materials_emissive_strength`, as that extension asks.
///
/// A multiplier of 0 (a traffic-light counter, among others) is a light the
/// game switches on while it runs. It is kept lit at strength 1, like a
/// material with no multiplier, so it does not export dark. A negative or
/// non-finite value is treated as no multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmissiveStrength {
    /// The material's `emissivemultiplier`, as read.
    pub multiplier: Option<f32>,
    /// The glTF strength: emissiveFactor x KHR_materials_emissive_strength.
    pub strength: f32,
    /// The light's colour (`EMISSIVE_TINT_PARAM`), when the material has
    /// one that is finite, not negative and not black.
    pub tint: Option<[f32; 3]>,
    /// `multiplier` (the multiplier divided by the unit), `default` (no
    /// multiplier: strength 1), `runtime` (multiplier 0: strength 1) or
    /// `capped` (over [`EMISSIVE_MAX_STRENGTH`]).
    pub source: &'static str,
}

impl EmissiveStrength {
    pub fn from_multiplier(multiplier: Option<f32>) -> Self {
        let (strength, source) = match multiplier {
            Some(m) if m.is_finite() && m > 0.0 => {
                let s = m / EMISSIVE_NITS_PER_UNIT;
                if s > EMISSIVE_MAX_STRENGTH {
                    (EMISSIVE_MAX_STRENGTH, "capped")
                } else {
                    (s, "multiplier")
                }
            }
            Some(m) if m == 0.0 => (1.0, "runtime"),
            _ => (1.0, "default"),
        };
        EmissiveStrength { multiplier: multiplier.filter(|m| m.is_finite()), strength, source, tint: None }
    }

    /// The light's colour from the tint constant's components.
    pub fn with_tint(mut self, tint: Option<&[f32]>) -> Self {
        self.tint = tint
            .filter(|t| t.len() >= 3)
            .map(|t| [t[0], t[1], t[2]])
            .filter(|t| t.iter().all(|x| x.is_finite() && *x >= 0.0) && t.iter().any(|x| *x > 0.001));
        self
    }

    /// The grey part of the glTF `emissiveFactor` (at most 1).
    pub fn factor(&self) -> f32 {
        self.strength.min(1.0)
    }

    /// The glTF `emissiveFactor`: the tint (white without one) times
    /// [`Self::factor`].
    pub fn factor_rgb(&self) -> [f32; 3] {
        let t = self.tint.unwrap_or([1.0; 3]);
        let f = self.factor();
        [t[0] * f, t[1] * f, t[2] * f]
    }

    /// `KHR_materials_emissive_strength.emissiveStrength`, when above 1.
    pub fn extension(&self) -> Option<f32> {
        (self.strength > 1.0).then_some(self.strength)
    }

    /// Set the emissive factor and extension of a glTF material that lights.
    pub fn apply(&self, m: &mut Value) {
        m["emissiveFactor"] = json!(self.factor_rgb());
        match self.extension() {
            Some(s) => m["extensions"]["KHR_materials_emissive_strength"] = json!({"emissiveStrength": s}),
            None => {
                if let Some(x) = m.get_mut("extensions").and_then(|x| x.as_object_mut()) {
                    x.remove("KHR_materials_emissive_strength");
                }
            }
        }
    }

    /// The record written to model.json, materials.json and the GLB extras.
    pub fn json(&self) -> Value {
        json!({"multiplier": self.multiplier, "strength": self.strength, "source": self.source, "tint": self.tint})
    }
}

/// glTF material fields for a classification: alpha mode and stand-in PBR
/// factors so untextured classes (paint, glass) render sensibly.
pub fn gltf_fields(c: &Value, has_base: bool) -> Value {
    let class = c["class"].as_str().unwrap_or("opaque");
    let mut m = json!({"alphaMode": c["alpha_mode"]});
    if c["alpha_mode"] == "MASK" {
        m["alphaCutoff"] = json!(0.5);
    }
    match class {
        "car_paint" => {
            m["pbrMetallicRoughness"] = json!({"metallicFactor": 0.6, "roughnessFactor": 0.25});
            if !has_base {
                m["pbrMetallicRoughness"]["baseColorFactor"] = json!([0.55, 0.55, 0.55, 1.0]);
            }
        }
        "glass" | "window" => {
            m["pbrMetallicRoughness"] = json!({"metallicFactor": 0.0, "roughnessFactor": 0.05});
            if !has_base {
                m["pbrMetallicRoughness"]["baseColorFactor"] = json!([0.08, 0.09, 0.1, 0.35]);
            }
        }
        // Lamps, signs, screens: the colour map lights itself. "base" is
        // resolved by the glTF writer to the base-colour texture (and
        // dropped when there is none).
        "emissive" => {
            m["emissiveFactor"] = json!([1.0, 1.0, 1.0]);
            m["emissiveTexture"] = json!("base");
        }
        "water" => m["pbrMetallicRoughness"] = json!({"metallicFactor": 0.0, "roughnessFactor": 0.05, "baseColorFactor": [0.1, 0.2, 0.25, 0.6]}),
        _ => {}
    }
    // Stand-ins (linear): tyre rubber, lamp-reflector chrome.
    let stand = match c["stand_in"].as_str() {
        Some("rubber") => Some((0.0, 0.85, [0.02, 0.02, 0.022, 1.0])),
        Some("chrome") => Some((1.0, 0.12, [0.9, 0.9, 0.9, 1.0])),
        // interior plastics, fabric, mechanical parts: colours not decoded
        Some("trim") => Some((0.0, 0.6, [0.11, 0.11, 0.115, 1.0])),
        // wheel rims: colour set at runtime; a plain alloy
        Some("rim") => Some((1.0, 0.35, [0.6, 0.6, 0.62, 1.0])),
        // vehicle parts coloured per vertex (COLOR_0): a white base
        Some("parts") => Some((0.0, 0.5, [1.0, 1.0, 1.0, 1.0])),
        // weapons: dark blued steel for every part (wood and polymer
        // furniture included, their colours are not decoded)
        Some("gunmetal") => Some((0.7, 0.45, [0.05, 0.05, 0.055, 1.0])),
        // weapon parts coloured per vertex (COLOR_0) from their
        // layered-material preset: a white base
        Some("weapon_parts") => Some((0.7, 0.45, [1.0, 1.0, 1.0, 1.0])),
        _ => None,
    };
    if let Some((metal, rough, colour)) = stand {
        m["pbrMetallicRoughness"] = json!({"metallicFactor": metal, "roughnessFactor": rough});
        if !has_base {
            m["pbrMetallicRoughness"]["baseColorFactor"] = json!(colour);
        }
    }
    // Not a visible surface (wiper sweep mask, runtime screen overlay):
    // a cutout with alpha 0, which every glTF viewer discards.
    if c["stand_in"] == "hidden" {
        m = json!({"alphaMode": "MASK", "alphaCutoff": 0.5, "pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.0, 0.0, 0.0], "metallicFactor": 0.0, "roughnessFactor": 1.0}});
    }
    m
}

/// `--paint` colour: sRGB hex RRGGBB or AARRGGBB (`#` optional; alpha
/// ignored) as linear RGB.
pub fn parse_paint(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#').trim_start_matches("0x");
    let v = u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6 || h.len() == 8)?;
    let lin = |c: u32| {
        let x = c as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    Some([lin((v >> 16) & 0xff), lin((v >> 8) & 0xff), lin(v & 0xff)])
}

/// A tyre shader of the shared `tyre_*` models:
/// `standard_vehicle_2lyr_tyre_inst` (99ad6fc9), its `_lod` variant
/// (b994d670, the only material of `tyre_truck_001`), or any other
/// `standard_vehicle...tyre...` shader by its recovered name.
pub fn is_tyre_shader(name_hash: &str) -> bool {
    matches!(name_hash, "99ad6fc9" | "b994d670") || crate::names::shader(name_hash).is_some_and(|n| n.starts_with("standard_vehicle") && n.contains("tyre"))
}

pub fn is_paint(name_hash: &str) -> bool {
    FAMILIES.iter().any(|f| f.0 == name_hash && f.2 == "car_paint")
}

/// Layer layout of the two-layer vertex-blended families (terrain_multilayer
/// walls/ground and the terrain blend decal), or None.
///
/// Evidence (2026-11-20, 2,868-model sample, 4.7 K multilayer materials):
/// * Slots group by texture size: inside a material the slots of one group
///   always share their aspect ratio, and the groups differ from each other
///   (b217b263 / 1251cae5 / cd8ceb5e / 77d8ab27: {p0 colour, p6 normal,
///   p7 data, p9 two-channel} and {p4 colour, p10 normal, p5 data, p2
///   two-channel}; 4f0b809b: {p2, p10, p3, p0} and {p7, p4, p5, p9}).
///   `prop_2lyr_batch` 89c8fe07 (rocks, containers) has the first layout,
///   and its recovered slot names confirm it (`lyr0diffusetex`,
///   `lyr0normaltex`, `lyr0materialbtex`, `lyr0materialatex`, `lyr1…`,
///   `dirtdiffusetex` for the overlay).
///   Confidence: high for the grouping.
/// * COLOR0.a is the only vertex channel painted over the full 0..255 range
///   (R is mostly a constant 255, G stays within 0..45, B has a few
///   distinct values); A = 0 on most vertices, so A weights layer 2 over
///   the base. Confidence: medium.
/// * UV sets: layer 2 follows TEXCOORD_1 (texel anisotropy matches its
///   textures in 108 of 117 primitives for 1251cae5, 54 of 72 for
///   cd8ceb5e); the base uses TEXCOORD_0 by elimination (TEXCOORD_2 is
///   often constant). Confidence: medium (layer 2), low (base).
/// * Data maps (BC7): R varies (AO-like), G is nearly constant per texture,
///   B and A vary. Two-channel maps (BC1): R = 0, G/B vary and are
///   anti-correlated. Meanings unverified.
pub fn layers(name_hash: &str, entries: &[Value]) -> Option<Value> {
    let tex = |p: usize| -> Value {
        match entries.iter().find(|e| e["param"].as_u64() == Some(p as u64)) {
            Some(e) => json!({"param": p, "png": e.get("png").cloned().unwrap_or(Value::Null), "status": e.get("status").cloned().unwrap_or(Value::Null)}),
            None => json!({"param": p, "png": null, "status": "no texture bound"}),
        }
    };
    if let Some(v) = FOUR_LAYER.iter().find(|v| v.0 == name_hash) {
        return Some(four_layer(v.1, v.2, &tex));
    }
    // (colour, normal, data, two-channel) for base and layer 2, overlay, extra data
    let (base, l2, overlay, extra): ([usize; 4], [usize; 4], usize, usize) = match name_hash {
        "b217b263" | "1251cae5" | "cd8ceb5e" | "77d8ab27" | "89c8fe07" => ([0, 6, 7, 9], [4, 10, 5, 2], 8, 11),
        "4f0b809b" => ([2, 10, 3, 0], [7, 4, 5, 9], 6, 11),
        _ => return None,
    };
    let layer = |name: &str, s: [usize; 4], uv: &str, weight: &str| {
        json!({"name": name, "colour": tex(s[0]), "normal": tex(s[1]), "data": tex(s[2]), "two_channel": tex(s[3]), "uv_set": uv, "weight": weight})
    };
    let mut out = json!({
        "layers": [
            layer("base", base, "TEXCOORD_0", "1 - COLOR0.a"),
            layer("layer2", l2, "TEXCOORD_1", "COLOR0.a"),
        ],
        "overlay": {"colour_alpha": tex(overlay), "uv_set": "TEXCOORD_2", "weight": "texture alpha (grime/dirt)", "uv_note": "TEXCOORD_2 by analogy with prop_2lyr_batch, where a seam test shows it (the tiling wall dirt maps cannot be checked that way). On walls it is constant (0, 1) on most primitives (uniform dirt), exactly the horizontal position at 1 m per unit (rotated, no residual) on ground and track pieces, and an irregular second mapping on others"},
        "extra_data": tex(extra),
        "vertex_color_channels": {"r": "unverified: a baked shading term, mostly 255 (dark blobs on ground tiles where trees stand, 23 on average on trees, 252 on interior shaders; does not follow sky visibility from its own or neighbouring placed geometry in ray tests, nor nearby batch trees, grass AO or road distance)", "g": "unverified: equal to r on 63 % of ordinary vertices; low on ground shaders and varies widely within one collision surface, so not a surface-type mask (looks painted per vertex)", "b": "tint palette index (entry within the palette row)", "a": "layer 2 weight"},
        "confidence": {"grouping": "high", "weight_channel": "medium", "uv_layer2": "medium", "uv_base": "low", "uv_overlay": "low", "data_channels": "unknown"},
        "note": "Derived from texture-size grouping and vertex/UV statistics, not from shader code. Blend: lerp(base, layer2, COLOR0.a), then the overlay by its alpha (TEXCOORD_2). Normal maps: DirectX green.",
    });
    if name_hash == "89c8fe07" {
        // Unpainted vertices are A = 255 here (every granite rock, plain
        // barriers), and A is painted down on container corner posts, edges
        // and corrugations (more on the weathered `_b` variant): the overlay
        // layer weight is 1 - A, the reverse of the wall shaders.
        out["layers"][0]["weight"] = json!("COLOR0.a");
        out["layers"][1]["weight"] = json!("1 - COLOR0.a");
        out["vertex_color_channels"]["a"] = json!("layer-1 weight as 1 - a: 255 (unpainted) = base layer only");
        out["vertex_color_channels"]["r"] = json!("unverified on props: 0/1 part masks on containers, varies on concrete barriers (lower on upward faces)");
        out["confidence"]["weight_channel"] = json!("medium (direction from where the painting sits, not from shader code)");
        out["note"] = json!("Derived from recovered slot names, UV tests and where the vertex alpha is painted, not from shader code. Blend: lerp(base, layer2, 1 - COLOR0.a), sharpened by the layers' materialb alpha (height-like), then the dirt overlay by its alpha on TEXCOORD_2. Normal maps: DirectX green.");
        out["overlay"].as_object_mut().unwrap().remove("uv_note");
        out["confidence"]["uv_overlay"] = json!("medium");
        out["prop_2lyr"] = json!({
            "slot_names": {"p0": "lyr0diffusetex", "p6": "lyr0normaltex", "p7": "lyr0materialbtex", "p9": "lyr0materialatex", "p4": "lyr1diffusetex", "p10": "lyr1normaltex", "p5": "lyr1materialbtex", "p2": "lyr1materialatex", "p8": "dirtdiffusetex", "p1": "tintpalettetex (embedded)", "p11": "unnamed (greyscale mask, containers only)"},
            "tint": "the tintpalettetex colour is applied per vertex as COLOR_0 (tint_applied in meshes[]; props with a palette such as containers and barriers, not the rocks); multiply it into the base layer (white paint), not the rust or moss overlay, whose textures are already coloured",
            "dirt_uv": "TEXCOORD_2. On barriers and most containers it is a mapping of its own with almost no seams (27 against about 560 for TEXCOORD_0 and _1 on one barrier), and the dirt map is continuous across those seams while it reads as random across the other sets' seams. On every rock and on some containers it is parked at one point, (1, 0) or (0, 1), so the dirt there is one uniform sample (with wrap and bilinear filtering, the average of the four corner texels: alpha about 64 of 255 on the rock dirt map); sampling it as-is matches the vertex data. Not checked against shader code",
            "note": "prop_2lyr_batch (rocks, containers, concrete barriers): same slot layout as the multilayer walls. Layer 0 is the unwrapped base (rock chunk atlas, painted metal) on TEXCOORD_0 (checked: rock UV0 triangles fall inside the atlas chunks, UV1 ones do not); layer 1 is a tiling overlay (moss, rust). materialb alpha looks like a height map in both layers (a height blend would sharpen the COLOR0.a weight). Raw COLOR0 on a container is all 1 and on a large rock R is 0/1, G 1, B about 0 and A varies, so A = 255 means base only and the overlay weight is 1 - A (see layers[].weight).",
        });
    }
    Some(out)
}

/// `layers` plus the material's measured UV mapping (`uv_fit`, from the
/// primitives that use it) and, for the 4-layer ground family, the shader's
/// constants (`params`) and a detail-UV recommendation that follows the
/// measured mapping.
pub fn multilayer(s: &crate::drawable::Shader, entries: &[Value], fit: impl FnOnce() -> Value) -> Option<Value> {
    let mut l = layers(&s.name_hash, entries)?;
    let fit = fit();
    if l["family"] == "four_layer" {
        l["params"] = four_layer_params(&s.constants);
        l["detail_uv"]["recommended"] = four_layer_detail_uv(&fit);
        four_layer_uv(&mut l, &s.constants);
        four_layer_tint(&mut l, &s.constants);
    }
    if !fit.is_null() {
        l["uv_fit"] = fit;
    }
    Some(l)
}

/// Fit each UV set as a plane over the horizontal model position
/// (uv = a*x + b*y + c, RAGE Z-up) across the given primitives. Map section
/// tiles have TEXCOORD_0 = position / 512 m; other models using the same
/// shaders (set pieces, rock formations) carry their own mappings, which
/// this measures. Null when there are no vertices.
pub fn uv_fit<'a>(prims: impl Iterator<Item = &'a crate::gltf::TriPrim>) -> Value {
    let prims: Vec<&crate::gltf::TriPrim> = prims.collect();
    let nsets = prims.iter().map(|p| p.uvs.len()).max().unwrap_or(0);
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let mut out = Vec::new();
    for k in 0..nsets {
        let mut pts: Vec<([f64; 2], [f64; 2])> = Vec::new();
        for p in &prims {
            let Some(uv) = p.uvs.get(k) else { continue };
            for (pos, t) in p.positions.iter().zip(uv) {
                pts.push(([pos[0] as f64, pos[1] as f64], [t[0] as f64, t[1] as f64]));
            }
        }
        if pts.is_empty() {
            continue;
        }
        // centre the positions for conditioning, then normal equations of [x y 1]
        let n = pts.len() as f64;
        let cx = pts.iter().map(|q| q.0[0]).sum::<f64>() / n;
        let cy = pts.iter().map(|q| q.0[1]).sum::<f64>() / n;
        let mut ata = [[0f64; 3]; 3];
        let mut atb = [[0f64; 2]; 3];
        let mut lo = [f64::MAX; 2];
        let mut hi = [f64::MIN; 2];
        for (r, t) in &pts {
            let r = [r[0] - cx, r[1] - cy, 1.0];
            for i in 0..3 {
                for j in 0..3 {
                    ata[i][j] += r[i] * r[j];
                }
                for c in 0..2 {
                    atb[i][c] += r[i] * t[c];
                }
            }
            for c in 0..2 {
                lo[c] = lo[c].min(t[c]);
                hi[c] = hi[c].max(t[c]);
            }
        }
        let span = [hi[0] - lo[0], hi[1] - lo[1]];
        let constant = span[0].max(span[1]) < 1e-4;
        let mut e = json!({"set": format!("TEXCOORD_{k}"), "constant": constant, "span": [r3(span[0]), r3(span[1])]});
        let coef: Option<Vec<[f64; 3]>> = (0..2).map(|c| solve3(ata, [atb[0][c], atb[1][c], atb[2][c]])).collect();
        if let (Some(coef), false) = (coef, constant) {
            let err = pts
                .iter()
                .map(|(r, t)| (0..2).map(|c| (coef[c][0] * (r[0] - cx) + coef[c][1] * (r[1] - cy) + coef[c][2] - t[c]).abs()).fold(0.0, f64::max))
                .fold(0.0, f64::max);
            let mpu: Vec<Value> = coef
                .iter()
                .map(|c| {
                    let g = (c[0] * c[0] + c[1] * c[1]).sqrt();
                    if g > 1e-9 { json!(r3(1.0 / g)) } else { Value::Null }
                })
                .collect();
            e["metres_per_unit"] = json!(mpu);
            e["max_error"] = json!(r3(err));
            e["planar"] = json!(err <= 0.05 * span[0].max(span[1]));
        }
        out.push(e);
    }
    if out.is_empty() { Value::Null } else { Value::Array(out) }
}

fn solve3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut x = [0f64; 3];
    for (i, xi) in x.iter_mut().enumerate() {
        let mut mi = m;
        for r in 0..3 {
            mi[r][i] = b[r];
        }
        *xi = det(mi) / d;
    }
    Some(x)
}

/// The variants of the clothing shader `ped_standard_muv` (name suffixes),
/// from the shader archive's effect list.
const CLOTHING_SHADERS: [&str; 22] = [
    "", "_alpha", "_alpha_blendshape", "_alpha_secmo", "_anisotropy_alpha", "_anisotropy_sheen", "_anisotropy_sheen_secmo", "_blendshape", "_blendshape_cutout", "_cutout", "_cutout_secmo",
    "_diffraction", "_diffraction_secmo", "_emissive", "_emissive_secmo", "_iridescence", "_iridescence_alpha", "_iridescence_alpha_secmo", "_iridescence_secmo", "_retroreflection",
    "_retroreflection_secmo", "_secmo",
];

/// The three shared stitch textures, by the names the game binds them
/// with: colour and coverage, a second map the clothing program that was
/// read does not sample, and the normal map. They are in the game's global
/// graphics texture dictionary (textures archive), not in a texture store.
pub const STITCH_TEXTURES: [&str; 3] = ["ped_stitches_d", "ped_stitches_ma", "ped_stitches_n"];

/// The stitch layer of a clothing material (`ped_standard_muv` and its
/// variants), measured on the material's third UV set. The clothing shader
/// draws seams' stitches from one shared atlas of stitch styles, sampled
/// with that UV set: vertices away from a seam are gathered in one small
/// empty patch of the atlas (on a t-shirt about 24 x 2 texels, right beside
/// the stitch line the garment uses), the others are laid over a stitch
/// line with V running along the seam. None for other
/// shaders or a material without a third UV set.
pub fn stitches(name_hash: &str, uv2: impl Iterator<Item = [f32; 2]>) -> Option<Value> {
    let hash = u32::from_str_radix(name_hash.trim(), 16).ok()?;
    if !CLOTHING_SHADERS.iter().any(|v| crate::meta7a::joaat(&format!("ped_standard_muv{v}")) == hash) {
        return None;
    }
    let uv: Vec<[f32; 2]> = uv2.filter(|x| x[0].is_finite() && x[1].is_finite()).collect();
    if uv.is_empty() {
        return None;
    }
    // the rest point: the most common UV, to 0.01; vertices within 0.02 of it count as resting
    let key = |x: &[f32; 2]| ((x[0] * 100.0).round() as i64, (x[1] * 100.0).round() as i64);
    let mut counts: std::collections::HashMap<(i64, i64), u64> = std::collections::HashMap::new();
    for x in &uv {
        *counts.entry(key(x)).or_default() += 1;
    }
    let (&rest, _) = counts.iter().max_by_key(|(k, n)| (**n, std::cmp::Reverse(**k)))?;
    let at_rest = uv.iter().filter(|x| (key(x).0 - rest.0).abs() <= 2 && (key(x).1 - rest.1).abs() <= 2).count() as u64;
    let n = uv.len() as u64;
    Some(json!({
        "uv_set": "TEXCOORD_2",
        "vertices": n,
        "rest_uv": [rest.0 as f64 / 100.0, rest.1 as f64 / 100.0],
        "vertices_at_rest": at_rest,
        "vertices_on_seams": n - at_rest,
        "has_stitches": n > at_rest,
        "textures": {
            "colour": STITCH_TEXTURES[0], "normal": STITCH_TEXTURES[2], "third": STITCH_TEXTURES[1],
            "where": "the game's global graphics texture dictionary in the textures archive (the dictionary that also holds the ped tint ramps); exported with it as <name>.png",
            "size": [2048, 128], "layout": "an atlas of stitch styles side by side: vertical lines that repeat along V, and a few horizontal and looped ones; most of it is empty",
            "colour_channels": "A = coverage (0 where there is no stitch), G = the grey value (255 off a stitch, darker on the thread), R and B = masks for the second and third stitch tint (0 on most styles)",
            "normal_channels": "a tangent normal map (R, G; flat = 127, 127)",
            "third_channels": "G about 243 everywhere, A between 100 and 255 on stitches; not sampled by the program that was read",
        },
        "weight": "stitch amount (the material library preset's stitchesblendamount) x the colour texture's alpha",
        "colour": "each of the preset's three stitchtint colours is a three-stop gradient indexed by the grey value g (below 0.5: first to second stop by 2g; above: second to third by 2(g - 0.5)); S = mix(g, tint0, tint0.alpha), then S = mix(S, tint1, tint1.alpha x R) and S = mix(S, tint2, tint2.alpha x B); colour = mix(colour, S, weight). The normal is blended towards the stitch normal map by the same weight",
        "source": "textures: the game's code binds the shader's two stitch inputs to these names (high confidence), and their channels fit the shader's use of them. Weight and colour: one pixel program of ped_standard_muv, read twice. UV set: of a garment's four UV sets only this one runs along seams (V spans tens to hundreds of repeats), and a garment without stitches has it collapsed to one empty point of the atlas; on a t-shirt the vertices away from seams are gathered in a small empty patch beside a stitch line (medium confidence: six garments, checked by eye; V's direction was not checked). rest_uv is the most common UV to 0.01 and vertices_at_rest those within 0.02 of it: a rough split, not a per-pixel test against the atlas",
        "not_decoded": "the stitch amount and tints of this garment (they are in its material library preset, which a lone model does not name), the default amount, and the use of the third texture. Not applied to the exported textures",
        "changed": "until 2026-11-24 this block named a 128 x 128 mask of the graphics texture store (19a5e59eedc47faa) and counted vertices on its three columns; that was the wrong texture",
    }))
}

/// Where a 4-layer material's layer weights come from, measured on its own
/// vertices: the shader mixes the lookup texture's weights with the second
/// vertex colour set (`_COLOR1`) by that set's alpha, so alpha 1 means the
/// vertex colours alone decide and alpha 0 the lookup texture alone. On map
/// section tiles the alpha is 1 on most vertices (14 tiles, about 150,000
/// vertices: 98 % on the most used variant, 58 to 100 % on the others).
/// Does nothing for other families or a material without that stream.
pub fn four_layer_vertex_weights(l: &mut Value, colours1: impl Iterator<Item = [u8; 4]>) {
    if l["family"] != "four_layer" {
        return;
    }
    let (mut n, mut on, mut off, mut alpha, mut over) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for c in colours1 {
        n += 1;
        on += (c[3] == 255) as u64;
        off += (c[3] == 0) as u64;
        alpha += c[3] as u64;
        over += (c[0] as u32 + c[1] as u32 + c[2] as u32 > 260) as u64;
    }
    if n == 0 {
        return;
    }
    let share = |x: u64| (x as f64 / n as f64 * 1000.0).round() / 1000.0;
    let source = if on == n {
        "vertex colours"
    } else if off == n {
        "lookup texture"
    } else {
        "mixed: per vertex"
    };
    l["weights"]["source"] = json!(source);
    l["weights"]["vertex"] = json!({
        "attribute": "_COLOR1", "vertices": n, "alpha_1": share(on), "alpha_0": share(off), "mean_alpha": (alpha as f64 / n as f64 / 255.0 * 1000.0).round() / 1000.0,
        "rgb_sum_over_1": share(over),
        "use": "weights.rgb = mix(lookup.rgb, _COLOR1.rgb / 255, _COLOR1.a / 255) per vertex; r, g, b weight layers 0, 1, 2 and the remainder layer 3",
        "note": "alpha_1 / alpha_0: share of this material's vertices where the vertex colours alone / the lookup texture alone decide. rgb_sum_over_1: share of vertices whose r + g + b exceeds 1, where the stream is probably not a plain weight triple",
    });
}

/// Each layer's UV of a 4-layer material from the material's own constants,
/// as the shader code applies them (read in two deferred pixel programs,
/// `standard_uber_4lyr` and `…_dpm_dm_hbb`): `724bed95[k] == 1` gives layer
/// k the first UV set times `4344d2cc.xy` plus `.zw`, anything else the
/// second set times `9ecaaa1e.xy` plus `.zw`. Replaces the assumed values
/// in `layers` and the recommendation; nothing changes when a constant is
/// missing.
fn four_layer_uv(l: &mut Value, consts: &[crate::drawable::ShaderConst]) {
    let get = |h: &str| consts.iter().find(|c| c.hash == h).and_then(|c| c.components()).filter(|v| v.len() == 4 && v.iter().all(|x| x.is_finite())).map(<[f32]>::to_vec);
    let (Some(first), Some(second), Some(pick)) = (get("4344d2cc"), get("9ecaaa1e"), get("724bed95")) else { return };
    let alt = l["detail_uv"]["alt_uv_set"].as_str().unwrap_or("the second UV set").to_string();
    for (k, layer) in l["layers"].as_array_mut().into_iter().flatten().enumerate().take(4) {
        let (set, t) = if pick[k] == 1.0 { ("TEXCOORD_0".to_string(), &first) } else { (alt.clone(), &second) };
        layer["uv_set"] = json!(set);
        layer["uv_scale"] = if t[0] == t[1] { json!(t[0]) } else { json!([t[0], t[1]]) };
        layer["uv_offset"] = json!([t[2], t[3]]);
        layer["uv_source"] = json!("material constants");
    }
    let l0 = l["layers"][0].clone();
    l["detail_uv"]["recommended"] = json!({"uv": l0["uv_set"], "scale": l0["uv_scale"], "offset": l0["uv_offset"], "per_layer": true,
        "why": "the material's own constants, as the shader applies them: see uv_set, uv_scale and uv_offset of each layer (a layer can use the other UV set)"});
    l["confidence"]["detail_uv_scale"] = json!("high (material constants and shader code)");
}

/// The tint of a 4-layer ground material as the shader computes it, with
/// this material's switches resolved: `tint_formula`. Read from the base
/// pixel program of `standard_uber_4lyr`; the most used variant's program
/// has the same code shape. Does nothing when the material lacks the tint
/// constants.
fn four_layer_tint(l: &mut Value, consts: &[crate::drawable::ShaderConst]) {
    let get = |h: &str| consts.iter().find(|c| c.hash == h).and_then(|c| c.components()).filter(|v| v.iter().all(|x| x.is_finite())).map(<[f32]>::to_vec);
    let four = |h: &str| get(h).filter(|v| v.len() == 4);
    let one = |h: &str| get(h).and_then(|v| v.first().copied());
    // per layer: texture tint mode, texture tint intensity, blend amount, vertex tint mode, vertex tint intensity, mask scale
    let (Some(mode), Some(ti), Some(blend), Some(vmode), Some(vi), Some(ms)) = (four("8e3fb0aa"), four("db26e4de"), four("1caef69a"), four("b1c84936"), four("81bc5bca"), four("896d1e47")) else { return };
    // material switches: vertex tint on, vertex tint second form, blend amount scaled by the mask
    let (Some(von), Some(vform), Some(masked)) = (one("3fb2b8f7"), one("834fb13b"), one("abe2c64d")) else { return };
    let layers: Vec<Value> = (0..4)
        .map(|k| {
            let invert = blend[k] > 0.0 && mode[k] == 0.0;
            let vertex = if von == 0.0 {
                "off"
            } else if vform != 0.0 {
                "second form"
            } else if vmode[k] == 1.0 {
                "multiply"
            } else {
                "luminance"
            };
            json!({
                "mask": if invert { "1 - a" } else { "a" }, "mask_scale": ms[k],
                "vertex_tint": if ms[k] == 0.0 || vi[k] == 0.0 { "off" } else { vertex }, "vertex_tint_intensity": vi[k],
                "texture_tint": if blend[k] == 0.0 || ti[k] == 0.0 { "off" } else if mode[k] == 1.0 { "multiply" } else { "luminance" },
                "texture_tint_intensity": ti[k], "texture_tint_amount": if masked != 0.0 { json!(format!("saturate(a x {})", blend[k])) } else { json!(blend[k]) },
            })
        })
        .collect();
    l["tint_formula"] = json!({
        "status": "not applied by the exporter",
        "inputs": {
            "D": "the layer's colour texture (linear)", "a": "alpha of the layer's data map",
            "C": "the first vertex colour set (_COLOR0) rgb, decoded from sRGB to linear", "C.a": "alpha of that set", "T": "the section's tint texture (macro.tint) rgb",
            "lum(x)": "0.2125 x.r + 0.7154 x.g + 0.0721 x.b",
        },
        "steps": [
            "m = saturate(mask x mask_scale)                       (mask: a, or 1 - a where the layer's blend amount is above 0 and its texture tint mode is luminance)",
            "V = multiply: saturate(C x intensity x D);  luminance: C x intensity x lum(D)",
            "Dv = mix(D, V, m)                                     (the vertex-tinted colour)",
            "D1 = Dv when the vertex tint is on, else D",
            "W = multiply: saturate(T x intensity x D1);  luminance: T x intensity x lum(D1)",
            "D2 = mix(D1, W, amount)                               (amount: the layer's blend amount, times a when the material scales it by the mask)",
            "colour = mix(D2, Dv, C.a)                             (the vertex colour's alpha brings the vertex-tinted colour back over the texture tint)"
        ],
        "second_form": "where vertex_tint is `second form`: Dv = D + mix(C x intensity x lum(D), saturate(D x C), third vertex colour set's blue) - D, scaled by m as above. One of 203 surveyed materials uses it",
        "switches": {"vertex tint on (3fb2b8f7)": von, "vertex tint second form (834fb13b)": vform, "blend amount scaled by the mask (abe2c64d)": masked},
        "per_layer": layers,
        "source": "shader code: the base pixel program of standard_uber_4lyr, read once; the most used variant's program has the same code shape. On 203 ground materials of 14 tiles, 189 have the switches 1, 0, 1. Medium confidence; not compared with a rendered image",
        "not_read": "the switch 7a090c2a beside these, and the tint's effect on the normal (tintbump) and roughness (tintroughnessbias)",
    });
}

/// Detail-layer UV for a 4-layer material from its measured mapping:
/// section tiles (TEXCOORD_0 planar at about 512 m per unit, 430-620 allowing for quantisation on small patches) repeat the layers
/// 64 times per TEXCOORD_0 unit (every 8 m); other models use the first
/// planar UV set at a detail scale (0.5-20 m per unit), else position / 8 m.
fn four_layer_detail_uv(fit: &Value) -> Value {
    let sets = fit.as_array().cloned().unwrap_or_default();
    let planar = |e: &Value| e["planar"] == true && e["constant"] == false;
    let mpu = |e: &Value| e["metres_per_unit"].as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).fold(0.0, f64::max)).unwrap_or(0.0);
    if sets.first().is_some_and(|e| planar(e) && (430.0..=620.0).contains(&mpu(e))) {
        return json!({"uv": "TEXCOORD_0", "scale": 64.0, "section_tile": true, "why": "TEXCOORD_0 is section position / 512 m (map section tile)"});
    }
    if let Some(e) = sets.iter().skip(1).find(|e| planar(e) && (0.5..=20.0).contains(&mpu(e))) {
        return json!({"uv": e["set"], "scale": 1.0, "section_tile": false, "metres_per_unit": e["metres_per_unit"], "why": "not a section tile; this set is a planar detail mapping"});
    }
    json!({"uv": "position", "scale": 0.125, "section_tile": false, "why": "not a section tile and no planar detail UV: tile from position, one repeat per 8 m"})
}

/// Constants of a 4-layer ground material: the float4 constants are per
/// layer (component k belongs to layer k, as the shader code uses them),
/// the others belong to the material. Keys are names where the name is
/// known, with the hash.
pub fn four_layer_params(consts: &[crate::drawable::ShaderConst]) -> Value {
    let key = |h: &str| match crate::names::param(h) {
        Some(n) => format!("{n} ({h})"),
        None => h.to_string(),
    };
    let mut layers: Vec<serde_json::Map<String, Value>> = vec![serde_json::Map::new(); 4];
    let mut other = serde_json::Map::new();
    for c in consts {
        let Some(v) = c.components().filter(|v| v.iter().all(|x| x.is_finite())) else { continue };
        if v.len() == 4 && !UV_TRANSFORMS.contains(&c.hash.as_str()) {
            for (k, l) in layers.iter_mut().enumerate() {
                l.insert(key(&c.hash), json!(v[k]));
            }
        } else {
            other.insert(key(&c.hash), c.json().unwrap_or(Value::Null));
        }
    }
    json!({
        "per_layer": layers,
        "material": other,
        "uv_transforms": {"4344d2cc": "scale (xy) and offset (zw) of the first UV set", "9ecaaa1e": "scale and offset of the second UV set", "724bed95": "listed per layer: 1 = the layer uses the first UV set, else the second", "source": "shader code (one deferred pixel program of standard_uber_4lyr)"},
        "note": "Constant values from the material's constant buffers (see drawable.shaders[].constants: buffer, byte offset, component count). float4 constants are split per layer; what most of them do is not decoded. Values exported before 2026-11-24 were read from wrong offsets and should be discarded.",
    })
}

/// float4 constants of the four-layer shaders that are a UV scale and
/// offset, not per-layer values.
const UV_TRANSFORMS: [&str; 2] = ["4344d2cc", "9ecaaa1e"];

/// The 4-layer ground shaders: name hash, the variant's suffix after
/// `standard_uber_4lyr`, and whether the variant's own main pixel program
/// was compared with the one read in full (the nine variants found on 14
/// surveyed map tiles). All 18 have the same 17 texture slots in the
/// shader archive's tables.
const FOUR_LAYER: [(&str, &str, bool); 18] = [
    ("88a32075", "", false),
    ("388f8fb6", "_dfm_dm_hbb", false),
    ("f8e1f225", "_dfm_dm_hbb_uv3", false),
    ("61fad45c", "_dpm_dfm_dm_hbb", true),
    ("40497278", "_dpm_dfm_dm_hbb_uv3", true),
    ("61e09df7", "_dpm_dm_hbb", true),
    ("0ad54cb9", "_dpm_dm_hbb_uv3", true),
    ("d5bdc029", "_hbo_dm_hbb", false),
    ("6f968246", "_mud_dm_hbb", false),
    ("1ae17fd6", "_mud_dm_hbb_uv3", false),
    ("5a71e7e1", "_pxm", false),
    ("4a579bbf", "_pxm_hbb", true),
    ("b21f1ba5", "_pxm_hbb_fixuv1", true),
    ("27e5c49d", "_pxm_hbb_uv3", true),
    ("10098c2c", "_pxm_uv3", false),
    ("360e396b", "_spx_dm_hbb", true),
    ("76fcff28", "_spx_dm_hbb_uv3", true),
    ("7543cefc", "_uv3", false),
];

/// What is known about one variant of the 4-layer ground shader, by the
/// parts of its name: what its program was seen to do differently, and
/// what was not read.
fn four_layer_variant(suffix: &str, compared: bool) -> Value {
    let has = |part: &str| suffix.split('_').any(|p| p == part);
    let mut differences = Vec::new();
    if has("uv3") {
        differences.push("uv3: the pixel program takes a second tangent frame from the mesh (one more interpolated vector, or two); the layers' UV inputs are the same as without uv3. The mesh's third UV set is about 60 to 100 m per unit on these materials; what it is used for was not read");
    }
    if has("dfm") {
        differences.push("dfm: reads three more textures that the engine supplies (not material slots), 9 more samples than the same variant without dfm; their use was not traced");
    }
    if has("pxm") {
        differences.push("pxm: 8 more texture samples with explicit gradients than the dpm variant; not traced");
    }
    if has("spx") {
        differences.push("spx: 4 more texture samples than the dpm variant; not traced. Its mesh program is the same as the pxm variant's");
    }
    if has("fixuv1") {
        differences.push("fixuv1: the second UV set is not used for the layers");
    }
    for part in ["mud", "hbo"] {
        if has(part) {
            differences.push(if part == "mud" { "mud: not examined" } else { "hbo: not examined" });
        }
    }
    let height_blend = if !has("hbb") {
        "this variant has no height-blend constants (03754af2, b2339a6c, blendsoftness), so its layers probably blend by weight alone; its program was not read"
    } else if has("dpm") {
        "as described in height_blend: on per layer by the switch f24b51f2"
    } else if compared {
        "the program has the same softness code as described in height_blend, but this variant has no switch constant f24b51f2: whether the blend is then always on was not read"
    } else {
        "the variant has the height-blend constants; its program was not read"
    };
    json!({
        "shader": format!("standard_uber_4lyr{suffix}"),
        "program": if compared {
            "its main pixel program was compared with the one read in full (standard_uber_4lyr_dpm_dm_hbb): the same layer UV selection, tint code and height-blend code are present; it was not read line by line"
        } else {
            "only its tables were compared (the same 17 texture slots as the variants that were read); no material using it was examined and its program was not read, so the description is by analogy"
        },
        "height_blend": height_blend,
        "differences": differences,
        "confidence": if compared { "medium" } else { "low" },
    })
}

/// The 17-slot 4-layer ground shaders (`standard_uber_4lyr` and its 17
/// variants, see `FOUR_LAYER`), same slot layout in all. Slot names are recovered parameter names
/// (`diffusetexture_layer0` …); the unnamed data maps were paired with
/// their layer by co-occurrence (each data-map hash goes with exactly one
/// layer's diffuse over 200 materials). Weights: the section-wide
/// `lookuptexture`, sampled with TEXCOORD_0 (= section position / 512 m);
/// R, G, B weight layers 0, 1, 2 (seam-continuity test across material
/// borders: the slot a channel drives agrees across the border 66 / 74 /
/// 92 % of the time where that channel dominates), the rest goes to layer 3.
///
/// Read from the shader code since (one deferred pixel program of
/// `standard_uber_4lyr`, which the other variants also use for their
/// alpha-clip pass; the variants' own main programs were not read): the
/// channel order and the remainder rule above, the pairing of each data map
/// with its layer, that the second vertex colour set can replace the lookup
/// weights, that a layer is skipped when its weight is 0, and that the
/// layers' results are added weighted.
fn four_layer(suffix: &str, compared: bool, tex: &dyn Fn(usize) -> Value) -> Value {
    // (colour, normal, data) per layer
    const SLOTS: [[usize; 3]; 4] = [[14, 15, 4], [12, 10, 0], [9, 11, 5], [13, 7, 1]];
    let weight = ["lookup.r", "lookup.g", "lookup.b", "1 - (lookup.r + lookup.g + lookup.b)"];
    let layers: Vec<Value> = SLOTS
        .iter()
        .enumerate()
        .map(|(i, s)| json!({"name": format!("layer{i}"), "colour": tex(s[0]), "normal": tex(s[1]), "data": tex(s[2]), "uv_set": "TEXCOORD_0", "uv_scale": 128.0, "uv_offset": [0.0, 0.0], "uv_source": "assumed (the usual values on map section tiles)", "weight": weight[i]}))
        .collect();
    // every variant's pixel program picks a layer's UV the same way; which
    // mesh stream the second input is comes from UV density, not code
    let alt_uv = if suffix.ends_with("_fixuv1") { Value::Null } else { json!("TEXCOORD_1") };
    json!({
        "family": "four_layer",
        "variant": four_layer_variant(suffix, compared),
        "layers": layers,
        "weights": {"texture": tex(2), "uv_set": "TEXCOORD_0 (on section tiles = section position / 512 m; on other models the model's own UV0, see uv_fit)", "channels": {"r": "layer0", "g": "layer1", "b": "layer2", "remainder": "layer3"}, "note": "R+G+B <= 1 (BC7 rounding up to 258/255: clamp); the remainder weights layer 3",
            "vertex_override": "weights = mix(lookup.rgb, COLOR_1.rgb, COLOR_1.a) per channel, and the remainder = saturate(1 - r - g - b) of each side mixed the same way: where the second vertex colour set has alpha 0 (the usual case) the lookup texture alone decides. From the shader code; COLOR_1 is the exported attribute `_COLOR1` (its r + g + b stays at or below 1 on map tiles, as weights do). See weights.source and weights.vertex for this material's own vertices.",
            "result": "each layer's shaded result is multiplied by its weight and the four are added; a layer whose weight is 0 is not sampled"},
        "macro": {"uv_set": "TEXCOORD_0", "tint": tex(16), "tint_normal": tex(8), "grass_mask": tex(3), "note": "section-wide maps (one 512 m section per texture): colour variation, large-scale normal, grass placement mask"},
        "detail_uv": {"default": "each layer's uv_set, uv_scale and uv_offset come from the material's constants (4344d2cc = scale and offset of the first UV set, 9ecaaa1e of the second, 724bed95 = which set each layer uses); on most map section tiles that is TEXCOORD_0 * 128 (world-planar, one repeat per 4 m; 64 was assumed until 2026-11-24). Without the constants: `recommended` follows this material's measured mapping (`uv_fit`): set pieces and rock formations using these shaders have their own UV0 (about 35 m per unit on the street-trash grounds), usually with a planar TEXCOORD_1 at 3 m per unit", "alt_uv_set": alt_uv, "note": "The pixel program picks each layer's UV by 724bed95: 1 = the first set, 2 = the second (the same code in all nine variants whose programs were compared, `uv3` ones included). The second set is TEXCOORD_1 by its density: about 3 to 6 m per unit on every variant over 14 tiles, like TEXCOORD_0 x 128 (4 m), while TEXCOORD_2 is 40 to 100 m per unit; medium confidence, the mesh programs that feed the inputs could not be read. Until 2026-11-24 the `uv3` variant 27e5c49d was given TEXCOORD_2 here. 'fixuv1' (b21f1ba5): the second set is taken as unused, so that variant tiles from position. The shader also has a branch that samples a layer again at another scale (constant 26cc8f94); not decoded"},
        "vertex_color_channels": {"_COLOR1": "rgb = layer weights for layers 0, 1, 2 (the remainder is layer 3), a = how much they replace the lookup texture's weights", "_COLOR0": "rgb = a vertex tint (stored sRGB), a = its blend amount; from the shader code, not applied by the exporter"},
        "texcoord_2": "a copy of TEXCOORD_0 on most 61e09df7 primitives, constant elsewhere",
        "height_blend": {
            "source": "shader code: the main deferred pixel program of standard_uber_4lyr_dpm_dm_hbb, read once; the constants' values on real materials fit (medium confidence)",
            "height": "h[k] = B channel of layer k's data map, sampled with the layer's UV",
            "steps": [
                "a[k] = max(0.01, (h[k] - 03754af2[k]) * b2339a6c[k])        (per-layer height offset and scale; usually 0 and 1)",
                "b[k] = saturate(a[k] * w[k] * fade)                            (w = the layer weights; fade is a distance term, 1 nearby)",
                "c[k] = b[k] / max(b[0], b[1], b[2], b[3], extra)               (extra: a fifth term that was not traced; 0 when absent)",
                "p[k] = 20 - 19 * saturate((blendsoftness[k] - 0.2) * 1.754)    (exponent: 20 at softness 0.2 or less, 1 at 0.77 or more)",
                "d[k] = pow(c[k], p[k]) - 0.001",
                "W[k] = saturate(d[k] / (d[0] + d[1] + d[2] + d[3] + d_extra))",
                "final[k] = f24b51f2[k] * W[k] + (1 - sum over j of f24b51f2[j] * W[j]) * w[k]"
            ],
            "switch": "f24b51f2[k] is 0 or 1 per layer: with 0 on all four layers (the usual case: 49 of 57 materials on 14 tiles) the weights stay as they are and no height blend happens",
            "constants": "per layer, in params.per_layer of this material: 03754af2, b2339a6c, blendsoftness, f24b51f2",
            "not_read": "the fade term and the fifth term; see `variant` for what holds on this material's shader"
        },
        "data_map_channels": {"a": "tint mask of the layer: how much of the tint colour replaces the layer colour (the shader can use 1 - a instead, by a material constant)", "b": "height, for the height-based blend (see height_blend); also overlay-blended with the detail array's blue channel into a scalar that is scaled and offset per layer", "g": "a scalar that is accumulated per layer beside the colour and changed by the detail array and a grazing-angle term: roughness-like, not confirmed", "r": "not read by the programs that were read", "source": "shader code"},
        "detail_array": {"texture": tex(6), "use": "a texture array sampled once per layer after the layer's colour; the slice is a per-layer constant, red and green are a detail normal (x2 - 1), blue (around 0.5) brightens or darkens the layer colour and feeds the height and roughness-like scalars", "source": "shader code (standard_uber_4lyr); medium confidence"},
        "tint": "not applied by the exporter. From the shader code, per layer: the layer colour is mixed towards a tinted colour by the data map's alpha (the tint mask); the tinted colour combines the vertex tint (_COLOR0 rgb, scaled by verttintintensity[k]) and the section's tint texture (macro.tint, scaled by tintintensity[k]) with the layer colour's luminance (weights 0.2125, 0.7154, 0.0721), blended by tintblendamount[k]. Several branches chosen by constants were not resolved, so there is no single formula to apply yet",
        "confidence": {"slots": "high (recovered names)", "data_map_pairing": "high (shader code: each data map is sampled with its layer's colour and normal map)", "weights_uv": "high", "channel_rgb_to_layer012": "high (shader code)", "layer3_remainder": "high (shader code)", "detail_uv_scale": "low"},
        "note": "Slots, pairing, channel order and the remainder rule are confirmed from shader code (one deferred pixel program); the UV scales and the height blend are still from recovered names and UV/seam statistics. Normal maps: DirectX green.",
    })
}

#[cfg(test)]
mod paint_tests {
    use super::*;

    #[test]
    fn family_names_match_their_hashes() {
        for (h, name, class, _) in FAMILIES {
            assert_eq!(format!("{:08x}", crate::meta7a::joaat(name)), *h, "{name}");
            // emissive exactly when the shader is one of the emissive ones
            assert_eq!(*class == "emissive", name.contains("emissive"), "{name} is labelled {class}");
        }
        let mut hashes: Vec<_> = FAMILIES.iter().map(|f| f.0).collect();
        hashes.sort();
        hashes.dedup();
        assert_eq!(hashes.len(), FAMILIES.len());
    }

    #[test]
    fn relabelled_families() {
        let alpha = [0, 0, 0, 0, 2, 0, 0, 0];
        for (h, class, shader) in [
            ("d9ec330c", "alpha", "standard_alpha_inst"),
            ("5b775918", "alpha", "standard_anisotropy_dirt_alpha"),
            ("e3ba3129", "alpha", "standard_alpha"),
            ("60643f2e", "emissive", "signs_emissive_inst"),
            ("3a09c775", "emissive", "standard_emissive_layer_pxm_inst"),
            ("db8b21bd", "emissive", "standard_emissive_layer_alpha"),
        ] {
            let c = classify(h, &alpha);
            assert_eq!((c["class"].as_str(), c["shader"].as_str(), c["alpha_mode"].as_str()), (Some(class), Some(shader), Some("BLEND")), "{h}");
            assert_eq!(gltf_fields(&c, true)["emissiveFactor"].is_null(), class != "emissive", "{h}");
        }
    }

    #[test]
    fn paint_hex() {
        assert_eq!(parse_paint("ffffff"), Some([1.0, 1.0, 1.0]));
        assert_eq!(parse_paint("#ff000000").map(|c| c[0]), Some(0.0));
        let c = parse_paint("0x00808080").unwrap();
        assert!((c[0] - 0.2158).abs() < 1e-3);
        assert_eq!(parse_paint("fff"), None);
        assert_eq!(parse_paint("zzzzzz"), None);
    }

    #[test]
    fn four_layer_variants() {
        let entries: Vec<Value> = vec![json!({"param": 14, "png": "t14.png", "status": "ok"})];
        // all 18 variants are described, with the same slots; the hashes are the names' hashes
        for (hash, suffix, _) in FOUR_LAYER {
            assert_eq!(format!("{:08x}", crate::meta7a::joaat(&format!("standard_uber_4lyr{suffix}"))), hash);
            let l = layers(hash, &entries).unwrap();
            assert_eq!((l["family"].as_str(), l["layers"][0]["colour"]["png"].as_str()), (Some("four_layer"), Some("t14.png")));
            assert_eq!(classify(hash, &[0; 8])["class"], "terrain_multilayer");
        }
        // a variant found on tiles: compared, uv3 keeps the second UV set, dfm is listed
        let v = &layers("40497278", &entries).unwrap();
        assert_eq!((v["variant"]["confidence"].as_str(), v["variant"]["differences"].as_array().map(Vec::len), v["detail_uv"]["alt_uv_set"].as_str()), (Some("medium"), Some(2), Some("TEXCOORD_1")));
        assert!(v["variant"]["height_blend"].as_str().unwrap().starts_with("as described"));
        // no switch constant outside the dpm variants; no height blend without hbb; unexamined variants say so
        assert!(layers("4a579bbf", &entries).unwrap()["variant"]["height_blend"].as_str().unwrap().contains("no switch constant"));
        let base = &layers("88a32075", &entries).unwrap()["variant"];
        assert!(base["height_blend"].as_str().unwrap().contains("no height-blend constants") && base["program"].as_str().unwrap().starts_with("only its tables"));
        assert_eq!((base["confidence"].as_str(), base["shader"].as_str()), (Some("low"), Some("standard_uber_4lyr")));
        assert_eq!(layers("6f968246", &entries).unwrap()["variant"]["differences"][0], "mud: not examined");
    }

    #[test]
    fn four_layer_slots() {
        let entries: Vec<Value> = [14usize, 15, 4, 2, 16].iter().map(|&p| json!({"param": p, "png": format!("t{p}.png"), "status": "ok"})).collect();
        let l = layers("61e09df7", &entries).unwrap();
        assert_eq!(l["layers"][0]["colour"]["png"], "t14.png");
        assert_eq!(l["layers"][0]["normal"]["png"], "t15.png");
        assert_eq!(l["layers"][0]["data"]["png"], "t4.png");
        assert_eq!(l["layers"][3]["colour"]["param"], 13);
        assert_eq!(l["layers"][3]["colour"]["status"], "no texture bound");
        assert_eq!(l["weights"]["texture"]["png"], "t2.png");
        // stitches: only clothing shaders with a third UV set; vertices away from the rest point are on seams
        let muv = format!("{:08x}", crate::meta7a::joaat("ped_standard_muv_secmo"));
        let x = stitches(&muv, [[2.33f32, 0.53], [2.334, 0.531], [0.5, -3.0], [-0.5, 7.0], [2.33, 0.53], [f32::NAN, 0.0]].into_iter()).unwrap();
        assert_eq!((x["vertices"].as_u64(), x["vertices_at_rest"].as_u64(), x["vertices_on_seams"].as_u64(), x["has_stitches"].as_bool()), (Some(5), Some(3), Some(2), Some(true)));
        assert_eq!((x["rest_uv"][0].as_f64(), x["rest_uv"][1].as_f64(), x["textures"]["colour"].as_str()), (Some(2.33), Some(0.53), Some("ped_stitches_d")));
        let x = stitches(&muv, [[0.72f32, 0.54]; 3].into_iter()).unwrap();
        assert_eq!((x["has_stitches"].as_bool(), x["vertices_on_seams"].as_u64()), (Some(false), Some(0)));
        assert!(stitches(&muv, std::iter::empty()).is_none());
        assert!(stitches(&format!("{:08x}", crate::meta7a::joaat("standard")), [[0.5f32, 0.0]].into_iter()).is_none());
        assert_eq!((l["height_blend"]["steps"].as_array().map(Vec::len), l["detail_array"]["texture"]["param"].as_u64()), (Some(7), Some(6)));
        assert!(l["data_map_channels"]["b"].as_str().unwrap().starts_with("height") && l["tint"].as_str().unwrap().starts_with("not applied"));
        // where the weights come from, measured on the material's vertices
        let mut v = l.clone();
        four_layer_vertex_weights(&mut v, [[10u8, 20, 30, 255], [0, 0, 0, 255]].into_iter());
        assert_eq!((v["weights"]["source"].as_str(), v["weights"]["vertex"]["alpha_1"].as_f64(), v["weights"]["vertex"]["vertices"].as_u64()), (Some("vertex colours"), Some(1.0), Some(2)));
        let mut v = l.clone();
        four_layer_vertex_weights(&mut v, [[0u8, 0, 0, 0], [200, 200, 0, 255], [0, 0, 0, 128], [0, 0, 0, 0]].into_iter());
        assert_eq!((v["weights"]["source"].as_str(), v["weights"]["vertex"]["alpha_0"].as_f64(), v["weights"]["vertex"]["rgb_sum_over_1"].as_f64()), (Some("mixed: per vertex"), Some(0.5), Some(0.25)));
        let mut v = l.clone();
        four_layer_vertex_weights(&mut v, std::iter::empty());
        assert!(v["weights"]["source"].is_null());
        // layer UVs from the material's constants: layer 1 uses the second set
        let c4 = |hash: &str, v: [f32; 4]| crate::drawable::ShaderConst { hash: hash.into(), desc: String::new(), offset: 0, values: Some(v), buffer: 7, count: 4, ty: None };
        let sh = crate::drawable::Shader { index: 0, record_off: 0, name_hash: "61e09df7".into(), param_hashes: vec![], textures: vec![], flags: vec![],
            constants: vec![c4("4344d2cc", [128.0, 128.0, 0.0, 0.0]), c4("9ecaaa1e", [1.0, 2.0, 0.5, 0.0]), c4("724bed95", [1.0, 2.0, 1.0, 1.0])] };
        let m = multilayer(&sh, &entries, || Value::Null).unwrap();
        assert_eq!((m["layers"][0]["uv_set"].as_str(), m["layers"][0]["uv_scale"].as_f64(), m["layers"][0]["uv_source"].as_str()), (Some("TEXCOORD_0"), Some(128.0), Some("material constants")));
        assert_eq!((m["layers"][1]["uv_set"].as_str(), m["layers"][1]["uv_scale"][1].as_f64(), m["layers"][1]["uv_offset"][0].as_f64()), (Some("TEXCOORD_1"), Some(2.0), Some(0.5)));
        assert_eq!(m["detail_uv"]["recommended"]["scale"].as_f64(), Some(128.0));
        // without the constants the assumed values stay and say so
        assert_eq!((l["layers"][0]["uv_scale"].as_f64(), l["layers"][0]["uv_source"].as_str().map(|s| s.starts_with("assumed"))), (Some(128.0), Some(true)));
        // constants: float4s per layer, the UV transforms and scalars with the material
        let k = |hash: &str, count: u8, v: [f32; 4]| crate::drawable::ShaderConst { hash: hash.into(), desc: String::new(), offset: 0, values: Some(v), buffer: 7, count, ty: None };
        let p = four_layer_params(&[k("4344d2cc", 4, [128.0, 128.0, 0.0, 0.0]), k("0f0f0f0f", 4, [1.0, 0.7, 0.2, 1.0]), k("0e0e0e0e", 1, [3.0, 0.0, 0.0, 0.0])]);
        assert_eq!((p["per_layer"][1]["0f0f0f0f"].as_f64().map(|x| (x * 10.0).round()), p["per_layer"][0].as_object().map(|o| o.len())), (Some(7.0), Some(1)));
        assert_eq!((p["material"]["4344d2cc"][0].as_f64(), p["material"]["0e0e0e0e"].as_f64()), (Some(128.0), Some(3.0)));
        assert_eq!((l["weights"]["channels"]["remainder"].as_str(), l["confidence"]["layer3_remainder"].as_str()), (Some("layer3"), Some("high (shader code)")));
        assert!(l["weights"]["vertex_override"].as_str().unwrap().contains("COLOR_1") && l["data_map_channels"]["a"].as_str().unwrap().starts_with("tint mask"));
        assert_eq!(l["macro"]["tint"]["png"], "t16.png");
        assert!(layers("b21f1ba5", &entries).unwrap()["detail_uv"]["alt_uv_set"].is_null());
        assert_eq!(layers("27e5c49d", &entries).unwrap()["detail_uv"]["alt_uv_set"], "TEXCOORD_1");
    }

    fn prim(f: impl Fn(f32, f32) -> [[f32; 2]; 2]) -> crate::gltf::TriPrim {
        let mut positions = Vec::new();
        let (mut u0, mut u1) = (Vec::new(), Vec::new());
        for i in 0..10 {
            for j in 0..10 {
                let (x, y) = (i as f32 * 3.0, j as f32 * 2.0);
                positions.push([x, y, 0.0]);
                let t = f(x, y);
                u0.push(t[0]);
                u1.push(t[1]);
            }
        }
        crate::gltf::TriPrim {
            name: String::new(), positions, normals: None, tangents: None, uvs: vec![u0, u1], colors: None, colors1: None, regions: None, tint: None,
            indices: vec![], material: Some(0), joints: None, weights: None, joints1: None, weights1: None,
        }
    }

    #[test]
    fn uv_fit_detail() {
        // section tile: UV0 = position / 512
        let tile = prim(|x, y| [[x / 512.0, y / 512.0], [0.0, 0.0]]);
        let f = uv_fit([&tile].into_iter());
        assert_eq!(f[0]["planar"], true);
        assert!((f[0]["metres_per_unit"][0].as_f64().unwrap() - 512.0).abs() < 1.0);
        assert_eq!(f[1]["constant"], true);
        assert_eq!(four_layer_detail_uv(&f)["section_tile"], true);
        // set piece: UV0 local at 35 m, UV1 planar at 3 m
        let piece = prim(|x, y| [[x / 35.0, y / 35.0], [x / 3.0, -y / 3.0]]);
        let r = four_layer_detail_uv(&uv_fit([&piece].into_iter()));
        assert_eq!(r["uv"], "TEXCOORD_1");
        // nothing planar at a detail scale: from position
        let odd = prim(|x, y| [[(x * y).sin(), x.cos()], [(x * 7.0).sin(), (y * 5.0).cos()]]);
        assert_eq!(four_layer_detail_uv(&uv_fit([&odd].into_iter()))["uv"], "position");
    }

    #[test]
    fn prop_two_layer_slots() {
        let entries: Vec<Value> = [0usize, 4, 6, 8, 10].iter().map(|&p| json!({"param": p, "png": format!("t{p}.png"), "status": "ok"})).collect();
        let l = layers("89c8fe07", &entries).unwrap();
        assert_eq!(l["layers"][0]["colour"]["png"], "t0.png");
        assert_eq!(l["layers"][0]["normal"]["png"], "t6.png");
        assert_eq!(l["layers"][1]["colour"]["png"], "t4.png");
        assert_eq!(l["layers"][1]["normal"]["png"], "t10.png");
        assert_eq!(l["overlay"]["colour_alpha"]["png"], "t8.png");
        assert_eq!(l["overlay"]["uv_set"], "TEXCOORD_2");
        assert!(l["prop_2lyr"].is_object());
        assert_eq!(l["layers"][1]["weight"], "1 - COLOR0.a");
        assert_eq!(layers("b217b263", &entries).unwrap()["layers"][1]["weight"], "COLOR0.a");
        assert_eq!(layers("b217b263", &entries).unwrap()["overlay"]["uv_set"], "TEXCOORD_2");
    }

    #[test]
    fn vehicle_classes() {
        let alpha = [0, 0, 0, 0, 2, 0, 0, 0];
        let opaque = [0, 0, 0, 0, 1, 0, 0, 0];
        // lamp lenses blend; reflectors are chrome, not paint; tyres rubber
        let lens = classify("389206d1", &alpha);
        assert_eq!((lens["class"].as_str(), lens["alpha_mode"].as_str()), (Some("glass"), Some("BLEND")));
        let refl = classify("03b27476", &opaque);
        assert_eq!((refl["class"].as_str(), refl["stand_in"].as_str()), (Some("opaque"), Some("chrome")));
        assert!(!is_paint("03b27476") && is_paint("4aa38282"));
        let tyre = classify("99ad6fc9", &opaque);
        assert_eq!(tyre["stand_in"], "rubber");
        assert!(is_tyre_shader("99ad6fc9") && is_tyre_shader("b994d670") && !is_tyre_shader("c5cdbdd2"));
        let g = gltf_fields(&tyre, false);
        assert_eq!(g["pbrMetallicRoughness"]["baseColorFactor"][0], 0.02);
        assert!(gltf_fields(&tyre, true)["pbrMetallicRoughness"]["baseColorFactor"].is_null());
        assert_eq!(classify("5a693bbe", &opaque)["class"], "emissive");
        assert_eq!(classify("4aa38282", &opaque)["stand_in"], Value::Null);
        let mask = classify("bd71bdbf", &alpha);
        assert_eq!(mask["stand_in"], "hidden");
        let g = gltf_fields(&mask, false);
        assert_eq!(g["alphaMode"], "MASK");
        assert_eq!(g["pbrMetallicRoughness"]["baseColorFactor"][3], 0.0);
        assert!(g["emissiveFactor"].is_null());
        assert_eq!(classify("fc34f08b", &opaque)["stand_in"], "hidden");
        assert_eq!(classify("e68a4a98", &opaque)["stand_in"], "gunmetal");
        let rim = gltf_fields(&json!({"class": "opaque", "alpha_mode": "OPAQUE", "stand_in": "rim"}), false);
        assert_eq!(rim["pbrMetallicRoughness"]["metallicFactor"], 1.0);
        let trim = gltf_fields(&json!({"class": "opaque", "alpha_mode": "OPAQUE", "stand_in": "trim"}), false);
        assert_eq!(trim["pbrMetallicRoughness"]["metallicFactor"], 0.0);
        assert_eq!(trim["pbrMetallicRoughness"]["baseColorFactor"][3], 1.0);
    }

    #[test]
    fn emissive_fields() {
        let m = gltf_fields(&json!({"class": "emissive", "alpha_mode": "OPAQUE"}), true);
        assert_eq!(m["emissiveTexture"], "base");
        assert_eq!(m["emissiveFactor"], json!([1.0, 1.0, 1.0]));
    }

    #[test]
    fn emissive_multiplier_hash() {
        assert_eq!(format!("{:08x}", crate::names::joaat_exact("emissivemultiplier")), EMISSIVE_MULTIPLIER_PARAM);
    }

    #[test]
    fn emissive_strength_from_multiplier() {
        let lit = |m: Option<f32>| {
            let mut g = gltf_fields(&json!({"class": "emissive", "alpha_mode": "OPAQUE"}), true);
            let e = EmissiveStrength::from_multiplier(m);
            e.apply(&mut g);
            (e, g)
        };
        // glass railings (5): dim, all in the factor, no extension
        let (e, g) = lit(Some(5.0));
        assert_eq!((e.source, e.strength), ("multiplier", 0.05));
        assert_eq!(g["emissiveFactor"], json!([0.05f32, 0.05f32, 0.05f32]));
        assert!(g["extensions"].is_null());
        // a bulb (628) and an LED sign (3225.8): factor 1, the rest in the extension
        for (m, s) in [(628.0f32, 6.28f32), (3225.8, 32.258)] {
            let (e, g) = lit(Some(m));
            assert!((e.strength - s).abs() < 1e-4, "{m}");
            assert_eq!(g["emissiveFactor"], json!([1.0, 1.0, 1.0]));
            assert_eq!(g["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"], json!(e.strength));
        }
        // 0: switched on at run time, kept lit at strength 1
        let (e, g) = lit(Some(0.0));
        assert_eq!((e.source, e.strength, e.multiplier), ("runtime", 1.0, Some(0.0)));
        assert_eq!(g["emissiveFactor"], json!([1.0, 1.0, 1.0]));
        assert!(g["extensions"].is_null());
        // none, negative or not finite: strength 1
        for m in [None, Some(-3.0), Some(f32::NAN), Some(f32::INFINITY)] {
            let e = EmissiveStrength::from_multiplier(m);
            assert_eq!((e.source, e.strength), ("default", 1.0), "{m:?}");
        }
        // over 10 000 nits: capped
        let e = EmissiveStrength::from_multiplier(Some(50_000.0));
        assert_eq!((e.source, e.strength, e.extension()), ("capped", EMISSIVE_MAX_STRENGTH, Some(EMISSIVE_MAX_STRENGTH)));
        assert_eq!(e.json()["multiplier"], json!(50_000.0));
        // tint: a dim tinted light keeps its colour in the factor; black,
        // negative or short tints are ignored
        let e = EmissiveStrength::from_multiplier(Some(50.0)).with_tint(Some(&[0.2, 0.4, 1.0]));
        assert_eq!(e.factor_rgb(), [0.1, 0.2, 0.5]);
        assert_eq!(e.json()["tint"], json!([0.2f32, 0.4f32, 1.0f32]));
        for t in [&[0.0f32, 0.0, 0.0][..], &[-1.0, 0.5, 0.5], &[0.5, 0.5], &[f32::NAN, 1.0, 1.0]] {
            assert_eq!(EmissiveStrength::from_multiplier(Some(50.0)).with_tint(Some(t)).tint, None, "{t:?}");
        }
    }

    #[test]
    fn four_layer_tint_formula() {
        let k = |hash: &str, v: [f32; 4], count: u8| crate::drawable::ShaderConst { hash: hash.into(), desc: String::new(), offset: 0, values: Some(v), buffer: 7, count, ty: None };
        let mut consts = vec![
            k("8e3fb0aa", [0.0, 1.0, 0.0, 0.0], 4), k("db26e4de", [2.0, 3.0, 0.0, 1.0], 4), k("1caef69a", [1.0, 1.0, 1.0, 0.0], 4), k("b1c84936", [1.0, 0.0, 0.0, 0.0], 4),
            k("81bc5bca", [1.0, 1.5, 1.0, 0.0], 4), k("896d1e47", [1.0, 1.0, 0.0, 1.0], 4), k("3fb2b8f7", [1.0, 0.0, 0.0, 0.0], 1), k("834fb13b", [0.0; 4], 1),
        ];
        let mut l = json!({});
        // a switch is missing: nothing is written
        four_layer_tint(&mut l, &consts);
        assert!(l["tint_formula"].is_null());
        consts.push(k("abe2c64d", [1.0, 0.0, 0.0, 0.0], 1));
        four_layer_tint(&mut l, &consts);
        let p = &l["tint_formula"]["per_layer"];
        // layer 0: luminance texture tint with a blend amount, so the mask is inverted; multiply vertex tint
        assert_eq!((p[0]["mask"].as_str(), p[0]["vertex_tint"].as_str(), p[0]["texture_tint"].as_str(), p[0]["texture_tint_amount"].as_str()), (Some("1 - a"), Some("multiply"), Some("luminance"), Some("saturate(a x 1)")));
        // layer 1: multiply texture tint keeps the mask; layer 2: no intensity and no mask scale; layer 3: no blend amount, no vertex intensity
        assert_eq!((p[1]["mask"].as_str(), p[1]["vertex_tint"].as_str(), p[1]["texture_tint"].as_str()), (Some("a"), Some("luminance"), Some("multiply")));
        assert_eq!((p[2]["vertex_tint"].as_str(), p[2]["texture_tint"].as_str(), p[3]["vertex_tint"].as_str(), p[3]["texture_tint"].as_str()), (Some("off"), Some("off"), Some("off"), Some("off")));
        assert_eq!((l["tint_formula"]["steps"].as_array().map(Vec::len), l["tint_formula"]["switches"]["vertex tint on (3fb2b8f7)"].as_f64()), (Some(7), Some(1.0)));
        // the blend amount as a number when the material does not scale it by the mask
        consts.last_mut().unwrap().values = Some([0.0; 4]);
        four_layer_tint(&mut l, &consts);
        assert_eq!(l["tint_formula"]["per_layer"][0]["texture_tint_amount"].as_f64(), Some(1.0));
    }
}
