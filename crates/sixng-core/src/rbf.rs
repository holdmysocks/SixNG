//! RBF binary XML (`RBF0` files: UI layouts, subtitles, GPS and gamepad
//! configs, tuning files). An RBF is a flat token stream that writes an XML
//! document depth first. Tag and attribute names are stored as text the
//! first time they are used, so every name is known; there are no hashes.
//!
//! Layout (all little-endian):
//!
//! ```text
//! "RBF0"
//! [FFFB u16 version]                 optional; version 1 = 8-byte numbers,
//!                                    no version token = 4-byte numbers
//! token := FFFF                      close the open element
//!        | FFFD u32 len bytes[len]   text content (len includes a final NUL)
//!        | u16 id_type               element; id = low 12 bits, type = top 4
//! id:      index into the name table. An id equal to the table size
//!          defines the next name: u16 len, bytes[len] (no NUL).
//! type 0:  element with content: u32 (always 0), u16 attribute count,
//!          attributes; stays open until FFFF
//! type 1:  integer value element (i64, or i32 without version)
//! type 2:  bool false, type 3: bool true (no payload)
//! type 4:  float value element (f64, or f32 without version)
//! type 5:  vector value element (3 x f32 in both versions)
//! type 6:  string value element (u16 len, bytes)
//! attribute := u16 id_type, [new name], value with the same type codes
//! ```
//!
//! Value elements (types 1-6) close themselves; in XML they are written the
//! usual way, `<name value="..."/>` (vectors as `<name x=".." y=".." z=".."/>`;
//! a vector attribute is written as `"x, y, z"`).
//!
//! An element with `content="vec2v_array"` holds binary text: 16 bytes per
//! point (f32 x, f32 y, 8 zero bytes), with no final NUL. It is written as
//! one `x, y` line per point. Other text with control characters is
//! written as hex with `encoding="hex"` added to the element.

/// Per-document counts.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Stats {
    pub version: u16,
    pub root: String,
    pub names: usize,
    pub elements: usize,
    pub value_elements: usize,
    pub attributes: usize,
    pub texts: usize,
    /// Texts written as hex (binary, not a known array layout).
    pub hex_texts: usize,
    pub max_depth: usize,
}

/// Why a file did not decode.
#[derive(Debug, Clone)]
pub struct Error {
    /// Short reason, used to count failures by kind.
    pub reason: &'static str,
    pub offset: usize,
    pub detail: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at 0x{:x}{}{}", self.reason, self.offset, if self.detail.is_empty() { "" } else { ": " }, self.detail)
    }
}

pub fn is_rbf(d: &[u8]) -> bool {
    d.starts_with(b"RBF0")
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && !"\t\n\r".contains(c) => o.push_str(&format!("&#x{:x};", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// A name usable as an XML tag or attribute: invalid characters become `_`.
fn xml_name(n: &str) -> String {
    let mut o: String = n.chars().map(|c| if c.is_ascii_alphanumeric() || "_-.:".contains(c) { c } else { '_' }).collect();
    if !o.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        o.insert(0, '_');
    }
    o
}

fn fmt_f64(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    wide: bool,
    names: Vec<String>,
}

impl<'a> Reader<'a> {
    fn err(&self, reason: &'static str, detail: String) -> Error {
        Error { reason, offset: self.p, detail }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.p + n > self.d.len() {
            return Err(self.err("truncated", format!("need {n} bytes")));
        }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn str16(&mut self) -> Result<String, Error> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn name(&mut self, id: u16) -> Result<String, Error> {
        let i = id as usize;
        if i == self.names.len() {
            let n = self.str16()?;
            self.names.push(n);
        } else if i > self.names.len() {
            self.p -= 2;
            return Err(self.err("name id out of order", format!("id {i}, {} names defined", self.names.len())));
        }
        Ok(self.names[i].clone())
    }
    /// Three f32 (type 5, the same size in both versions).
    fn vec3(&mut self) -> Result<[String; 3], Error> {
        let mut v: [String; 3] = Default::default();
        for x in &mut v {
            *x = fmt_f64(f32::from_bits(self.u32()?) as f64);
        }
        Ok(v)
    }
    /// The value of a type code 1-6 as text.
    fn value(&mut self, ty: u8) -> Result<String, Error> {
        Ok(match ty {
            1 => {
                if self.wide {
                    i64::from_le_bytes(self.take(8)?.try_into().unwrap()).to_string()
                } else {
                    (self.u32()? as i32).to_string()
                }
            }
            2 => "false".into(),
            3 => "true".into(),
            4 => {
                if self.wide {
                    fmt_f64(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
                } else {
                    fmt_f64(f32::from_bits(self.u32()?) as f64)
                }
            }
            5 => self.vec3()?.join(", "),
            6 => self.str16()?,
            t => {
                self.p -= 2;
                return Err(self.err("unknown value type", format!("type {t}")));
            }
        })
    }
}

/// Element state while writing: has it written its `>` yet, and how.
#[derive(PartialEq)]
enum Open {
    Empty,
    Text,
    Children,
}

/// Decode an RBF file to XML text.
pub fn to_xml(d: &[u8]) -> Result<(String, Stats), Error> {
    if !is_rbf(d) {
        return Err(Error { reason: "not RBF0", offset: 0, detail: String::new() });
    }
    let mut r = Reader { d, p: 4, wide: false, names: Vec::new() };
    let mut st = Stats::default();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut stack: Vec<(String, Open, Option<String>)> = Vec::new();
    let mut roots = 0;
    // Close the start tag of the innermost element before content.
    fn open_content(out: &mut String, stack: &mut [(String, Open, Option<String>)], text: bool) {
        if let Some(top) = stack.last_mut() {
            if top.1 == Open::Empty {
                out.push('>');
                top.1 = if text { Open::Text } else { Open::Children };
            } else if !text && top.1 == Open::Text {
                top.1 = Open::Children;
            }
        }
        if !text {
            out.push('\n');
            out.push_str(&"  ".repeat(stack.len()));
        }
    }
    while r.p < d.len() {
        let t = r.u16()?;
        match t {
            0xffff => {
                let Some((name, state, _)) = stack.pop() else {
                    r.p -= 2;
                    return Err(r.err("close without open element", String::new()));
                };
                match state {
                    Open::Empty => out.push_str("/>"),
                    Open::Text => out.push_str(&format!("</{name}>")),
                    Open::Children => {
                        out.push('\n');
                        out.push_str(&"  ".repeat(stack.len()));
                        out.push_str(&format!("</{name}>"));
                    }
                }
            }
            0xfffd => {
                let n = r.u32()? as usize;
                let b = r.take(n)?;
                let Some(top) = stack.last() else {
                    r.p -= n + 6;
                    return Err(r.err("text outside the root", String::new()));
                };
                st.texts += 1;
                let depth = stack.len();
                if top.2.as_deref() == Some("vec2v_array") && n % 16 == 0 && b.chunks(16).all(|c| c[8..].iter().all(|&x| x == 0)) {
                    if let Some(top) = stack.last_mut() {
                        if top.1 == Open::Empty {
                            out.push('>');
                        }
                        top.1 = Open::Children;
                    }
                    for c in b.chunks(16) {
                        let f = |i: usize| fmt_f64(f32::from_le_bytes(c[i..i + 4].try_into().unwrap()) as f64);
                        out.push('\n');
                        out.push_str(&"  ".repeat(depth));
                        out.push_str(&format!("{}, {}", f(0), f(4)));
                    }
                    continue;
                }
                let b = b.strip_suffix(&[0]).unwrap_or(b);
                let t = String::from_utf8_lossy(b);
                if t.chars().any(|c| (c as u32) < 0x20 && !"\t\n\r".contains(c)) {
                    st.hex_texts += 1;
                    if stack.last().is_some_and(|t| t.1 == Open::Empty) {
                        out.push_str(" encoding=\"hex\"");
                    }
                    open_content(&mut out, &mut stack, true);
                    out.push_str(&b.iter().map(|x| format!("{x:02x}")).collect::<String>());
                } else {
                    open_content(&mut out, &mut stack, true);
                    out.push_str(&esc(&t));
                }
            }
            0xfffb => {
                if r.p != 6 {
                    r.p -= 2;
                    return Err(r.err("version token not at the start", String::new()));
                }
                st.version = r.u16()?;
                r.wide = st.version >= 1;
            }
            t if t >= 0xfff0 => {
                r.p -= 2;
                return Err(r.err("unknown token", format!("{t:04x}")));
            }
            t => {
                let (ty, id) = ((t >> 12) as u8, t & 0xfff);
                let name = xml_name(&r.name(id)?);
                if stack.is_empty() {
                    roots += 1;
                    if roots > 1 {
                        r.p -= 2;
                        return Err(r.err("second root element", String::new()));
                    }
                    st.root = name.clone();
                } else {
                    open_content(&mut out, &mut stack, false);
                }
                if ty == 0 {
                    st.elements += 1;
                    let _reserved = r.u32()?;
                    let na = r.u16()?;
                    out.push_str(&format!("<{name}"));
                    let mut content = None;
                    for _ in 0..na {
                        let a = r.u16()?;
                        let (aty, aid) = ((a >> 12) as u8, a & 0xfff);
                        let an = xml_name(&r.name(aid)?);
                        let v = r.value(aty)?;
                        st.attributes += 1;
                        if an == "content" {
                            content = Some(v.clone());
                        }
                        out.push_str(&format!(" {an}=\"{}\"", esc(&v)));
                    }
                    stack.push((name, Open::Empty, content));
                    st.max_depth = st.max_depth.max(stack.len());
                } else {
                    st.value_elements += 1;
                    if ty == 5 {
                        let [x, y, z] = r.vec3()?;
                        out.push_str(&format!("<{name} x=\"{x}\" y=\"{y}\" z=\"{z}\"/>"));
                    } else {
                        let v = r.value(ty)?;
                        out.push_str(&format!("<{name} value=\"{}\"/>", esc(&v)));
                    }
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err(r.err("unclosed elements at end of file", format!("{} open", stack.len())));
    }
    if roots == 0 {
        return Err(r.err("no root element", String::new()));
    }
    out.push('\n');
    st.names = r.names.len();
    Ok((out, st))
}

/// Summary of an export.
#[derive(Debug, Default, serde::Serialize)]
pub struct Summary {
    pub decoded: usize,
    pub failed: usize,
    /// Failure reason -> count.
    pub errors: std::collections::BTreeMap<String, usize>,
}

/// Convert the RBF entries of an archive to `<out>/rbf/<archive>/<offset>.xml`
/// plus `index.jsonl` (one line per entry, with stats or the error).
pub fn export(a: &crate::game::ArchiveInfo, entries: &[&crate::scan::Entry], out: &std::path::Path, progress: crate::export::Progress) -> anyhow::Result<Summary> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    let dir = out.join("rbf").join(a.id.trim_end_matches(".rpf"));
    std::fs::create_dir_all(&dir)?;
    let total = entries.len() as u64;
    let done = AtomicU64::new(0);
    let rows: Vec<(serde_json::Value, Result<(), String>)> = entries
        .par_iter()
        .map(|e| {
            progress(&e.id, done.fetch_add(1, Ordering::Relaxed) + 1, total);
            let r = crate::scan::read_entry(&a.path, e).map_err(|x| ("read failed".to_string(), x.to_string())).and_then(|b| to_xml(&b).map_err(|x| (x.reason.to_string(), x.to_string())));
            match r {
                Ok((xml, st)) => {
                    let file = format!("{:x}.xml", e.off);
                    match crate::atomic::write(dir.join(&file), xml) {
                        Ok(()) => (serde_json::json!({"id": e.id, "name": e.name, "file": file, "stats": st}), Ok(())),
                        Err(x) => (serde_json::json!({"id": e.id, "error": x.to_string()}), Err("write failed".into())),
                    }
                }
                Err((reason, detail)) => (serde_json::json!({"id": e.id, "name": e.name, "error": detail}), Err(reason)),
            }
        })
        .collect();
    let mut s = Summary::default();
    let mut lines = String::new();
    for (row, r) in rows {
        match r {
            Ok(()) => s.decoded += 1,
            Err(reason) => {
                s.failed += 1;
                *s.errors.entry(reason).or_default() += 1;
            }
        }
        lines += &row.to_string();
        lines.push('\n');
    }
    crate::atomic::write(dir.join("index.jsonl"), lines)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(b: &mut Vec<u8>, id_type: u16, n: &str) {
        b.extend(id_type.to_le_bytes());
        b.extend((n.len() as u16).to_le_bytes());
        b.extend(n.as_bytes());
    }

    #[test]
    fn decodes_versioned_document() {
        let mut b = b"RBF0".to_vec();
        b.extend([0xfb, 0xff, 1, 0]);
        name(&mut b, 0x0000, "Root"); // element, name 0
        b.extend([0, 0, 0, 0, 1, 0]); // reserved, 1 attribute
        name(&mut b, 0x6001, "type"); // string attribute, name 1
        b.extend(3u16.to_le_bytes());
        b.extend(b"A&B");
        name(&mut b, 0x4002, "scale"); // float value element
        b.extend(1.5f64.to_le_bytes());
        name(&mut b, 0x1003, "count");
        b.extend((-2i64).to_le_bytes());
        name(&mut b, 0x3004, "on");
        name(&mut b, 0x0005, "Label");
        b.extend([0, 0, 0, 0, 0, 0]);
        b.extend([0xfd, 0xff, 4, 0, 0, 0]);
        b.extend(b"hi<\0");
        b.extend([0xff, 0xff, 0xff, 0xff]);
        let (xml, st) = to_xml(&b).unwrap();
        assert_eq!(
            xml,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Root type=\"A&amp;B\">\n  <scale value=\"1.5\"/>\n  <count value=\"-2\"/>\n  <on value=\"true\"/>\n  <Label>hi&lt;</Label>\n</Root>\n"
        );
        assert_eq!((st.version, st.elements, st.value_elements, st.attributes, st.texts, st.names), (1, 2, 3, 1, 1, 6));
    }

    #[test]
    fn narrow_numbers_without_version() {
        let mut b = b"RBF0".to_vec();
        name(&mut b, 0x0000, "T");
        b.extend([0, 0, 0, 0, 0, 0]);
        name(&mut b, 0x4001, "f");
        b.extend(1.0f32.to_le_bytes());
        b.extend([0xff, 0xff]);
        let (xml, _) = to_xml(&b).unwrap();
        assert!(xml.contains("<T>\n  <f value=\"1.0\"/>\n</T>"), "{xml}");
    }

    #[test]
    fn vec2_array_text() {
        let mut b = b"RBF0".to_vec();
        b.extend([0xfb, 0xff, 1, 0]);
        name(&mut b, 0x0000, "Points");
        b.extend([0, 0, 0, 0, 1, 0]);
        name(&mut b, 0x6001, "content");
        b.extend(11u16.to_le_bytes());
        b.extend(b"vec2v_array");
        b.extend([0xfd, 0xff, 32, 0, 0, 0]);
        for (x, y) in [(10.0f32, 0.0f32), (0.5, 72.0)] {
            b.extend(x.to_le_bytes());
            b.extend(y.to_le_bytes());
            b.extend([0; 8]);
        }
        b.extend([0xff, 0xff]);
        let (xml, _) = to_xml(&b).unwrap();
        assert!(xml.contains("<Points content=\"vec2v_array\">
  10.0, 0.0
  0.5, 72.0
</Points>"), "{xml}");
    }

    #[test]
    fn vector_values() {
        let mut b = b"RBF0".to_vec();
        b.extend([0xfb, 0xff, 1, 0]);
        name(&mut b, 0x0000, "E");
        b.extend([0, 0, 0, 0, 1, 0]);
        name(&mut b, 0x5001, "off");
        for f in [1.0f32, 2.5, -3.0] {
            b.extend(f.to_le_bytes());
        }
        name(&mut b, 0x5002, "Position");
        for f in [0.0f32, 1.0, 2.0] {
            b.extend(f.to_le_bytes());
        }
        b.extend([0xff, 0xff]);
        let (xml, _) = to_xml(&b).unwrap();
        assert!(xml.contains("<E off=\"1.0, 2.5, -3.0\">\n  <Position x=\"0.0\" y=\"1.0\" z=\"2.0\"/>\n</E>"), "{xml}");
    }

    #[test]
    fn reports_errors() {
        let mut b = b"RBF0".to_vec();
        name(&mut b, 0x0000, "T");
        b.extend([0, 0, 0, 0, 0, 0]);
        assert_eq!(to_xml(&b).unwrap_err().reason, "unclosed elements at end of file");
        let mut c = b"RBF0".to_vec();
        c.extend([5, 0]);
        assert_eq!(to_xml(&c).unwrap_err().reason, "name id out of order");
    }
}
