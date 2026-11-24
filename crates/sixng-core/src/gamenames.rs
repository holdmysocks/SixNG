//! Names for two hashes in the world data, both from XML in `common.rpf`:
//!
//! * **Collision surface ids** (u16 at +0 of each 8-byte bound material):
//!   `18 + elf(name) % 32749`, where `name` is a physics material's `<name>`
//!   as written (upper case) and `elf` is the Unix ELF/PJW hash. Ids lie in
//!   18..=32766; 0..17 look reserved (only 0 occurs). The material list is in
//!   the material XML; its first 64 KiB block is not zlib and is not read, so
//!   most materials in it stay unnamed (the natural-ground ones are
//!   recovered by hash, see [`RECOVERED_SURFACE_NAMES`]). The deformable-terrain XML lists more
//!   materials (`<PhysicsMaterials>`), some of them from that block.
//! * **Road node street names** (u32 at +0x20 of a path node): joaat of the
//!   lower-cased key of an `<Item key="CustomEntity<n>_<m>">` in the location
//!   name table; the item's `<Name>` is the street's text label
//!   (`STREET_<…>_NAME`), looked up in the text tables of `data.rpf`.
use crate::export::Workspace;
use crate::meta7a::joaat;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Unix ELF / PJW hash.
pub fn elf(s: &[u8]) -> u32 {
    let mut h = 0u32;
    for &c in s {
        h = (h << 4).wrapping_add(c as u32);
        let g = h & 0xF000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}

/// Collision surface id of a physics material name (as written, upper case).
pub fn surface_id(name: &str) -> u16 {
    (18 + elf(name.as_bytes()) % 32749) as u16
}

/// Text and XML entries of `common.rpf`, decompressed, plus what inflates
/// of a zlib stream that directly follows an entry but was not carved (the
/// material XML's third block is one: it breaks off after a few hundred
/// bytes, and the part before the break holds more material names).
fn common_texts(ws: &Workspace) -> Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    let a = ws.resolve("common")?;
    let ix = ws.load_index(&a).context("common.rpf is not indexed (run `sixng index common`)")?;
    let starts: std::collections::HashSet<u64> = ix.entries.iter().flat_map(|e| e.streams.iter().map(|s| s[0])).collect();
    let mut f = std::fs::File::open(&a.path)?;
    let mut out = Vec::new();
    for e in ix.entries.iter().filter(|e| matches!(e.kind, crate::classify::Kind::Xml | crate::classify::Kind::Text)) {
        if let Ok(b) = crate::scan::read_entry(&a.path, e) {
            out.push(String::from_utf8_lossy(&b).into_owned());
        }
        let Some(last) = e.streams.last() else { continue };
        let next = (last[0] + last[1]).div_ceil(16) * 16;
        if starts.contains(&next) || f.seek(SeekFrom::Start(next)).is_err() {
            continue;
        }
        let mut buf = vec![0u8; 0x10000];
        let n = f.read(&mut buf).unwrap_or(0);
        if n < 2 || buf[0] != 0x78 || (u16::from(buf[0]) << 8 | u16::from(buf[1])) % 31 != 0 {
            continue;
        }
        let tail = inflate_prefix(&buf[..n]);
        if !tail.is_empty() {
            out.push(String::from_utf8_lossy(&tail).into_owned());
        }
    }
    Ok(out)
}

/// Inflate a zlib stream as far as it decodes (up to 64 KiB of output).
fn inflate_prefix(b: &[u8]) -> Vec<u8> {
    let mut d = flate2::Decompress::new(true);
    let mut out = Vec::with_capacity(0x10000);
    let mut at = 0;
    // Small steps, so the output before a broken spot is kept.
    while at < b.len() && out.len() < out.capacity() {
        let end = (at + 16).min(b.len());
        match d.decompress_vec(&b[at..end], &mut out, flate2::FlushDecompress::None) {
            Ok(flate2::Status::StreamEnd) | Err(_) => break,
            Ok(_) => at = d.total_in() as usize,
        }
    }
    out
}

fn tag_values<'a>(s: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    let mut v = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(open) {
        let t = &rest[i + open.len()..];
        let Some(j) = t.find(close) else { break };
        v.push(t[..j].trim());
        rest = &t[j + close.len()..];
    }
    v
}

fn is_material_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}

/// Natural-ground materials from the unread first block of the material XML,
/// recovered by hash: the material suffixes the readable categories share
/// (`DEFORM_`, `SUBMERGED_`, `TRACK_`, `STUNT_`, `VEG_`) were tried under
/// other prefixes, and `TERRAIN_` gives an unnamed id in `levels.rpf` for 31
/// of 76 (chance would give fewer than one). The ids sit next to other
/// ground surfaces in the bounds and cover about 21 % of collision
/// triangles. Names read from the XML win over these.
pub const RECOVERED_SURFACE_NAMES: &[&str] = &[
    "TERRAIN_CORAL",
    "TERRAIN_DIRT_COMPACT",
    "TERRAIN_DIRT_LOOSE",
    "TERRAIN_GRASS_DEAD",
    "TERRAIN_GRASS_LONG",
    "TERRAIN_GRASS_LUSH",
    "TERRAIN_GRASS_SHORT",
    "TERRAIN_GRAVEL_LARGE",
    "TERRAIN_GRAVEL_MEDIUM",
    "TERRAIN_GRAVEL_SMALL",
    "TERRAIN_GRAVEL_TRAP",
    "TERRAIN_LEAVES",
    "TERRAIN_LEAVES_DEAD",
    "TERRAIN_MIXED_DIRT_GRASS",
    "TERRAIN_MIXED_GRAVEL_DIRT",
    "TERRAIN_MIXED_GRAVEL_GRASS",
    "TERRAIN_MIXED_SAND_DIRT",
    "TERRAIN_MIXED_SAND_GRASS",
    "TERRAIN_MIXED_SAND_GRAVEL",
    "TERRAIN_MUD_HARD",
    "TERRAIN_MUD_SOFT",
    "TERRAIN_PINE_NEEDLES",
    "TERRAIN_ROCK_BRITTLE",
    "TERRAIN_SAND_COMPACT",
    "TERRAIN_SAND_LOOSE",
    "TERRAIN_SAND_SHELL",
    "TERRAIN_SAND_WET",
    "TERRAIN_SEAWEED",
    "TERRAIN_SOIL",
    "TERRAIN_TWIGS",
    "TERRAIN_WOODCHIPS",
];

/// Physics material names by surface id: `<name>` of the material XML
/// (entries with `<friction`) and the `<PhysicsMaterials>` lists of the
/// deformable-terrain XML, then [`RECOVERED_SURFACE_NAMES`] for ids still
/// without a name.
pub fn surface_names_from(texts: &[String]) -> BTreeMap<u16, String> {
    let mut out = BTreeMap::new();
    for s in texts {
        let mut names: Vec<&str> = Vec::new();
        if s.contains("<friction") {
            names.extend(tag_values(s, "<name>", "</name>"));
        }
        for block in tag_values(s, "<PhysicsMaterials>", "</PhysicsMaterials>") {
            names.extend(tag_values(block, "<Item>", "</Item>"));
        }
        for n in names.into_iter().filter(|n| is_material_name(n)) {
            out.entry(surface_id(n)).or_insert_with(|| n.to_string());
        }
    }
    for n in RECOVERED_SURFACE_NAMES {
        out.entry(surface_id(n)).or_insert_with(|| n.to_string());
    }
    out
}

/// Street keys and labels by node street hash: joaat(lower(key)) ->
/// (key, label) from the `<Item key=…><Name>…</Name>` entries.
pub fn street_keys_from(texts: &[String]) -> HashMap<u32, (String, String)> {
    let mut out = HashMap::new();
    for s in texts {
        let mut rest = s.as_str();
        while let Some(i) = rest.find("<Item key=\"") {
            let t = &rest[i + 11..];
            let Some(j) = t.find('"') else { break };
            let key = &t[..j];
            rest = &t[j..];
            let after = rest.trim_start_matches(|c| c != '>').get(1..).unwrap_or("").trim_start();
            if let Some(n) = after.strip_prefix("<Name>").and_then(|x| x.split_once("</Name>")).map(|x| x.0.trim()) {
                if !n.is_empty() {
                    out.entry(joaat(key)).or_insert_with(|| (key.to_string(), n.to_string()));
                }
            }
        }
    }
    out
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Add the names to the outputs of `collision`, `surfaces` and `paths` for
/// one archive (whichever exist): `collision/<archive>/surface_names.json`,
/// `names_by_surface_id` in `surfaces.json`, `name` per id in
/// `surface_categories.json`, and `street_key` / `street_label` /
/// `street_text` on the nodes of `paths/<archive>/graph.json` (the text from
/// the first `data.rpf` text table that has the label, when data.rpf is
/// indexed). Returns a summary.
pub fn annotate(ws: &Workspace, archive: &str, out: &Path, progress: crate::export::Progress) -> Result<Value> {
    let texts = common_texts(ws)?;
    let stem = archive.trim_end_matches(".rpf");
    let mut sum = serde_json::Map::new();

    let names = surface_names_from(&texts);
    let cdir = out.join("collision").join(stem);
    let sp = cdir.join("surfaces.json");
    if let Some(mut s) = read_json(&sp) {
        let tri: BTreeMap<String, u64> = s["triangles_by_surface_id"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0))).collect()).unwrap_or_default();
        let total: u64 = tri.values().sum();
        let by: BTreeMap<String, &String> = names.iter().map(|(k, v)| (format!("{k:04x}"), v)).filter(|(k, _)| tri.contains_key(k)).collect();
        let named_tri: u64 = by.keys().map(|k| tri[k]).sum();
        s["names_by_surface_id"] = json!(by);
        s["names_note"] = json!("surface id = 18 + elf(material name) % 32749 (Unix ELF/PJW hash of the name as written in common.rpf's material XML). The TERRAIN_ names are recovered by hash (that block of the XML is not read). Ids without a name belong to other materials in that block.");
        std::fs::write(&sp, serde_json::to_vec_pretty(&s)?)?;
        let all: BTreeMap<String, &String> = names.iter().map(|(k, v)| (format!("{k:04x}"), v)).collect();
        std::fs::write(cdir.join("surface_names.json"), serde_json::to_vec_pretty(&json!({"note": s["names_note"], "names": all}))?)?;
        sum.insert("surface_ids".into(), json!(tri.len()));
        sum.insert("surface_ids_named".into(), json!(by.len()));
        sum.insert("triangles_named".into(), json!(if total > 0 { named_tri as f64 / total as f64 } else { 0.0 }));
        sum.insert("material_names".into(), json!(names.len()));
    }
    let cp = cdir.join("surface_categories.json");
    if let Some(mut c) = read_json(&cp) {
        if let Some(o) = c["surfaces"].as_object_mut() {
            for (k, v) in o.iter_mut() {
                if let Some(n) = u16::from_str_radix(k, 16).ok().and_then(|id| names.get(&id)) {
                    v["name"] = json!(n);
                }
            }
        }
        std::fs::write(&cp, serde_json::to_vec_pretty(&c)?)?;
    }

    let gp = out.join("paths").join(stem).join("graph.json");
    if let Some(mut g) = read_json(&gp) {
        let keys = street_keys_from(&texts);
        let mut want: HashMap<u32, String> = HashMap::new();
        if let Some(ns) = g["nodes"].as_array() {
            for n in ns {
                if let Some((_, l)) = n["street"].as_str().and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(|h| keys.get(&h)) {
                    want.insert(joaat(l), l.clone());
                }
            }
        }
        let mut text: HashMap<u32, String> = HashMap::new();
        if let Ok(d) = ws.resolve("data") {
            if let Ok(ix) = ws.load_index(&d) {
                let (tables, _) = crate::text::read_all(&d, &ix.entries, &[], progress)?;
                for t in &tables {
                    for h in want.keys() {
                        if let (false, Some(s)) = (text.contains_key(h), t.strings.get(h)) {
                            text.insert(*h, crate::text::plain(s));
                        }
                    }
                }
            }
        }
        let (mut named, mut resolved, mut with_text) = (0usize, 0usize, 0usize);
        let mut distinct = std::collections::HashSet::new();
        if let Some(ns) = g["nodes"].as_array_mut() {
            for n in ns.iter_mut() {
                let Some(h) = n["street"].as_str().and_then(|h| u32::from_str_radix(h, 16).ok()).filter(|h| *h != 0) else { continue };
                named += 1;
                distinct.insert(h);
                if let Some((k, l)) = keys.get(&h) {
                    resolved += 1;
                    n["street_key"] = json!(k);
                    n["street_label"] = json!(l);
                    if let Some(t) = text.get(&joaat(l)) {
                        with_text += 1;
                        n["street_text"] = json!(t);
                    }
                }
            }
        }
        g["street_note"] = json!("street = joaat(lower-case location key), the key of an <Item key=\"CustomEntity<n>_<m>\"> in common.rpf's location name table; street_label is that item's <Name> (a text label), street_text its text from the first data.rpf text table that has it.");
        std::fs::write(&gp, serde_json::to_vec(&g)?)?;
        sum.insert("street_hashes".into(), json!(distinct.len()));
        sum.insert("named_nodes".into(), json!(named));
        sum.insert("nodes_with_label".into(), json!(resolved));
        sum.insert("nodes_with_text".into(), json!(with_text));
    }
    Ok(Value::Object(sum))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_id_test_vector() {
        assert_eq!(elf(b"ROAD_TARMAC"), 0x04D1_64F3);
        assert_eq!(surface_id("ROAD_TARMAC"), 0x1C31);
    }
    #[test]
    fn names_from_xml() {
        let t = vec![
            "<Item>\n<name>ROAD_TARMAC</name>\n<friction value=\"1\"/></Item>".to_string(),
            "<PhysicsMaterials>\n<Item>DEFORM_CLAY</Item>\n</PhysicsMaterials>".to_string(),
            "<Item key=\"CustomEntity34_5464\">\n  <Name>STREET_X_NAME</Name></Item>".to_string(),
        ];
        let s = surface_names_from(&t);
        assert_eq!(s[&0x1C31], "ROAD_TARMAC");
        assert_eq!(s[&surface_id("DEFORM_CLAY")], "DEFORM_CLAY");
        let k = street_keys_from(&t);
        assert_eq!(k[&joaat("customentity34_5464")], ("CustomEntity34_5464".to_string(), "STREET_X_NAME".to_string()));
    }
    #[test]
    fn recovered_surface_names() {
        // Ids seen in levels.rpf bounds that the readable XML does not name.
        for (n, id) in [("TERRAIN_SAND_WET", 0x2EA2), ("TERRAIN_GRASS_LUSH", 0x5DEF), ("TERRAIN_SAND_COMPACT", 0x3B23), ("TERRAIN_CORAL", 0x01B0)] {
            assert_eq!(surface_id(n), id, "{n}");
        }
        let ids: std::collections::BTreeSet<u16> = RECOVERED_SURFACE_NAMES.iter().map(|n| surface_id(n)).collect();
        assert_eq!(ids.len(), RECOVERED_SURFACE_NAMES.len());
        assert!(RECOVERED_SURFACE_NAMES.iter().all(|n| is_material_name(n)));
        // A name read from the XML keeps its id.
        let s = surface_names_from(&["<Item>\n<name>ROAD_TARMAC</name>\n<friction value=\"1\"/></Item>".to_string()]);
        assert_eq!(s[&0x1C31], "ROAD_TARMAC");
        assert_eq!(s[&0x2EA2], "TERRAIN_SAND_WET");
    }
}
