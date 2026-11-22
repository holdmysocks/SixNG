//! Whole animals and characters from their parts.
//!
//! An animal's fragment (`crr_iguana_green_m_00`) holds the skeleton and a
//! placeholder plane; its textured body, eyes, fur shells, head and so on are
//! separate drawables named `<root>_<part>_00` (variants use other numbers or
//! suffixes: `_body_08_collarharness`). Named characters (`cs_ali`,
//! `u_m_m_farm_monster_man`, `player_story000_01`) are sets of clothing and
//! body drawables `<prefix>_<item>_<fit>_…` with fit tags like `mr1` / `fr1`.
//! Both are put together here for the viewer: the default parts, one copy each
//! (the first in the archive), in their bind pose.

use sixng_core::outfit::{dedupe, is_fit};
pub use sixng_core::outfit::{
    is_animal, is_ped_part, is_whole_ped, outfit_options, outfit_parts, outfit_prefix, PartOpt,
};
use sixng_core::scan::{ArchiveIndex, Entry};

const ANIMAL_SKIP: &[&str] = &[
    "generic",
    "costume",
    "collar",
    "harness",
    "hat",
    "badge",
    "ribbon",
    "bandana",
    "cape",
    "lace",
    "flower",
    "knife",
    "vest",
    "rope",
    "tranquilizer",
    "loop",
    "shadesaviators",
    "accessories",
    "human",
    "stick",
    "corpse",
    "roadkill",
    "gut",
    "gonad",
    "bone",
];

/// Every part of an animal root: `<root>_<words>_<NN>[_<set>]` (`body_00`,
/// `body_08_collarharness`, `harness_08_collarharness`). The default look is
/// the `_00` parts without accessories. Roots without parts of their own fall
/// back to `<root minus _NN>_…`.
pub fn animal_options(ix: &ArchiveIndex, root: &str) -> Vec<PartOpt> {
    let root = root.to_ascii_lowercase();
    let mut prefixes = vec![format!("{root}_")];
    if let Some((stem, n)) = root.rsplit_once('_') {
        if n.chars().all(|c| c.is_ascii_digit()) {
            prefixes.push(format!("{stem}_"));
        }
    }
    let mut first_any: Vec<PartOpt> = Vec::new();
    for p in prefixes {
        let by_name = dedupe(
            ix.entries
                .iter()
                .filter(|e| e.rtype.as_deref() == Some("dr"))
                .filter_map(|e| {
                    let n = e.name.as_ref()?.to_ascii_lowercase();
                    n.starts_with(&p).then_some((n, e))
                }),
        );
        let mut opts = Vec::new();
        for (n, e) in by_name {
            let toks: Vec<&str> = n[p.len()..].split('_').collect();
            let Some(j) = toks
                .iter()
                .position(|t| t.len() == 2 && t.chars().all(|c| c.is_ascii_digit()))
            else {
                continue;
            };
            let group = &toks[..j];
            if group.is_empty()
                || group
                    .iter()
                    .any(|t| t.is_empty() || !t.chars().all(|c| c.is_ascii_lowercase()))
            {
                continue;
            }
            let set = toks[j + 1..].join(" ");
            let default =
                toks[j] == "00" && set.is_empty() && !group.iter().any(|t| ANIMAL_SKIP.contains(t));
            let label = if toks[j] == "00" && set.is_empty() {
                "default".to_string()
            } else {
                format!("{} {}", toks[j], set).trim().to_string()
            };
            opts.push(PartOpt {
                off: e.off,
                id: e.id.clone(),
                name: n.clone(),
                group: group.join(" "),
                label,
                default,
            });
        }
        if opts.iter().any(|o| o.default) {
            return opts;
        }
        if first_any.is_empty() {
            first_any = opts;
        }
    }
    first_any
}

/// Default parts of an animal root: `<root>_<words>_00` without accessories.
pub fn animal_parts<'a>(ix: &'a ArchiveIndex, root: &str) -> Vec<&'a Entry> {
    let offs: Vec<u64> = animal_options(ix, root)
        .into_iter()
        .filter(|o| o.default)
        .map(|o| o.off)
        .collect();
    ix.entries
        .iter()
        .filter(|e| offs.contains(&e.off))
        .collect()
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Readable names for animals and people; None for other models.
/// `crr_iguana_green_m_00` → "Iguana (green, male)",
/// `crm_dog_rottweiler_f_01` → "Dog (rottweiler, female) 2",
/// `cs_ali` → "Ali", `u_m_m_farm_monster_man` → "Farm monster man",
/// `tshirtcrewneck_mr1_019_…` → "Tshirtcrewneck (male) 019".
pub fn pretty_name(name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    if is_animal(&n) {
        let t: Vec<&str> = n[4..].split('_').collect();
        let i = t
            .iter()
            .position(|x| *x == "m" || *x == "f")
            .unwrap_or(t.len());
        let sex = t.get(i).map(|x| if *x == "m" { "male" } else { "female" });
        let mut after: Vec<&str> = t.get(i + 1..).map(|x| x.to_vec()).unwrap_or_default();
        let mut num = None;
        if after
            .first()
            .map_or(false, |x| x.chars().all(|c| c.is_ascii_digit()))
        {
            num = after.remove(0).parse::<u32>().ok();
        }
        if after.last() == Some(&"00") {
            after.pop();
        }
        let species = t.first()?;
        let mut extra: Vec<String> = t[1..i.min(t.len())].iter().map(|s| s.to_string()).collect();
        if let Some(s) = sex {
            extra.push(s.into());
        }
        let mut out = cap(species);
        if !extra.is_empty() {
            out += &format!(" ({})", extra.join(", "));
        }
        if let Some(k) = num.filter(|k| *k > 0) {
            out += &format!(" {}", k + 1);
        }
        if !after.is_empty() {
            out += &format!(" – {}", after.join(" "));
        }
        return Some(out);
    }
    if let Some(p) = n.strip_prefix("player_story") {
        let t: Vec<&str> = p.split('_').collect();
        return Some(format!(
            "Player (story {}, outfit {})",
            t.first().unwrap_or(&""),
            t.get(1).unwrap_or(&"")
        ));
    }
    if !n.contains('_') || n.split('_').any(is_fit) {
        // ped part: words before the fit tag + sex + number
        let t: Vec<&str> = n.split('_').collect();
        let i = t.iter().position(|x| is_fit(x))?;
        let words = &t[..i];
        let sex = if t[i].starts_with('m') {
            "male"
        } else {
            "female"
        };
        let num = t
            .get(i + 1)
            .filter(|x| x.chars().all(|c| c.is_ascii_digit()))
            .map(|x| format!(" {x}"))
            .unwrap_or_default();
        let head = words
            .iter()
            .position(|w| *w != "cs" && *w != "u" && *w != "sc" && *w != "hc")
            .unwrap_or(0);
        return Some(format!("{} ({sex}){num}", cap(&words[head..].join(" "))));
    }
    // character prefixes
    let t: Vec<&str> = n.split('_').collect();
    match t[0] {
        "cs" | "sc" | "ig" => Some(cap(&t[1..].join(" "))),
        "hc" => Some(format!("{} heads", cap(&t[1..].join(" ")))),
        // u_<sex>_<age>_<name…>
        "u" | "a" | "s" if t.len() > 3 => Some(cap(&t[3..].join(" "))),
        _ => None,
    }
}

/// Label materials that have no colour texture (BCPack-only, or skin the
/// game builds at runtime): they render plain white, and the material list
/// says so instead of inventing a colour.
pub fn mark_missing_colour(model: &mut crate::glb::Model) {
    for m in model.materials.iter_mut() {
        if m.base.is_some()
            || matches!(
                m.class.as_str(),
                "decal" | "glass" | "window" | "emissive" | "water" | "car_paint"
            )
        {
            continue;
        }
        if m.gap.is_empty() || m.gap == "ok" {
            m.gap = "no colour texture".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(
            pretty_name("crr_iguana_green_m_00").unwrap(),
            "Iguana (green, male)"
        );
        assert_eq!(
            pretty_name("crm_dog_rottweiler_f_01").unwrap(),
            "Dog (rottweiler, female) 2"
        );
        assert_eq!(pretty_name("cs_ali").unwrap(), "Ali");
        assert_eq!(
            pretty_name("u_m_m_farm_monster_man").unwrap(),
            "Farm monster man"
        );
        assert_eq!(
            pretty_name("tshirtcrewneck_mr1_019_compression_cc_un_short_cull_00").unwrap(),
            "Tshirtcrewneck (male) 019"
        );
        assert_eq!(
            pretty_name("crr_iguana_green_m_00_body_00").unwrap(),
            "Iguana (green, male) – body"
        );
        assert!(pretty_name("prop_bench_01a").is_none());
    }
}
