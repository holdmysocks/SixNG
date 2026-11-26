//! Ped parts in the content packs (`pack/<name>/<name>.rpf`).
//!
//! A few ped drawables are not in packs.rpf but in a content pack.
//! core_restofworld holds:
//! * the anatomical inserts: `bodynipplesgen_fr1_000_regular_ws00_full` and
//!   `bodygenitalsgen_<fit>_<nnn>_regular_ws00_full` (fits `fr1`, `fb1`,
//!   `fo1`, `mr1`), each with `_alt_00` to `_alt_02`;
//! * invisible stand-ins for underwear (`invisible_brabalcony_fr1_000`, …);
//! * some piercings, tights and underwear, and a second `mc1` body.
//!
//! Wardrobe items in packs.rpf name them by asset GUID
//! (`meta7a::asset_id` of the drawable name), like any other item. The
//! female body skins have no nipples painted in, and no lower-body skin has
//! genitals painted in: the game draws those parts with these inserts.
//!
//! An entry's offset is only unique inside its archive, so the copies kept
//! here carry [`KEY_BIT`] in `off`. That makes `off` a part key, not a file
//! offset; loading reads the entry's streams, and [`archive_of`] gives the
//! archive an entry belongs to by its id.

use crate::game::ArchiveInfo;
use crate::scan::Entry;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Set in the `off` of every entry of [`PackParts`]: no archive is that large.
pub const KEY_BIT: u64 = 1 << 62;

#[derive(Debug, Default)]
pub struct PackParts {
    /// The content packs that have ped drawables.
    pub archives: Vec<ArchiveInfo>,
    /// Their ped drawables, one per name (the largest copy, LOD0), with
    /// [`KEY_BIT`] in `off`.
    pub entries: Vec<Entry>,
    by_guid: HashMap<u64, usize>,
    by_name: HashMap<String, usize>,
}

impl PackParts {
    /// The drawable an asset GUID names.
    pub fn by_guid(&self, guid: u64) -> Option<&Entry> {
        self.by_guid.get(&guid).map(|&i| &self.entries[i])
    }

    /// A drawable by its lower-case name.
    pub fn by_name(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }

    /// The table from archives and their entries (`off` as in the index).
    pub fn from_entries(sets: Vec<(ArchiveInfo, Vec<Entry>)>) -> Self {
        let mut t = PackParts::default();
        for (a, list) in sets {
            let found = crate::outfit::dedupe(list.iter().filter(|e| e.rtype.as_deref() == Some("dr")).filter_map(|e| {
                let n = e.name.as_ref()?.to_ascii_lowercase();
                crate::outfit::is_ped_part(&n).then_some((n, e))
            }));
            if found.is_empty() {
                continue;
            }
            for (n, e) in found {
                if t.by_name.contains_key(&n) {
                    continue;
                }
                let mut e = e.clone();
                e.off |= KEY_BIT;
                t.by_guid.insert(crate::meta7a::asset_id(&n), t.entries.len());
                t.by_name.insert(n, t.entries.len());
                t.entries.push(e);
            }
            t.archives.push(a);
        }
        t
    }
}

static TABLE: OnceLock<PackParts> = OnceLock::new();

/// The ped drawables of the content packs that are indexed (read once from
/// their indexes in `<work>/index`). Empty when none is indexed.
pub fn get(game_root: &Path) -> &'static PackParts {
    TABLE.get_or_init(|| {
        let Some(work) = crate::names::work_dir() else { return PackParts::default() };
        let index = work.join("index");
        let sets = crate::game::discover(game_root)
            .into_iter()
            .filter(|a| a.id.starts_with("pack/") && a.container == "rpf9")
            .filter_map(|a| {
                let ix = crate::scan::load_index(&crate::scan::index_path(&index, &a.id)).ok()?;
                (ix.tool_version == crate::INDEX_VERSION).then_some((a, ix.entries))
            })
            .collect();
        PackParts::from_entries(sets)
    })
}

/// The table, when [`get`] has filled it.
pub fn loaded() -> Option<&'static PackParts> {
    TABLE.get()
}

/// The archive an entry belongs to: `a`, unless the entry's id names one of
/// the content packs of [`get`].
pub fn archive_of<'a>(a: &'a ArchiveInfo, e: &Entry) -> Cow<'a, ArchiveInfo> {
    let id = e.id.split('#').next().unwrap_or("").rsplit_once('@').map_or("", |x| x.0);
    if id.is_empty() || id.eq_ignore_ascii_case(&a.id) {
        return Cow::Borrowed(a);
    }
    match TABLE.get().and_then(|t| t.archives.iter().find(|x| x.id.eq_ignore_ascii_case(id))) {
        Some(x) => Cow::Owned(x.clone()),
        None => Cow::Borrowed(a),
    }
}

/// Whether a part key is a content-pack entry of [`get`].
pub fn is_pack_key(off: u64) -> bool {
    off & KEY_BIT != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(archive: &str, off: u64, name: &str, usize: u64) -> Entry {
        Entry { id: format!("{archive}@{off:x}"), off, csize: usize / 2, usize, streams: vec![[off, usize / 2, usize]], kind: crate::classify::Kind::RageObj, sub: String::new(), ext: "rsc".into(), name: Some(name.into()), rtype: Some("dr".into()), members: vec![] }
    }

    fn archive(id: &str) -> ArchiveInfo {
        ArchiveInfo { id: id.into(), path: id.into(), size: 0, container: "rpf9".into() }
    }

    #[test]
    fn guid_and_name_lookup_with_part_keys() {
        let pack = "pack/core_restofworld/core_restofworld.rpf";
        let n = "bodynipplesgen_fr1_000_regular_ws00_full";
        let t = PackParts::from_entries(vec![(archive(pack), vec![entry(pack, 0xb47010, n, 3712), entry(pack, 0xb43010, n, 14976), entry(pack, 0x1000, "cvar_am_kidney_01a", 10)])]);
        // one entry per name, the largest copy, keyed apart from packs.rpf offsets
        assert_eq!(t.entries.len(), 1);
        let e = t.by_guid(crate::meta7a::asset_id(n)).unwrap();
        assert_eq!(e.off, 0xb43010 | KEY_BIT);
        assert!(is_pack_key(e.off) && !is_pack_key(0xb43010));
        // loading reads the streams, which keep the file offset
        assert_eq!(e.streams[0][0], 0xb43010);
        assert_eq!(t.by_name(n).unwrap().id, format!("{pack}@b43010"));
        assert!(t.by_name("cvar_am_kidney_01a").is_none());
    }

    #[test]
    fn archive_of_keeps_the_main_archive() {
        let packs = archive("packs.rpf");
        let e = entry("packs.rpf", 0x63b7010, "hand_fr1_000_nude_cull", 10);
        assert_eq!(archive_of(&packs, &e).id, "packs.rpf");
    }
}
