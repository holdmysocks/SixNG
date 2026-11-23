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

/// (shader name hash, class, family description)
const FAMILIES: &[(&str, &str, &str)] = &[
    // vehicles
    ("4aa38282", "car_paint", "vehicle body (paint is a runtime colour; colour slot = engine-default white)"),
    ("476ac73a", "car_paint", "vehicle body variant"),
    ("03b27476", "opaque", "vehicle lamp reflectors (standard_vehicle_2lyr_reflector)"),
    ("4d2b05e7", "car_paint", "vehicle body variant"),
    ("c5cdbdd2", "car_paint", "vehicle body variant"),
    ("389206d1", "glass", "vehicle lamp lenses and clear covers (standard_vehicle_alpha; opacity in the colour alpha)"),
    ("29648098", "emissive", "vehicle lamps (standard_vehicle_emissive_animated_bones)"),
    ("5a693bbe", "emissive", "vehicle lamps and lit panels (standard_vehicle_emissive_layer)"),
    ("99ad6fc9", "opaque", "vehicle tyre (standard_vehicle_2lyr_tyre_inst)"),
    ("fa9cc069", "window", "vehicle windows"),
    ("4dc0ac6a", "window", "vehicle windows (helicopters, boats)"),
    ("bd71bdbf", "window", "windscreen wiper sweep mask (windshield_wipermask): a fan over the glass that marks where the wipers clear rain; not a visible surface"),
    ("921b19e5", "decal", "vehicle livery / sign decal"),
    // glass
    ("298c4c0f", "glass", "exterior window panes"),
    ("653582a4", "glass", "exterior glass"),
    ("6a817597", "glass", "interior glass, curtains, doors"),
    ("db8b21bd", "glass", "glass railings, door glass"),
    ("e3ba3129", "glass", "building doors / glazing"),
    ("656a1cdb", "glass", "bottles and drinking glasses"),
    ("12839ca7", "glass", "drinking glasses with liquid"),
    ("9ceb543e", "glass", "cups with liquid"),
    ("9f8ae601", "glass", "bottles"),
    ("0357215a", "glass", "cups"),
    ("d5a93b6f", "glass", "glass / sheer cloth"),
    ("d9ec330c", "emissive", "light bulbs (clear glass)"),
    // emissive
    ("faec4fc8", "emissive", "digital signs / screens"),
    ("f5ba53da", "emissive", "baked lightmap overlay"),
    ("3a09c775", "emissive", "LED lights and panels"),
    ("3bc8ac60", "emissive", "emissive detail"),
    ("60643f2e", "emissive", "traffic-light counter"),
    ("5b775918", "emissive", "phone screens"),
    // water
    ("7f3a6aa9", "water", "water surface"),
    ("97596162", "water", "fountain / VFX water"),
    ("735d2de6", "water", "waterfall VFX"),
    // terrain / multilayer
    ("61e09df7", "terrain_multilayer", "landscape blend (17 params)"),
    ("360e396b", "terrain_multilayer", "landscape blend (17 params)"),
    ("b21f1ba5", "terrain_multilayer", "landscape blend (17 params)"),
    ("0ad54cb9", "terrain_multilayer", "landscape blend (17 params)"),
    ("77d8ab27", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("1251cae5", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("cd8ceb5e", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("4f0b809b", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    ("b217b263", "terrain_multilayer", "vertex-blended multilayer: ground, walls, interiors (12 params)"),
    // other
    ("255417c3", "cutout", "vegetation"),
    ("e05f1a42", "decal", "road signs"),
    ("89a1cf72", "opaque", "ped head / skin"),
    ("379efcef", "opaque", "ped eyes"),
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
    SLOTS.iter().find(|s| s.0 == param_hash).map(|s| s.1)
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
    let class = fam.map(|f| f.1).unwrap_or(if named_emissive { "emissive" } else { bucket });
    // Parts whose colour is not in a texture (it comes from the vehicle
    // material palette, not decoded): a stand-in surface for converters.
    let stand_in = match (name_hash, name.as_deref()) {
        ("99ad6fc9", _) => Some("rubber"),
        (_, Some(n)) if n.starts_with("standard_vehicle") && n.contains("tyre") => Some("rubber"),
        ("03b27476", _) => Some("chrome"),
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
        "family": fam.map(|f| f.2),
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

pub fn is_paint(name_hash: &str) -> bool {
    FAMILIES.iter().any(|f| f.0 == name_hash && f.1 == "car_paint")
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
    if matches!(name_hash, "61e09df7" | "b21f1ba5" | "360e396b" | "27e5c49d") {
        return Some(four_layer(name_hash, &tex));
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

/// Constants of a 4-layer ground material. Vec4s whose component k follows
/// layer k's texture (seam test: equal across a material border whenever
/// that layer's texture is equal there) are split per layer; their meaning
/// is not known, so they keep their hash (and name when one resolved).
pub fn four_layer_params(consts: &[crate::drawable::ShaderConst]) -> Value {
    // per-layer vec4s (component k = layer k) with the seam agreement found
    const PER_LAYER: &[(&str, &str)] = &[
        ("c35e27af", "equal across seams 100 % for the same texture (44 % otherwise); values 0.65-2.0, 1.0 on 108 of 137 layers: a per-texture factor (tiling or intensity, not decided: no correlation with texture brightness or feature size, |r| <= 0.12, and the baked low-detail maps show no detail repeat to measure). As a tiling factor, values where 64 x value is not whole (1.2, 1.3, 1.325, 1.4, 1.6, 0.65) would leave a seam at every 512 m section border, since TEXCOORD_0 restarts there"),
        ("627ce8ae", "99 % / 71 %; values 1 or 2"),
        ("781f7100", "98 % / 10 %; values 0.35-0.75"),
        ("9a4a3781", "92 % / 14 %; values 0-0.175 on some sections, mostly 0.5 or 1.0 on others (parallax or height depth?)"),
        ("f354f896", "85 % / 13 %; values about 1-16"),
        ("9ede7c13", "83 % / 23 %; values about 1-14"),
        ("b25b6656", "100 % / 86 %; mostly 1.0"),
        ("30bf5bed", "mostly 0.5"),
    ];
    let name = |h: &str| crate::names::param(h);
    let mut layers: Vec<serde_json::Map<String, Value>> = vec![serde_json::Map::new(); 4];
    let mut per_layer_notes = serde_json::Map::new();
    let mut other = serde_json::Map::new();
    for c in consts {
        let Some(v) = c.values else { continue };
        if v.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
            continue; // the block's first words are not constants (pointer-like)
        }
        if let Some((_, note)) = PER_LAYER.iter().find(|(h, _)| *h == c.hash) {
            for (k, l) in layers.iter_mut().enumerate() {
                l.insert(c.hash.clone(), json!(v[k]));
            }
            per_layer_notes.insert(c.hash.clone(), json!(note));
        } else if v.iter().any(|x| *x != 0.0) {
            let key = match name(&c.hash) { Some(n) => format!("{n} ({})", c.hash), None => c.hash.clone() };
            other.insert(key, json!(v));
        }
    }
    json!({
        "per_layer": layers,
        "per_layer_notes": per_layer_notes,
        "material": other,
        "note": "Constant block values (vec4, float offset from the parameter table). Names: joaat dictionary hits only (blendsoftness, tintintensity, tintblendamount, bumpiness …); the rest are hashes. Meanings are not decoded.",
    })
}

/// The 17-slot 4-layer ground shaders (`standard_uber_4lyr_dpm_dm_hbb`
/// 61e09df7, `…_pxm_hbb_fixuv1` b21f1ba5, `…_spx_dm_hbb` 360e396b,
/// `…_pxm_hbb_uv3` 27e5c49d), same slot layout in all four. Slot names are recovered parameter names
/// (`diffusetexture_layer0` …); the unnamed data maps were paired with
/// their layer by co-occurrence (each data-map hash goes with exactly one
/// layer's diffuse over 200 materials). Weights: the section-wide
/// `lookuptexture`, sampled with TEXCOORD_0 (= section position / 512 m);
/// R, G, B weight layers 0, 1, 2 (seam-continuity test across material
/// borders: the slot a channel drives agrees across the border 66 / 74 /
/// 92 % of the time where that channel dominates), the rest goes to layer 3.
fn four_layer(name_hash: &str, tex: &dyn Fn(usize) -> Value) -> Value {
    // (colour, normal, data) per layer
    const SLOTS: [[usize; 3]; 4] = [[14, 15, 4], [12, 10, 0], [9, 11, 5], [13, 7, 1]];
    let weight = ["lookup.r", "lookup.g", "lookup.b", "1 - (lookup.r + lookup.g + lookup.b)"];
    let layers: Vec<Value> = SLOTS
        .iter()
        .enumerate()
        .map(|(i, s)| json!({"name": format!("layer{i}"), "colour": tex(s[0]), "normal": tex(s[1]), "data": tex(s[2]), "uv_set": "TEXCOORD_0", "uv_scale": 64.0, "weight": weight[i]}))
        .collect();
    let alt_uv = match name_hash {
        "b21f1ba5" => Value::Null,
        "27e5c49d" => json!("TEXCOORD_2"),
        _ => json!("TEXCOORD_1"),
    };
    json!({
        "family": "four_layer",
        "layers": layers,
        "weights": {"texture": tex(2), "uv_set": "TEXCOORD_0 (on section tiles = section position / 512 m; on other models the model's own UV0, see uv_fit)", "channels": {"r": "layer0", "g": "layer1", "b": "layer2", "remainder": "layer3"}, "note": "R+G+B <= 1 (BC7 rounding up to 258/255: clamp); the remainder weights layer 3"},
        "macro": {"uv_set": "TEXCOORD_0", "tint": tex(16), "tint_normal": tex(8), "grass_mask": tex(3), "note": "section-wide maps (one 512 m section per texture): colour variation, large-scale normal, grass placement mask"},
        "detail_uv": {"default": "TEXCOORD_0 * 64 (world-planar, one repeat per 8 m) on map section tiles only; `recommended` follows this material's measured mapping (`uv_fit`): set pieces and rock formations using these shaders have their own UV0 (about 35 m per unit on the street-trash grounds), usually with a planar TEXCOORD_1 at 3 m per unit", "alt_uv_set": alt_uv, "note": "TEXCOORD_1 (61e09df7, 360e396b) is world-planar / 8 m on flat ground and unwrapped elsewhere (probably slopes); constant on b21f1ba5 ('fixuv1'), so that variant tiles from position. 27e5c49d ('uv3'): TEXCOORD_1 is not a usable UV (spans >1000), TEXCOORD_2 is an unwrap (not world-planar), so the alternative set is TEXCOORD_2 (low confidence). Per-layer tiling factors sit in the shader's constant block and are not decoded; scale per layer by eye"},
        "vertex_color_channels": {"rgb": "unverified: varies 50..230, mean ~155, not weights (does not sum to a constant); baked shading or tint", "a": "always 0"},
        "texcoord_2": "a copy of TEXCOORD_0 on most 61e09df7 primitives, constant elsewhere",
        "height_blend": "the shader names end in hbb (height-based blend): each layer's data map B channel looks like a height map; sharpen the weights by height for the game's look",
        "confidence": {"slots": "high (recovered names)", "data_map_pairing": "high", "weights_uv": "high", "channel_rgb_to_layer012": "medium", "layer3_remainder": "low", "detail_uv_scale": "low"},
        "note": "Derived from recovered parameter names, texture co-occurrence and UV/seam statistics, not from shader code. Normal maps: DirectX green.",
    })
}

#[cfg(test)]
mod paint_tests {
    use super::*;

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
    fn four_layer_slots() {
        let entries: Vec<Value> = [14usize, 15, 4, 2, 16].iter().map(|&p| json!({"param": p, "png": format!("t{p}.png"), "status": "ok"})).collect();
        let l = layers("61e09df7", &entries).unwrap();
        assert_eq!(l["layers"][0]["colour"]["png"], "t14.png");
        assert_eq!(l["layers"][0]["normal"]["png"], "t15.png");
        assert_eq!(l["layers"][0]["data"]["png"], "t4.png");
        assert_eq!(l["layers"][3]["colour"]["param"], 13);
        assert_eq!(l["layers"][3]["colour"]["status"], "no texture bound");
        assert_eq!(l["weights"]["texture"]["png"], "t2.png");
        assert_eq!(l["macro"]["tint"]["png"], "t16.png");
        assert!(layers("b21f1ba5", &entries).unwrap()["detail_uv"]["alt_uv_set"].is_null());
        assert_eq!(layers("27e5c49d", &entries).unwrap()["detail_uv"]["alt_uv_set"], "TEXCOORD_2");
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
            name: String::new(), positions, normals: None, tangents: None, uvs: vec![u0, u1], colors: None, tint: None,
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
}
