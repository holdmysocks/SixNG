//! A character's full set of parts: every drawable its outfits use, with
//! each texture and colour variant and the outfits that wear it.
//!
//! The game ties a character to its parts in three steps, all read by
//! `pedlook`:
//!
//! 1. The outfit list (packs.rpf PSO, struct `a5ea6310`) maps joaat(model
//!    name) to the model's outfits: variation ids in order.
//! 2. The model's wardrobe resource holds those variations. An outfit
//!    variation lists its items and includes other variations by id: the
//!    head set, the body set, brows and lashes, a body-shape preset.
//! 3. Each item names its drawable by asset GUID (the name id of the
//!    drawable) and carries its own textures, tint, colour and garment
//!    preset. Two items on one drawable are two variants of that part.
//!
//! The wardrobe can hold variations that no listed outfit reaches; their
//! parts are listed too (`other`), so the set is every part the wardrobe
//! names, not only those of the listed outfits.
//!
//! Kinds are read from the drawable's item word: `body` (head, eyes, teeth,
//! skin), `hair` (hair, brows, lashes, beards: strand placeholders),
//! `clothing` (tops, outer wear, bottoms, one-piece garments, shoes) and
//! `accessory` (the rest: watches, jewellery, glasses, hats, nails …). The
//! wardrobe stores no such grouping itself.
use crate::pedlook::{Look, LookPart, Outfit};
use crate::wardrobe::PartTint;
use serde::Serialize;

/// One outfit (or other wardrobe variation) of the set.
#[derive(Debug, Clone, Serialize)]
pub struct SetOutfit {
    /// Variation id (hex).
    pub id: String,
    pub label: String,
    /// A listed outfit (false: another variation of the wardrobe).
    pub listed: bool,
    /// The variations it is made of (hex).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_shape: Option<String>,
    /// Items that resolved to no drawable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    /// Items left out on purpose, with why (`Outfit::not_shown`): they are
    /// not missing, so they do not make the set incomplete.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_shown: Vec<String>,
}

/// One wardrobe item on a drawable: a texture / colour variant of the part.
#[derive(Debug, Clone, Serialize)]
pub struct SetVariant {
    /// Wardrobe item id (hex).
    pub item: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<PartTint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour_tag: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preset_colours: Vec<Option<[u8; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_preset: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tex_remap: Vec<(String, String)>,
    /// Indices into `PartSet::outfits` of the outfits that wear it.
    pub outfits: Vec<usize>,
}

/// One drawable of the set.
#[derive(Debug, Clone, Serialize)]
pub struct SetPart {
    /// body, hair, clothing or accessory.
    pub kind: &'static str,
    pub slot: String,
    pub drawable: String,
    /// A strand-hair placeholder: the hair itself is the groom of the
    /// variant's GUID.
    pub strands: bool,
    /// How the drawable was found (`LookPart::found_by`), first variant.
    pub found_by: &'static str,
    /// Item-less parts (the default skin, the head's eyes) have one variant
    /// with an empty item id.
    pub variants: Vec<SetVariant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartSet {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    pub resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    pub outfits: Vec<SetOutfit>,
    /// Ordered by kind, then slot, then drawable.
    pub parts: Vec<SetPart>,
    /// Every item of every outfit and wardrobe variation resolved to a drawable.
    pub complete: bool,
    pub unresolved: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The kind of an outfit slot (see the module notes).
pub fn kind_of(slot: &str) -> &'static str {
    match slot {
        "head" | "eyes" | "teeth" | "body upper" | "body lower" => "body",
        "hair" | "brows" | "lashes" | "beard" | "body hair" => "hair",
        "top" | "outer" | "bottom" | "feet" | "full" => "clothing",
        _ => "accessory",
    }
}

fn kind_rank(k: &str) -> usize {
    ["body", "hair", "clothing", "accessory"].iter().position(|x| *x == k).unwrap_or(4)
}

fn variant(p: &LookPart) -> SetVariant {
    SetVariant {
        item: p.item.clone(),
        guid: p.guid.clone(),
        category: p.category.clone(),
        tint: p.tint.clone(),
        colour: p.colour,
        colour_tag: p.colour_tag,
        preset: p.preset.clone(),
        preset_colours: p.preset_colours.clone(),
        hair_preset: p.hair_preset.clone(),
        tex_remap: p.tex_remap.clone(),
        outfits: vec![],
    }
}

/// The full set of a look: its outfits, then the wardrobe's other
/// variations, merged into one list of drawables.
pub fn part_set(look: &Look, display: Option<String>) -> PartSet {
    let all: Vec<(&Outfit, bool)> = look.outfits.iter().map(|o| (o, true)).chain(look.other_variations.iter().map(|o| (o, false))).collect();
    let outfits: Vec<SetOutfit> = all.iter().map(|(o, listed)| SetOutfit { id: o.id.clone(), label: o.label.clone(), listed: *listed, variations: o.variations.clone(), body_shape: o.body_shape.clone(), unresolved: o.unresolved.clone(), not_shown: o.not_shown.clone() }).collect();
    let mut parts: Vec<SetPart> = Vec::new();
    for (oi, (o, _)) in all.iter().enumerate() {
        for p in &o.parts {
            let i = match parts.iter().position(|x| x.drawable == p.drawable) {
                Some(i) => i,
                None => {
                    parts.push(SetPart { kind: kind_of(&p.slot), slot: p.slot.clone(), drawable: p.drawable.clone(), strands: p.strands, found_by: p.found_by, variants: vec![] });
                    parts.len() - 1
                }
            };
            let vs = &mut parts[i].variants;
            // one variant per item; a hair worn in another state (under a
            // hat) is the same item
            let k = match vs.iter().position(|v| v.item == p.item) {
                Some(k) => k,
                None => {
                    vs.push(variant(p));
                    vs.len() - 1
                }
            };
            if !vs[k].outfits.contains(&oi) {
                vs[k].outfits.push(oi);
            }
        }
    }
    parts.sort_by(|a, b| (kind_rank(a.kind), &a.slot, &a.drawable).cmp(&(kind_rank(b.kind), &b.slot, &b.drawable)));
    let unresolved: usize = outfits.iter().map(|o| o.unresolved.len()).sum();
    PartSet { model: look.model.clone(), display, resource: look.resource.clone(), fit: look.fit.clone(), outfits, parts, complete: unresolved == 0, unresolved, note: look.note.clone() }
}

/// The full set of a character model (`cs_mandy`), with the name the game
/// shows for it.
///
/// A model the outfit data does not cover and that cannot stand in its own
/// parts (a cutscene head, a hoodie and boots with no trousers) still has a
/// set: its own drawables, with no outfits (`own_set`).
pub fn of(game_root: &std::path::Path, a: &crate::game::ArchiveInfo, ix: &crate::scan::ArchiveIndex, model: &str) -> Result<PartSet, String> {
    let display = crate::pedlook::model_names(game_root).ok().and_then(|n| n.display.get(&crate::pedlook::model_hash(&model.to_ascii_lowercase())).cloned());
    match crate::pedlook::look(game_root, a, ix, model) {
        Ok(look) => Ok(part_set(&look, display)),
        Err(e) => {
            let names: Vec<String> = crate::outfit::outfit_options(ix, model).into_iter().map(|o| o.name).collect();
            own_set(model, display, &a.id, names).ok_or(e)
        }
    }
}

/// The set of a model with no outfit data: each of its own drawables once,
/// worn by no outfit. Not `complete`: nothing says these parts make a whole
/// character. None when the model has no drawable of its own.
pub fn own_set(model: &str, display: Option<String>, archive: &str, names: Vec<String>) -> Option<PartSet> {
    if names.is_empty() {
        return None;
    }
    let mut parts: Vec<SetPart> = names
        .into_iter()
        .map(|n| {
            let slot = crate::pedlook::slot_of(&n);
            let strands = crate::pedlook::is_strand_word(crate::pedlook::item_word(&n));
            SetPart { kind: kind_of(&slot), slot, drawable: n, strands, found_by: "own", variants: vec![SetVariant { item: String::new(), guid: None, category: None, tint: None, colour: None, colour_tag: None, preset: None, preset_colours: vec![], hair_preset: None, tex_remap: vec![], outfits: vec![] }] }
        })
        .collect();
    parts.sort_by(|a, b| (kind_rank(a.kind), &a.slot, &a.drawable).cmp(&(kind_rank(b.kind), &b.slot, &b.drawable)));
    let fit = parts.iter().find(|p| p.slot == "head").and_then(|h| h.drawable.split('_').find(|t| crate::outfit::is_fit(t)).map(str::to_string));
    Some(PartSet {
        model: model.to_string(),
        display,
        resource: format!("{archive} (own parts: the outfit data does not cover this model)"),
        fit,
        outfits: vec![],
        parts,
        complete: false,
        unresolved: 0,
        note: Some("The outfit data does not cover this model, and its own parts are not a whole character: these are the drawables of its own, worn by no outfit.".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(slot: &str, drawable: &str, item: &str) -> LookPart {
        LookPart { slot: slot.into(), drawable: drawable.into(), item: item.into(), strands: false, guid: None, tint: None, colour: None, colour_tag: None, hair_preset: None, layer_set: None, preset_colours: vec![], preset: None, tex_remap: vec![], skins: vec![], found_by: "asset id (name)", category: None, layer: None, hides: vec![], hides_sub: vec![], hair_state: None, state_guids: vec![] }
    }

    fn outfit(id: &str, parts: Vec<LookPart>, unresolved: Vec<String>) -> Outfit {
        Outfit { id: id.into(), outfit_type: String::new(), scale: 0.0, label: id.into(), body_shape: None, parts, strand_colours: vec![], unresolved, not_shown: vec![], variations: vec![id.into()] }
    }

    #[test]
    fn merges_drawables_and_items() {
        let look = Look {
            model: "cs_x".into(),
            resource: "packs.rpf@1".into(),
            fit: Some("fr1".into()),
            outfits: vec![
                outfit("a", vec![part("head", "x_head_fr1_full", "h"), part("bottom", "shortsjeans_fr1_018_full", "1"), part("watch", "watchsmart_fr1_000_full", "w")], vec![]),
                outfit("b", vec![part("head", "x_head_fr1_full", "h"), part("bottom", "shortsjeans_fr1_018_full", "2")], vec![]),
            ],
            other_variations: vec![outfit("c", vec![part("top", "tanktop_fr1_001_full", "t")], vec!["9 (shoes: its model is not found)".into()])],
            note: None,
        };
        let s = part_set(&look, Some("X".into()));
        assert_eq!(s.outfits.len(), 3);
        assert!(!s.outfits[2].listed);
        assert_eq!(s.parts.len(), 4);
        assert_eq!(s.parts.iter().map(|p| p.kind).collect::<Vec<_>>(), ["body", "clothing", "clothing", "accessory"]);
        let jeans = s.parts.iter().find(|p| p.drawable.starts_with("shortsjeans")).unwrap();
        assert_eq!(jeans.variants.len(), 2);
        assert_eq!(jeans.variants[1].outfits, [1]);
        assert_eq!(s.parts[0].variants[0].outfits, [0, 1]);
        assert!(!s.complete && s.unresolved == 1);
    }

    #[test]
    fn own_set_lists_the_models_own_drawables() {
        let names = vec!["cs_roxy_hoodiezip_fr1_000_slim_h_hd_mid_full".to_string(), "cs_roxy_head_fr1_cau_000_m_full".to_string(), "cs_roxy_bootsankle_fr1_000_mid_hh_full".to_string()];
        let s = own_set("cs_roxy", None, "packs", names).unwrap();
        assert!(s.outfits.is_empty() && !s.complete && s.unresolved == 0);
        assert_eq!(s.parts.len(), 3);
        assert_eq!(s.parts[0].kind, "body");
        assert_eq!(s.fit.as_deref(), Some("fr1"));
        assert!(s.parts.iter().all(|p| p.variants.len() == 1 && p.variants[0].outfits.is_empty()));
        assert!(own_set("cs_nobody", None, "packs", vec![]).is_none());
    }
}
