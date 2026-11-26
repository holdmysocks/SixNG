//! The game's hair and fur strand tables, read from the game's own files:
//! colour ramps and colour sets, parameter ramps and parameter sets, and the
//! hair LOD presets. They are binary XML files (`RBF0`) of the packs
//! archive, found through its table of contents by their root element
//! (`rage__crsmHairStrandColorDatabase` ...), parsed once and cached in the
//! work folder.
//!
//! The output has the shape `hairtone::StrandData::from_json` takes
//! (`colour_ramps`, `colour_sets`, `parameter_ramps`, `parameter_sets`,
//! each keyed by name) plus `lod_presets` (preset name -> its fields).
use anyhow::{anyhow, bail, ensure, Result};
use serde_json::{json, Map, Value};

/// One element of a binary XML tree.
#[derive(Debug, Default, Clone)]
pub struct Elem {
    pub name: String,
    pub attrs: Vec<(String, Attr)>,
    pub children: Vec<Elem>,
    pub text: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum Attr {
    Int(i64),
    Bool(bool),
    Float(f64),
    Vec3([f32; 3]),
    Str(String),
}

impl Elem {
    pub fn attr(&self, n: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.0 == n).map(|a| &a.1)
    }
    pub fn kids<'a>(&'a self, n: &'a str) -> impl Iterator<Item = &'a Elem> + 'a {
        self.children.iter().filter(move |c| c.name == n)
    }
    fn text_str(&self) -> Option<String> {
        self.text.as_ref().map(|t| String::from_utf8_lossy(t).trim_end_matches('\0').to_string())
    }
    /// Ten (x, y) pairs stored as 80 bytes of text.
    fn curve(&self) -> Option<Value> {
        let t = self.text.as_ref().filter(|t| t.len() == 80)?;
        let v: Vec<f32> = t.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        Some(json!(v.chunks(2).map(|p| [p[0], p[1]]).collect::<Vec<_>>()))
    }
}

impl Attr {
    fn json(&self) -> Value {
        match self {
            Attr::Int(i) => json!(i),
            Attr::Bool(b) => json!(b),
            Attr::Float(f) => json!(f),
            Attr::Vec3(v) => json!(v),
            Attr::Str(s) => json!(s),
        }
    }
    fn str(&self) -> Option<&str> {
        if let Attr::Str(s) = self { Some(s) } else { None }
    }
}

/// Parse an `RBF0` binary XML file. Tag word: name index | type << 12;
/// type 0 element (then 6 bytes, the last an attribute count), 1 i64,
/// 2 true, 3 false, 4 f64, 5 three f32, 6 string; 0xfffd text (u32 length),
/// 0xfffb skip 2 bytes, 0xffff close. A name is defined at its first use.
pub fn parse_rbf(d: &[u8]) -> Result<Elem> {
    ensure!(d.get(..4) == Some(b"RBF0"), "not an RBF0 file");
    let mut p = 4usize;
    let mut names: Vec<String> = Vec::new();
    let mut stack: Vec<Elem> = Vec::new();
    let u16at = |p: usize| d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| anyhow!("RBF cut short at {p:#x}"));
    let take = |p: usize, n: usize| d.get(p..p + n).ok_or_else(|| anyhow!("RBF cut short at {p:#x}"));
    while p < d.len() {
        let t = u16at(p)?;
        p += 2;
        match t {
            0xffff => {
                let e = stack.pop().ok_or_else(|| anyhow!("RBF: close without an open element"))?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(e),
                    None => return Ok(e),
                }
            }
            0xfffd => {
                let n = u32::from_le_bytes(take(p, 4)?.try_into().unwrap()) as usize;
                p += 4;
                let text = take(p, n)?.to_vec();
                p += n;
                stack.last_mut().ok_or_else(|| anyhow!("RBF: text outside an element"))?.text = Some(text);
            }
            0xfffb => p += 2,
            _ => {
                let idx = (t & 0xfff) as usize;
                if idx == names.len() {
                    let n = u16at(p)? as usize;
                    names.push(String::from_utf8_lossy(take(p + 2, n)?).into_owned());
                    p += 2 + n;
                }
                let name = names.get(idx).cloned().ok_or_else(|| anyhow!("RBF: name index {idx} not defined"))?;
                let v = match t >> 12 {
                    0 => {
                        p += 6;
                        stack.push(Elem { name, ..Default::default() });
                        continue;
                    }
                    1 => {
                        let v = i64::from_le_bytes(take(p, 8)?.try_into().unwrap());
                        p += 8;
                        Attr::Int(v)
                    }
                    2 => Attr::Bool(true),
                    3 => Attr::Bool(false),
                    4 => {
                        let v = f64::from_le_bytes(take(p, 8)?.try_into().unwrap());
                        p += 8;
                        Attr::Float(v)
                    }
                    5 => {
                        let b = take(p, 12)?;
                        p += 12;
                        Attr::Vec3([0, 1, 2].map(|i| f32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap())))
                    }
                    6 => {
                        let n = u16at(p)? as usize;
                        let s = String::from_utf8_lossy(take(p + 2, n)?).into_owned();
                        p += 2 + n;
                        Attr::Str(s)
                    }
                    k => bail!("RBF: value type {k} at {:#x}", p - 2),
                };
                stack.last_mut().ok_or_else(|| anyhow!("RBF: value outside an element"))?.attrs.push((name, v));
            }
        }
    }
    bail!("RBF: file ends inside an element")
}

/// Root elements of the files read here.
pub const ROOTS: [&str; 5] = ["rage__crsmHairStrandColorDatabase", "rage__crsmHairStrandColorPresetDatabase", "rage__crsmHairStrandMaterialDatabase", "rage__crsmHairStrandMaterialPresetDatabase", "rage__crsmHairLodPresetDatabase"];

fn items(root: &Elem) -> impl Iterator<Item = &Elem> {
    root.children.first().into_iter().flat_map(|l| l.children.iter())
}

fn key(it: &Elem) -> Option<String> {
    it.attr("key").and_then(|a| a.str()).map(|s| s.to_string())
}

fn hash_hex(name: &str) -> String {
    format!("{:08x}", crate::meta7a::joaat(&name.to_ascii_lowercase()))
}

/// A number stored as an attribute or as a child element's `value`.
fn number(it: &Elem, k: &str) -> Option<Value> {
    it.attr(k).map(|a| a.json()).or_else(|| it.kids(k).next().and_then(|e| e.attr("value")).map(|a| a.json()))
}

/// The tables as JSON, from the parsed files by root element name.
pub fn tables_json(files: &[Elem]) -> Value {
    let root = |n: &str| files.iter().find(|e| e.name == n);
    let mut cols = Map::new();
    for it in root(ROOTS[0]).into_iter().flat_map(items) {
        let Some(k) = key(it) else { continue };
        let stops: Vec<Value> = (0..4).map(|i| it.attr(&format!("Color{i}")).map_or(Value::Null, |a| a.json())).collect();
        cols.insert(k.clone(), json!({"hash": hash_hex(&k), "colours": stops, "curve": it.kids("ColorBezierPoints").next().and_then(|e| e.curve()), "average": it.attr("Average").map(|a| a.json())}));
    }
    let names = |it: &Elem, list: &str| -> Vec<Value> { it.kids(list).next().map_or(Vec::new(), |c| c.children.iter().map(|x| x.text_str().map_or(Value::Null, Value::String)).collect()) };
    let mut csets = Map::new();
    for it in root(ROOTS[1]).into_iter().flat_map(items) {
        let Some(k) = key(it) else { continue };
        csets.insert(k.clone(), json!({"hash": hash_hex(&k), "colours": names(it, "Colors"), "average": it.attr("Average").map(|a| a.json())}));
    }
    let mut mats = Map::new();
    for it in root(ROOTS[2]).into_iter().flat_map(items) {
        let Some(k) = key(it) else { continue };
        let mut m = Map::new();
        m.insert("hash".into(), json!(hash_hex(&k)));
        for f in ["Thickness", "Roughness", "Translucency", "BinormalRoughness"] {
            m.insert(f.into(), number(it, f).unwrap_or(Value::Null));
            m.insert(format!("{f}Curve"), it.kids(&format!("{f}BezierPoints")).next().and_then(|e| e.curve()).unwrap_or(Value::Null));
        }
        mats.insert(k, Value::Object(m));
    }
    let mut msets = Map::new();
    for it in root(ROOTS[3]).into_iter().flat_map(items) {
        let Some(k) = key(it) else { continue };
        let mut m = Map::new();
        m.insert("hash".into(), json!(hash_hex(&k)));
        m.insert("materials".into(), json!(names(it, "Materials")));
        for (n, a) in &it.attrs {
            if n != "key" {
                m.insert(n.clone(), a.json());
            }
        }
        msets.insert(k, Value::Object(m));
    }
    let mut lods = Map::new();
    for it in root(ROOTS[4]).into_iter().flat_map(items) {
        let Some(k) = key(it) else { continue };
        let mut m = Map::new();
        for (n, a) in &it.attrs {
            if n != "key" {
                m.insert(n.clone(), a.json());
            }
        }
        for c in &it.children {
            if let Some(v) = c.attr("value") {
                m.insert(c.name.clone(), v.json());
            }
        }
        m.insert("hash".into(), json!(hash_hex(&k)));
        lods.insert(k, Value::Object(m));
    }
    json!({"colour_ramps": cols, "colour_sets": csets, "parameter_ramps": mats, "parameter_sets": msets, "lod_presets": lods})
}

const CACHE_VERSION: u64 = 1;

/// The tables of the archive `a` (the packs archive), from the cache in the
/// work folder's index or read now through the archive's table of contents.
pub fn load_or_build(ws: &crate::export::Workspace, a: &crate::game::ArchiveInfo) -> Result<Value> {
    let cache = ws.work.join("index").join("hair_tables.json");
    if let Some(v) = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(|v| v["version"].as_u64() == Some(CACHE_VERSION)) {
        return Ok(v);
    }
    let mut cipher = crate::gamecipher::open(&crate::export::game_root(a))?;
    let mut f = std::fs::File::open(&a.path)?;
    let toc = crate::rpf9toc::open_toc(cipher.as_mut(), &mut f, 0)?;
    let entries = toc.files()?;
    let mut found: Vec<Elem> = Vec::new();
    // plain files of a few hundred KiB at most; the root element names them
    for e in entries.iter().filter(|e| e.resource.is_none() && e.unpacked > 0 && e.unpacked < 1 << 20) {
        let Ok((data, _)) = crate::rpf9toc::read_file(cipher.as_mut(), &mut f, 0, e) else { continue };
        if data.get(..4) != Some(b"RBF0") {
            continue;
        }
        let Ok(root) = parse_rbf(&data) else { continue };
        if ROOTS.contains(&root.name.as_str()) && !found.iter().any(|x| x.name == root.name) {
            found.push(root);
            if found.len() == ROOTS.len() {
                break;
            }
        }
    }
    ensure!(!found.is_empty(), "no hair strand table found in {}", a.id);
    let mut v = tables_json(&found);
    v["version"] = json!(CACHE_VERSION);
    v["found"] = json!(found.iter().map(|e| e.name.clone()).collect::<Vec<_>>());
    crate::atomic::write(&cache, serde_json::to_vec(&v)?)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(idx: u16, ty: u16) -> [u8; 2] {
        (idx | ty << 12).to_le_bytes()
    }

    #[test]
    fn rbf_tree_and_tables() {
        // <rage__crsmHairLodPresetDatabase><Presets><Item key="Default" DistanceCutoff=100.0/></Presets></...>
        let mut d = b"RBF0".to_vec();
        let name = |d: &mut Vec<u8>, s: &str| {
            d.extend((s.len() as u16).to_le_bytes());
            d.extend(s.as_bytes());
        };
        d.extend(tag(0, 0));
        name(&mut d, "rage__crsmHairLodPresetDatabase");
        d.extend([0u8; 6]);
        d.extend(tag(1, 0));
        name(&mut d, "Presets");
        d.extend([0u8; 6]);
        d.extend(tag(2, 0));
        name(&mut d, "Item");
        d.extend([0u8; 6]);
        d.extend(tag(3, 6));
        name(&mut d, "key");
        name(&mut d, "Default");
        d.extend(tag(4, 4));
        name(&mut d, "DistanceCutoff");
        d.extend(100f64.to_le_bytes());
        d.extend([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        let root = parse_rbf(&d).unwrap();
        assert_eq!(root.name, ROOTS[4]);
        let v = tables_json(&[root]);
        assert_eq!(v["lod_presets"]["Default"]["DistanceCutoff"], json!(100.0));
        assert_eq!(v["lod_presets"]["Default"]["hash"], json!(hash_hex("Default")));
    }
}
