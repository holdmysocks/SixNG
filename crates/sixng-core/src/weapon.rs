//! Weapons from their parts.
//!
//! A weapon model (`w_ar_adb_01_hi`, in `data.rpf`) is a small skeleton with
//! a placeholder shell of about a hundred triangles. What is seen in game is
//! built from separate part models named `<weapon>_<slot>_<NN>` with the same
//! LOD suffix: `w_ar_adb_01_receiver_01_hi`, `…_barrel_02_hi`, `…_stock_03_hi`.
//!
//! Parts mount on attach-point bones. A part's root bone is `AAP_<Slot>`
//! (at its origin); the part it mounts on has a bone `WAP_<Slot>`. The base
//! has one (`WAP_Receiver` on a rifle, `WAP_Frame` on a pistol), the receiver
//! or frame has most of the others (`WAP_Barrel`, `WAP_Stock`, `WAP_Mag`,
//! `WAP_Slide` …), and parts carry further ones (`WAP_Muzzle` on a barrel,
//! `WAP_Sight_F` on a slide). Names match without regard to case
//! (`WAP_Dustcover` / `AAP_DustCover`). A part without bones is modelled
//! around its attach point and takes the bone named after its slot
//! (`…_pgrip_01` on `WAP_PGrip`).
//!
//! Which variant of each slot a weapon carries by default is not decoded: the
//! lowest-numbered one is taken.

use crate::outfit::PartOpt;
use crate::scan::Entry;
use std::collections::BTreeMap;

/// 4x4 matrix, column-major (translation in elements 12..15).
pub type Mat = [f32; 16];

pub const IDENTITY: Mat = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// Inverse of a rigid transform (rotation and translation).
pub fn inverse_rigid(m: &Mat) -> Mat {
    let mut o = IDENTITY;
    for c in 0..3 {
        for r in 0..3 {
            o[c * 4 + r] = m[r * 4 + c];
        }
    }
    for r in 0..3 {
        o[12 + r] = -(0..3).map(|k| o[k * 4 + r] * m[12 + k]).sum::<f32>();
    }
    o
}

/// Splits `w_ar_adb_01_hi` into the weapon stem and its LOD suffix.
fn stem(name: &str) -> (&str, &str) {
    for s in ["_hi", "_mid"] {
        if let Some(b) = name.strip_suffix(s) {
            return (b, s);
        }
    }
    (name, "")
}

/// A weapon's own model: `w_<class>_<make>_<NN>` with an optional LOD
/// suffix. Part models have more words; the placeholder shapes
/// (`w_preset_handgun_large_01`) do too.
pub fn is_weapon(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let (b, _) = stem(&n);
    let w: Vec<&str> = b.split('_').collect();
    w.len() == 4 && w[0] == "w" && w[1].chars().all(|c| c.is_ascii_lowercase()) && !w[2].is_empty() && w[2].chars().all(|c| c.is_ascii_alphanumeric()) && w[3].len() >= 2 && w[3].chars().all(|c| c.is_ascii_digit())
}

/// Slot and variant of a part name (`barrel_02` → ("barrel", "02"),
/// `splitter_20_alt` is not a weapon form and gives None).
fn slot_variant(part: &str) -> Option<(&str, &str)> {
    let (slot, var) = part.rsplit_once('_')?;
    let digits = var.trim_end_matches(|c: char| c.is_ascii_lowercase());
    (!slot.is_empty() && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some((slot, var))
}

/// The parts of a weapon for the viewer's Parts panel: one group per slot,
/// the lowest-numbered variant on by default. Parts are taken at the
/// weapon's own LOD suffix, falling back to `_hi` where a part has no copy
/// at that level. Empty for a model that is not a weapon or has no parts.
pub fn weapon_options(entries: &[Entry], name: &str) -> Vec<PartOpt> {
    let n = name.to_ascii_lowercase();
    if !is_weapon(&n) {
        return Vec::new();
    }
    let (base, suffix) = stem(&n);
    let prefix = format!("{base}_");
    // part -> (level it was found at, entry); the weapon's own level wins
    let mut found: BTreeMap<String, (bool, &Entry)> = BTreeMap::new();
    for e in entries {
        let Some(en) = e.name.as_deref() else { continue };
        let en = en.to_ascii_lowercase();
        let Some(rest) = en.strip_prefix(&prefix) else { continue };
        let (part, s) = stem(rest);
        let own = s == suffix;
        if !own && s != "_hi" {
            continue;
        }
        if slot_variant(part).is_none() {
            continue;
        }
        let better = match found.get(part) {
            None => true,
            Some((o, x)) => (own, e.usize) > (*o, x.usize),
        };
        if better {
            found.insert(part.to_string(), (own, e));
        }
    }
    let mut seen: Vec<String> = Vec::new();
    found
        .iter()
        .map(|(part, (_, e))| {
            let (slot, var) = slot_variant(part).unwrap();
            let first = !seen.iter().any(|s| s == slot);
            if first {
                seen.push(slot.to_string());
            }
            PartOpt { off: e.off, id: e.id.clone(), name: e.name.clone().unwrap_or_default(), group: slot.to_string(), label: format!("{} {var}", slot.replace('_', " ")), default: first }
        })
        .collect()
}

/// Slot of a part model's name for a weapon (`w_pi_duke_01_pgrip_01_hi` of
/// `w_pi_duke_01_hi` → "pgrip").
pub fn part_slot(weapon: &str, part: &str) -> Option<String> {
    let (w, p) = (weapon.to_ascii_lowercase(), part.to_ascii_lowercase());
    let rest = stem(&p).0.strip_prefix(&format!("{}_", stem(&w).0))?.to_string();
    slot_variant(&rest).map(|(s, _)| s.to_string())
}

/// Places parts on a weapon. `base` and each part give their bones as
/// (name, bone-to-model matrix) in the model's own space; a part also gives
/// its slot. Returns each part's model-to-weapon matrix, or None where no
/// attach point takes it (an accessory for a mount this build lacks).
///
/// A part goes on the bone `WAP_<x>` of the base or of a part placed
/// earlier, where `<x>` is its root `AAP_<x>` bone's name, else its slot.
/// Its own attach bone (identity when it has none) is put on that bone.
pub fn place(base: &[(String, Mat)], parts: &[(String, Vec<(String, Mat)>)]) -> Vec<Option<Mat>> {
    let lower = |s: &str| s.to_ascii_lowercase();
    // attach points in weapon space, by lower-case name
    let mut points: BTreeMap<String, Mat> = base.iter().filter(|b| lower(&b.0).starts_with("wap_")).map(|b| (lower(&b.0), b.1)).collect();
    let mut out: Vec<Option<Mat>> = vec![None; parts.len()];
    loop {
        let mut progress = false;
        for (i, (slot, bones)) in parts.iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            let aap = bones.iter().find(|b| lower(&b.0).starts_with("aap_"));
            let mut wanted: Vec<String> = Vec::new();
            if let Some(a) = aap {
                wanted.push(format!("wap_{}", &lower(&a.0)[4..]));
            }
            wanted.push(format!("wap_{}", lower(slot)));
            let Some(at) = wanted.iter().find_map(|w| points.get(w)) else { continue };
            let own = aap.map(|a| inverse_rigid(&a.1)).unwrap_or(IDENTITY);
            let t = mul(at, &own);
            for b in bones.iter().filter(|b| lower(&b.0).starts_with("wap_")) {
                points.entry(lower(&b.0)).or_insert_with(|| mul(&t, &b.1));
            }
            out[i] = Some(t);
            progress = true;
        }
        if !progress {
            return out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32, z: f32) -> Mat {
        let mut m = IDENTITY;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    #[test]
    fn names() {
        assert!(is_weapon("w_ar_adb_01_hi"));
        assert!(is_weapon("w_pi_duke_01"));
        assert!(is_weapon("W_SG_Kozlov_01_mid"));
        assert!(!is_weapon("w_ar_adb_01_barrel_01_hi"));
        assert!(!is_weapon("w_preset_handgun_large_01"));
        assert!(!is_weapon("wpn_proxy_thrown_bomb"));
        assert_eq!(part_slot("w_ar_adb_01_hi", "w_ar_adb_01_sight_r_02_hi").as_deref(), Some("sight_r"));
        assert_eq!(part_slot("w_ar_adb_01_hi", "w_ar_duke_01_barrel_01_hi"), None);
    }

    #[test]
    fn chain_of_parts() {
        let base = vec![("WAP_Receiver".to_string(), at(0.0, 0.0, 0.1))];
        let parts = vec![
            // listed before the barrel it mounts on
            ("muzzle".to_string(), vec![("AAP_Muzzle".to_string(), IDENTITY)]),
            ("barrel".to_string(), vec![("AAP_Barrel".to_string(), IDENTITY), ("WAP_Muzzle".to_string(), at(0.4, 0.0, 0.0))]),
            ("receiver".to_string(), vec![("AAP_Receiver".to_string(), IDENTITY), ("WAP_Barrel".to_string(), at(0.1, 0.0, 0.0)), ("WAP_PGrip".to_string(), at(-0.05, 0.0, -0.02))]),
            // no bones: by slot name
            ("pgrip".to_string(), vec![]),
            // nothing takes it
            ("reflexmount".to_string(), vec![("AAP_ReflexMount_01".to_string(), IDENTITY)]),
        ];
        let t = place(&base, &parts);
        let pos = |m: &Option<Mat>| m.map(|m| [m[12], m[13], m[14]]);
        assert_eq!(pos(&t[2]), Some([0.0, 0.0, 0.1]));
        assert_eq!(pos(&t[1]), Some([0.1, 0.0, 0.1]));
        assert_eq!(pos(&t[0]), Some([0.5, 0.0, 0.1]));
        assert_eq!(pos(&t[3]), Some([-0.05, 0.0, 0.08]));
        assert_eq!(t[4], None);
    }

    #[test]
    fn rigid_inverse() {
        // 90 degrees about Z, then a translation
        let m: Mat = [0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0];
        let p = mul(&m, &inverse_rigid(&m));
        for (a, b) in p.iter().zip(IDENTITY.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
