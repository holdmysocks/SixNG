//! Script data documents (`script.rpf`): 1,012 PSO files that each hold one
//! XML-like document in binary form (struct `20151ceade396fe2`). They are
//! the data side of the scripts (wanted-level and law tuning, positions and
//! headings, persistent characters, item sets …); the compiled script code
//! itself is not in readable form in the install.
//!
//! Layout (field names are hashes; the ones here were named by their use):
//!
//! ```text
//! document: elements (2260b8be: array of 01393259), Attributes (85b4c631),
//!           strings (4880f687: one entry per string attribute, type 0x16;
//!           the string text is not readable), vectors (d54b8545: aeb623d8
//!           {80950108 x, 9303a5e5 y, b6606c9e z}, each f32 bits)
//! element:  name (joaat of the tag), 01296aed first child, 881bf649 next
//!           sibling, f33047d9 previous sibling, d89231bd parent (0xffff =
//!           none), 03341d59 first attribute, 79661755 attribute count
//! attribute: name, 2a8e416f type, 93bab68e value, 817ed7a4 joaat of the
//!           value as text (so string values resolve by name lookup)
//!   types: 1 vector (value = vector index), 2 float (f32 bits),
//!          3 integer (i32), 4 bool, 5 string (value = string index)
//! ```
//!
//! Converted to XML text. Tag, attribute and string values are named from
//! the name dictionary; unnamed ones are written as `_<hash>` (tags and
//! attributes) and `#<hash>` (string values).
use serde_json::Value;

pub const DOC_STRUCT: &str = "20151ceade396fe2";

/// Per-document counts.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Stats {
    pub root: String,
    pub elements: usize,
    pub attributes: usize,
    /// Tags and attribute names that resolved.
    pub names_resolved: usize,
    pub names_total: usize,
    /// String values that resolved.
    pub strings_resolved: usize,
    pub strings_total: usize,
}

fn field<'a>(v: &'a Value, h: u32) -> &'a Value {
    &v[crate::meta7a::field_name(h)]
}

fn num(v: &Value, h: u32) -> Option<u64> {
    field(v, h).as_u64()
}

/// Names these documents use that the dictionary lacks (joaat checked).
const KNOWN: &[&str] = &["id"];

fn name_of(h: u32) -> Option<String> {
    KNOWN.iter().find(|n| crate::names::joaat_exact(n) == h).map(|n| n.to_string()).or_else(|| crate::names::lookup(h, crate::names::Ctx::Field).filter(|n| !n.is_empty()))
}

fn xml_name(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || "_-.".contains(x))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn f32s(bits: u64) -> String {
    let f = f32::from_bits(bits as u32);
    if f.fract() == 0.0 && f.abs() < 1e7 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// The document object of a decoded PSO (`pso::Pso::to_json`), if it is one.
pub fn document(pso_json: &Value) -> Option<&Value> {
    pso_json["objects"][DOC_STRUCT].as_array()?.first()
}

/// Convert a document to XML text.
pub fn to_xml(doc: &Value) -> Option<(String, Stats)> {
    let els = field(doc, 0x2260_b8be).as_array()?;
    let atts = doc["Attributes"].as_array().cloned().unwrap_or_default();
    let vecs = field(doc, 0xd54b_8545).as_array().cloned().unwrap_or_default();
    let mut st = Stats { elements: els.len(), attributes: atts.len(), ..Default::default() };
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let tag = |h: u32, st: &mut Stats| -> String {
        st.names_total += 1;
        match name_of(h).filter(|n| xml_name(n)) {
            Some(n) => {
                st.names_resolved += 1;
                n
            }
            None => format!("_{h:08x}"),
        }
    };
    let value = |a: &Value, st: &mut Stats| -> String {
        let v = num(a, 0x93ba_b68e).unwrap_or(0);
        let vh = num(a, 0x817e_d7a4).unwrap_or(0) as u32;
        match num(a, 0x2a8e_416f).unwrap_or(0) {
            1 => vecs
                .get(v as usize)
                .map(|x| [0x8095_0108u32, 0x9303_a5e5, 0xb660_6c9e].iter().map(|&k| f32s(field(x, k).as_i64().unwrap_or(0) as u32 as u64)).collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| format!("#{vh:08x}")),
            2 => f32s(v),
            3 => (v as u32 as i32).to_string(),
            4 => (if v != 0 { "true" } else { "false" }).to_string(),
            5 => {
                st.strings_total += 1;
                if vh == 0 {
                    String::new()
                } else {
                    match name_of(vh) {
                        Some(n) => {
                            st.strings_resolved += 1;
                            n
                        }
                        None => format!("#{vh:08x}"),
                    }
                }
            }
            t => format!("#{vh:08x} (type {t})"),
        }
    };
    // Depth-first from the first element without a parent.
    let none = 0xffff;
    let start = els.iter().position(|e| num(e, 0xd892_31bd) == Some(none)).unwrap_or(0);
    let mut stack: Vec<(usize, usize, Option<String>)> = vec![(start, 0, None)];
    let mut guard = 0;
    while let Some((i, depth, closing)) = stack.pop() {
        guard += 1;
        if guard > 4 * els.len() + 8 {
            break;
        }
        let pad = "  ".repeat(depth);
        if let Some(name) = closing {
            out += &format!("{pad}</{name}>
");
            continue;
        }
        let e = els.get(i)?;
        let name = tag(e["name"].as_u64().unwrap_or(0) as u32, &mut st);
        if depth == 0 {
            st.root = name.clone();
        }
        let mut line = format!("{pad}<{name}");
        let (fa, na) = (num(e, 0x0334_1d59).unwrap_or(none), num(e, 0x7966_1755).unwrap_or(0));
        if fa != none {
            for k in 0..na {
                if let Some(a) = atts.get((fa + k) as usize) {
                    let an = tag(a["name"].as_u64().unwrap_or(0) as u32, &mut st);
                    line += &format!(" {an}=\"{}\"", esc(&value(a, &mut st)));
                }
            }
        }
        let child = num(e, 0x0129_6aed).unwrap_or(none);
        let next = num(e, 0x881b_f649).unwrap_or(none);
        if depth > 0 && next != none {
            stack.push((next as usize, depth, None));
        }
        if child == none {
            out += &format!("{line}/>\n");
        } else {
            out += &format!("{line}>\n");
            stack.push((i, depth, Some(name)));
            stack.push((child as usize, depth + 1, None));
        }
    }
    Some((out, st))
}

/// Convert every script data document of an archive to
/// `<out>/script_docs/<archive>/<offset>.xml` plus `index.jsonl`.
/// Returns (converted, not a document or failed).
pub fn export(a: &crate::game::ArchiveInfo, entries: &[&crate::scan::Entry], out: &std::path::Path) -> anyhow::Result<(usize, usize)> {
    let dir = out.join("script_docs").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let (mut ok, mut bad) = (0, 0);
    let mut lines = String::new();
    for e in entries {
        let doc = crate::scan::read_entry(&a.path, e).ok().and_then(|b| crate::pso::parse(&b)).map(|p| p.to_json());
        let Some((xml, st)) = doc.as_ref().and_then(document).and_then(to_xml) else {
            bad += 1;
            continue;
        };
        ok += 1;
        let file = format!("{:x}.xml", e.off);
        crate::atomic::write(dir.join(&file), xml)?;
        lines += &serde_json::json!({"id": e.id, "file": file, "stats": st}).to_string();
        lines.push('\n');
    }
    crate::atomic::write(dir.join("index.jsonl"), lines)?;
    Ok((ok, bad))
}

#[cfg(test)]
mod tests {
    use crate::meta7a::field_name as f;
    use serde_json::json;

    #[test]
    fn document_to_xml() {
        let none = 0xffff;
        let el = |name: u32, child: u32, next: u32, parent: u32, fa: u32, na: u32| {
            let mut m = serde_json::Map::new();
            m.insert("name".into(), json!(name));
            for (h, v) in [(0x0129_6aedu32, child), (0x881b_f649, next), (0xd892_31bd, parent), (0xf330_47d9, none), (0x0334_1d59, fa), (0x7966_1755, na)] {
                m.insert(f(h), json!(v));
            }
            serde_json::Value::Object(m)
        };
        let at = |name: u32, ty: u32, v: u32, vh: u32| {
            let mut m = serde_json::Map::new();
            m.insert("name".into(), json!(name));
            m.insert(f(0x2a8e_416f), json!(ty));
            m.insert(f(0x93ba_b68e), json!(v));
            m.insert(f(0x817e_d7a4), json!(vh));
            serde_json::Value::Object(m)
        };
        let id = crate::names::joaat_exact("id");
        let mut doc = serde_json::Map::new();
        doc.insert(f(0x2260_b8be), json!([el(0x1234, 1, none, none, none, 0), el(0x5678, none, 2, 0, 0, 2), el(0x5678, none, none, 0, 2, 1)]));
        doc.insert("Attributes".into(), json!([at(id, 3, (-5i32) as u32, 0), at(0x9, 2, 1.5f32.to_bits(), 0), at(id, 4, 1, 0)]));
        let (xml, st) = super::to_xml(&serde_json::Value::Object(doc)).unwrap();
        assert!(xml.contains("<_00001234>
  <_00005678 id=\"-5\" _00000009=\"1.5\"/>
  <_00005678 id=\"true\"/>
</_00001234>"), "{xml}");
        assert_eq!((st.elements, st.attributes), (3, 3));
    }
}
