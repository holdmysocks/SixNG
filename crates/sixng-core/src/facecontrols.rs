//! Facial control names.
//!
//! Facial clips animate the face through float tracks of kind
//! [`FACIAL_TRANSLATION`] (joaat `facialtranslation`); the track id is the
//! joaat of the control's name. The names are not stored anywhere in the
//! game files: they were recovered by matching joaat hashes of FACS-style
//! word combinations (`face_[l_|r_]<region>_<action>`) against the 91 ids
//! the facial clips use. Every left/right name also has its mirror, which
//! is a check on each find. 12 ids remain unnamed and show as hex.
//!
//! The face rigs read these controls as pseudo-bone channels (track types
//! 4 and 5: a control's value and, for the 2-D controls such as `face_jaw`,
//! its second axis) to pose the teeth and tongue bones.
//!
//! The head expression network (see [`crate::facegraph`]) is driven from
//! these controls by the face engine setup, which is not in the shipped
//! archives, so which network input a control feeds is not known.

/// joaat("facialtranslation"): the track kind of facial control values.
pub const FACIAL_TRANSLATION: u32 = 0x364d3bdb;

/// The recovered control names, sorted.
pub const NAMES: &[&str] = &[
    "face_jaw",
    "face_jaw_forwardback",
    "face_l_brow_in",
    "face_l_cheek_raiser",
    "face_l_eye",
    "face_l_eye_squint",
    "face_l_eyelid",
    "face_l_innerbrow_raiserlowerer",
    "face_l_lipcorner_depressor",
    "face_l_lipcorner_dimpler",
    "face_l_lipcorner_puller",
    "face_l_lipcorner_stretcher",
    "face_l_lowerlip_depressor",
    "face_l_lowerlip_funnel",
    "face_l_lowerlip_press",
    "face_l_lowerlip_pucker",
    "face_l_lowerlip_suck",
    "face_l_lowerlip_suckblow",
    "face_l_lowerlip_tightener",
    "face_l_lowerlip_together_part",
    "face_l_lowerlipchin_raiser",
    "face_l_mouth_suckpuff",
    "face_l_neck_stretcher",
    "face_l_nose_dilatorcompressor",
    "face_l_nose_wrinkler",
    "face_l_outerbrow_raiser",
    "face_l_upperlip_funnel",
    "face_l_upperlip_press",
    "face_l_upperlip_pucker",
    "face_l_upperlip_raiser",
    "face_l_upperlip_suck",
    "face_l_upperlip_suckblow",
    "face_l_upperlip_tightener",
    "face_l_upperlip_together_part",
    "face_l_upperlipchin_raiser",
    "face_mouth",
    "face_neck_swallow",
    "face_r_brow_in",
    "face_r_cheek_raiser",
    "face_r_eye",
    "face_r_eye_squint",
    "face_r_eyelid",
    "face_r_innerbrow_raiserlowerer",
    "face_r_lipcorner_depressor",
    "face_r_lipcorner_dimpler",
    "face_r_lipcorner_puller",
    "face_r_lipcorner_stretcher",
    "face_r_lowerlip_depressor",
    "face_r_lowerlip_funnel",
    "face_r_lowerlip_press",
    "face_r_lowerlip_pucker",
    "face_r_lowerlip_suck",
    "face_r_lowerlip_suckblow",
    "face_r_lowerlip_tightener",
    "face_r_lowerlip_together_part",
    "face_r_lowerlipchin_raiser",
    "face_r_mouth_suckpuff",
    "face_r_neck_stretcher",
    "face_r_nose_dilatorcompressor",
    "face_r_nose_wrinkler",
    "face_r_outerbrow_raiser",
    "face_r_upperlip_funnel",
    "face_r_upperlip_press",
    "face_r_upperlip_pucker",
    "face_r_upperlip_raiser",
    "face_r_upperlip_suck",
    "face_r_upperlip_suckblow",
    "face_r_upperlip_tightener",
    "face_r_upperlip_together_part",
    "face_r_upperlipchin_raiser",
    "face_tongue_root",
    "face_tongueaim",
    "face_tongueextension",
    "face_tonguefold",
    "face_tonguepress",
    "face_tonguethickness",
    "face_tonguetipcurl",
    "face_tonguetwist",
    "face_tonguewidth",
];

/// The name of a facial control id, when it is known.
pub fn name(id: u32) -> Option<&'static str> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_ID: OnceLock<HashMap<u32, &'static str>> = OnceLock::new();
    BY_ID.get_or_init(|| NAMES.iter().map(|n| (crate::meta7a::joaat(n), *n)).collect()).get(&id).copied()
}

/// The name of a facial control id, or its hash in hex.
pub fn label(id: u32) -> String {
    name(id).map_or_else(|| format!("{id:08x}"), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids() {
        // ids as the facial clips store them
        assert_eq!(name(0xffa72c3e), Some("face_jaw"));
        assert_eq!(name(0xee13ea8b), Some("face_mouth"));
        assert_eq!(name(0x56833078), Some("face_l_lowerlip_depressor"));
        assert_eq!(name(0x960e1ee5), Some("face_tongueextension"));
        assert_eq!(name(0x0375a088), None);
        assert_eq!(label(0x0375a088), "0375a088");
        assert_eq!(crate::meta7a::joaat("facialtranslation"), FACIAL_TRANSLATION);
    }

    #[test]
    fn sides_mirror() {
        assert_eq!(NAMES.len(), 79);
        for n in NAMES {
            let m = if let Some(r) = n.strip_prefix("face_l_") { format!("face_r_{r}") } else if let Some(r) = n.strip_prefix("face_r_") { format!("face_l_{r}") } else { continue };
            assert!(NAMES.contains(&m.as_str()), "{n} has no mirror");
        }
    }
}
