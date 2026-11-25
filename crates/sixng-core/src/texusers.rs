//! Which materials use a texture (`sixng texture-users`): every material of
//! every model (drawables, fragments, dictionary members) with its texture
//! slots, so a texture can be traced to the material it belongs to and from
//! there to the material's other maps (base colour -> normal, packed
//! roughness/metal …).
//!
//! Written to `<work>/index/texture_users.jsonl.gz`: a header line, one line
//! per distinct material (`{"m": [shader, [[param, hash64], …]]}`; the same
//! material in several models or LOD copies is stored once) and one line per
//! model (`{"id", "name", "mats": [material index, …]}`).
use crate::export::{Progress, Workspace};
use crate::game::ArchiveInfo;
use crate::resource::Resource;
use crate::classify::Kind;
use crate::scan::Entry;
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn path(ws: &Workspace) -> PathBuf {
    ws.index_dir().join("texture_users.jsonl.gz")
}

/// A material: shader (name where known, else hash) and its texture slots
/// (parameter name where known, else hash; texture hash64).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Material {
    pub shader: String,
    pub slots: Vec<(String, u64)>,
}

/// Archives with drawables, fragments or drawable dictionaries that are
/// indexed (all of them unless `only` names some).
pub fn archives(ws: &Workspace, only: &[String]) -> Result<Vec<(ArchiveInfo, Vec<Entry>)>> {
    let list: Vec<ArchiveInfo> = if only.is_empty() { ws.archives().into_iter().filter(|a| ws.is_indexed(a)).collect() } else { only.iter().map(|q| ws.resolve(q)).collect::<Result<_>>()? };
    let mut out = Vec::new();
    for a in list {
        let ix = ws.load_index(&a)?;
        let sel: Vec<Entry> = ix.entries.into_iter().filter(|e| e.kind == Kind::RageObj && matches!(e.rtype.as_deref(), Some("dr") | Some("ft") | Some("dd"))).collect();
        if !sel.is_empty() {
            out.push((a, sel));
        }
    }
    Ok(out)
}

/// Scan the models and write the index. Returns a summary.
pub fn build(ws: &Workspace, archives: &[(ArchiveInfo, Vec<Entry>)], progress: Progress) -> Result<Value> {
    let total: u64 = archives.iter().flat_map(|a| &a.1).map(|e| if e.id.contains('#') { 1 } else { e.members.len().max(1) as u64 }).sum();
    let done = AtomicU64::new(0);
    let mut models: Vec<(String, String, Vec<Material>)> = Vec::new();
    for (a, entries) in archives {
        let entries: Vec<Entry> = entries
            .iter()
            .flat_map(|e| if e.members.is_empty() || e.id.contains('#') { vec![e.clone()] } else { e.members.iter().filter_map(|n| crate::export::member_entry(e, n).ok()).collect() })
            .collect();
        let resolver = crate::export::ModelResolver::new(&entries);
        let m: Vec<(String, String, Vec<Material>)> = entries
            .par_iter()
            .filter_map(|e| {
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 512 == 0 {
                    progress(&e.id, d, total);
                }
                let (source, member) = resolver.resolve(&entries, &e.id).ok()?;
                let r = if member.is_some() { Resource::load(&a.path, source) } else { Resource::load_sys(&a.path, source) }.ok()?;
                let shaders = if let Some(name) = member {
                    let mut info = crate::drawable::describe(&r);
                    crate::drawable::select_member(&r, &mut info, name).ok()?;
                    info.shaders
                } else {
                    crate::drawable::find_shaders(&r)
                };
                let mats: Vec<Material> = shaders
                    .iter()
                    .filter(|s| !s.textures.is_empty())
                    .map(|s| Material {
                        shader: crate::names::shader(&s.name_hash).unwrap_or_else(|| s.name_hash.clone()),
                        slots: s.textures.iter().map(|t| (crate::names::param(&t.param_hash).unwrap_or_else(|| t.param_hash.clone()), u64::from_str_radix(&t.hash64, 16).unwrap_or(0))).collect(),
                    })
                    .collect();
                (!mats.is_empty()).then(|| (e.id.clone(), e.name.clone().unwrap_or_default(), mats))
            })
            .collect();
        models.extend(m);
    }
    progress("writing", total, total);
    let mut ids: HashMap<Material, u32> = HashMap::new();
    let mut mats: Vec<&Material> = Vec::new();
    let mut rows: Vec<Vec<u32>> = Vec::with_capacity(models.len());
    for (_, _, ms) in &models {
        rows.push(
            ms.iter()
                .map(|m| {
                    *ids.entry(m.clone()).or_insert_with(|| {
                        mats.push(m);
                        (mats.len() - 1) as u32
                    })
                })
                .collect(),
        );
    }
    let p = path(ws);
    std::fs::create_dir_all(p.parent().unwrap())?;
    {
        let f = crate::atomic::AtomicFile::create(&p)?;
        let mut w = std::io::BufWriter::new(flate2::write::GzEncoder::new(f, flate2::Compression::default()));
        writeln!(w, "{}", json!({"texture_users": 1, "materials": mats.len(), "models": models.len()}))?;
        for m in &mats {
            let slots: Vec<Value> = m.slots.iter().map(|(n, h)| json!([n, format!("{h:016x}")])).collect();
            writeln!(w, "{}", json!({"m": [m.shader, slots]}))?;
        }
        for ((id, name, _), r) in models.iter().zip(&rows) {
            writeln!(w, "{}", json!({"id": id, "name": name, "mats": r}))?;
        }
        w.into_inner().map_err(|e| e.into_error())?.finish()?.commit()?;
    }
    let textures: std::collections::HashSet<u64> = mats.iter().flat_map(|m| m.slots.iter().map(|s| s.1)).collect();
    Ok(json!({"file": p, "models": models.len(), "materials": mats.len(), "textures": textures.len()}))
}

/// The index, loaded: per texture the materials that use it, per material
/// the models that have it.
pub struct TexUsers {
    pub materials: Vec<Material>,
    /// (entry id, name)
    pub models: Vec<(String, String)>,
    pub by_texture: HashMap<u64, Vec<u32>>,
    pub material_models: Vec<Vec<u32>>,
}

impl TexUsers {
    pub fn load(p: &Path) -> Result<TexUsers> {
        let f = std::fs::File::open(p).with_context(|| format!("{} not found (run `sixng texture-users`)", p.display()))?;
        let r = std::io::BufReader::new(flate2::read::GzDecoder::new(f));
        let mut t = TexUsers { materials: Vec::new(), models: Vec::new(), by_texture: HashMap::new(), material_models: Vec::new() };
        for line in r.lines() {
            let line = line?;
            let v: Value = serde_json::from_str(&line)?;
            if let Some(m) = v.get("m") {
                let slots = m[1].as_array().map(|a| a.iter().map(|s| (s[0].as_str().unwrap_or("").to_string(), u64::from_str_radix(s[1].as_str().unwrap_or("0"), 16).unwrap_or(0))).collect()).unwrap_or_default();
                t.materials.push(Material { shader: m[0].as_str().unwrap_or("").to_string(), slots });
            } else if let Some(ms) = v.get("mats").and_then(|x| x.as_array()) {
                let mi = t.models.len() as u32;
                t.models.push((v["id"].as_str().unwrap_or("").to_string(), v["name"].as_str().unwrap_or("").to_string()));
                for k in ms.iter().filter_map(|x| x.as_u64()) {
                    if let Some(l) = t.material_models.get_mut(k as usize) {
                        l.push(mi);
                    }
                }
            }
            if t.material_models.len() < t.materials.len() {
                t.material_models.resize(t.materials.len(), Vec::new());
            }
        }
        for (i, m) in t.materials.iter().enumerate() {
            let mut seen: Vec<u64> = m.slots.iter().map(|s| s.1).collect();
            seen.sort();
            seen.dedup();
            for h in seen {
                t.by_texture.entry(h).or_default().push(i as u32);
            }
        }
        Ok(t)
    }

    /// Materials that use `hash`, most-used first.
    pub fn users(&self, hash: u64) -> Vec<u32> {
        let mut v = self.by_texture.get(&hash).cloned().unwrap_or_default();
        v.sort_by_key(|&m| std::cmp::Reverse(self.material_models[m as usize].len()));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn load_round_trip() {
        let dir = std::env::temp_dir().join(format!("sixng_tu_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.jsonl.gz");
        {
            let mut w = flate2::write::GzEncoder::new(std::fs::File::create(&p).unwrap(), flate2::Compression::fast());
            writeln!(w, r#"{{"texture_users":1}}"#).unwrap();
            writeln!(w, r#"{{"m":["standard",[["diffusetex","00000000000000aa"],["bumptex","00000000000000bb"]]]}}"#).unwrap();
            writeln!(w, r#"{{"m":["decal",[["diffusetex","00000000000000aa"]]]}}"#).unwrap();
            writeln!(w, r#"{{"id":"x.rpf@1","name":"crate","mats":[0,1]}}"#).unwrap();
            writeln!(w, r#"{{"id":"x.rpf@2","name":"crate_lod","mats":[0]}}"#).unwrap();
            w.finish().unwrap();
        }
        let t = TexUsers::load(&p).unwrap();
        assert_eq!(t.users(0xaa), vec![0, 1]);
        assert_eq!(t.users(0xbb), vec![0]);
        assert_eq!(t.material_models[0], vec![0, 1]);
        assert_eq!(t.materials[0].slots[1], ("bumptex".to_string(), 0xbb));
        std::fs::remove_dir_all(&dir).ok();
    }
}
