//! The strand colour set and parameter set each hair / fur asset names.
//!
//! An asset's registry record (PSO, struct `20e0f6ed`, type 30) carries
//! properties as a key with a list of name hashes. Two of them name the
//! asset's strand sets by the joaat of the set's lower-case name: 960033653
//! the colour set and 2259149558 the parameter set (the names of the game's
//! strand tables, `hairtables`). Cached in `<work>/index/hair_asset_sets.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

/// Registry property: the asset's strand colour set.
pub const PROP_COLOUR_SET: u32 = 960_033_653;
/// Registry property: the asset's strand parameter set.
pub const PROP_PARAM_SET: u32 = 2_259_149_558;

const CACHE_VERSION: u32 = 1;

/// The two sets of one asset (0 when the asset names none).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Sets {
    pub colour: u32,
    pub param: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AssetSets {
    version: u32,
    /// Asset GUID (16 hex digits) -> its sets.
    pub sets: BTreeMap<String, Sets>,
}

impl AssetSets {
    /// The sets of hair asset `guid`.
    pub fn of(&self, guid: u64) -> Option<Sets> {
        self.sets.get(&format!("{guid:016x}")).copied()
    }
}

/// Build the table from the registries of an archive.
pub fn build(a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> AssetSets {
    let first = |p: &std::collections::HashMap<u32, Vec<u32>>, k: u32| p.get(&k).and_then(|v| v.first()).copied().unwrap_or(0);
    let sets = crate::hairlink::registry_props(&a.path, all, &[PROP_COLOUR_SET, PROP_PARAM_SET])
        .into_iter()
        .map(|(g, p)| (format!("{g:016x}"), Sets { colour: first(&p, PROP_COLOUR_SET), param: first(&p, PROP_PARAM_SET) }))
        .filter(|(_, s)| s.colour != 0 || s.param != 0)
        .collect();
    AssetSets { version: CACHE_VERSION, sets }
}

/// The table (built on first use, then cached in the work directory).
pub fn get(work: &Path, a: &crate::game::ArchiveInfo, all: &[crate::scan::Entry]) -> &'static AssetSets {
    static T: OnceLock<AssetSets> = OnceLock::new();
    T.get_or_init(|| {
        let cache = work.join("index").join("hair_asset_sets.json");
        let ixp = crate::scan::index_path(&work.join("index"), &a.id);
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let Ok(b) = std::fs::read(&cache) {
            if let Ok(t) = serde_json::from_slice::<AssetSets>(&b) {
                if t.version == CACHE_VERSION && stamp(&cache) >= stamp(&ixp) {
                    return t;
                }
            }
        }
        let t = build(a, all);
        if !t.sets.is_empty() {
            if let Ok(b) = serde_json::to_vec(&t) {
                let _ = crate::atomic::write(&cache, b);
            }
        }
        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_guid_and_cache_round_trip() {
        let mut t = AssetSets { version: CACHE_VERSION, sets: BTreeMap::new() };
        t.sets.insert(format!("{:016x}", 0x0123_u64), Sets { colour: 7, param: 9 });
        let back: AssetSets = serde_json::from_slice(&serde_json::to_vec(&t).unwrap()).unwrap();
        assert_eq!(back.of(0x0123), Some(Sets { colour: 7, param: 9 }));
        assert_eq!(back.of(0x0124), None);
    }
}
