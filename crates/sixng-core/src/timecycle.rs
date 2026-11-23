//! Timecycle, weather and cloud data: the plain XML files of `common.rpf`
//! (`data/timecycle/*`, the weather list and the custom cloud groups).
//!
//! The archive stores no file names, so each document is recognised by its
//! root element:
//! - `timecycle_modifier_data`: modifiers. Each `<modifier>` has the
//!   attributes `name`, `description`, `groupApplyTag`, `localApplyIndex`,
//!   `userFlags` (plus `numAxes` and `numKeyframes` when animated), then
//!   an optional `<submodifiers>` list of other modifier names, then one
//!   element per variable holding two numbers (`<postfx_noise>1.0 0.0</…>`).
//!   The first is the value the modifier sets; the meaning of the second is
//!   not known (it is 0 or 1 in most records). Animated modifiers add
//!   `<axis controlValue=… blendType=…>` blocks whose variables hold one
//!   number per keyframe, with the keyframe times in `keyframe_t_value`.
//! - `timecycle_keyframe_data`: one weather cycle (`<cycle name=…>`), a
//!   `High_Level_Vars` block, then per region (`GLOBAL`, `URBAN`) one
//!   element per variable with one number per time sample.
//! - `time_data`: the time samples (hour, interpolation, underwater
//!   modifier) that the keyframe columns stand for.
//! - `CContentsOfWeatherXmlFile`: weather types and their cycle files.
//! - `CustomClouds`: placed cloud groups (one per scripted scene).
//! - `CDataFileMgr__ContentChangeSetDef` with `TIMECYCLEMOD_FILE` items:
//!   the list of modifier file names, used to name the unnamed modifier
//!   files (see [`name_files`]).
use serde_json::{json, Map, Value};

/// A parsed XML element.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    pub fn child(&self, k: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == k)
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Minimal XML reader for the game's data files: elements, attributes,
/// text, comments, declarations and CDATA. Returns the first root element.
pub fn parse_xml(src: &str) -> Option<Node> {
    parse_xml_all(src)?.into_iter().next()
}

/// Every root element of `src`. Some archive entries hold several XML
/// documents back to back (`…</timecycle_modifier_data><?xml …?>…`).
pub fn parse_xml_all(src: &str) -> Option<Vec<Node>> {
    let s = src.trim_start_matches('\u{feff}');
    let b = s.as_bytes();
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let j = s[i..].find('<').map(|x| i + x).unwrap_or(b.len());
            let t = s[i..j].trim();
            if !t.is_empty() {
                let top = stack.last_mut()?;
                if !top.text.is_empty() {
                    top.text.push(' ');
                }
                top.text.push_str(&unescape(t));
            }
            i = j;
            continue;
        }
        let rest = &s[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->")? + 3;
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let e = r.find("]]>")?;
            stack.last_mut()?.text.push_str(&r[..e]);
            i += 9 + e + 3;
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            i += rest.find('>')? + 1;
        } else if rest.starts_with("</") {
            i += rest.find('>')? + 1;
            let n = stack.pop()?;
            stack.last_mut()?.children.push(n);
        } else {
            // start tag: name, attributes, optional "/>"
            let mut j = i + 1;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let mut n = Node { name: s[i + 1..j].to_string(), ..Default::default() };
            let mut closed = false;
            loop {
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= b.len() {
                    return None;
                }
                if b[j] == b'>' {
                    j += 1;
                    break;
                }
                if b[j] == b'/' {
                    closed = true;
                    j += 1;
                    continue;
                }
                let k0 = j;
                while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                    j += 1;
                }
                let key = s[k0..j].to_string();
                while j < b.len() && (b[j].is_ascii_whitespace() || b[j] == b'=') {
                    j += 1;
                }
                let q = *b.get(j)?;
                if q == b'"' || q == b'\'' {
                    let e = s[j + 1..].find(q as char)? + j + 1;
                    // attribute-value normalisation: line breaks and tabs become spaces
                    let raw = s[j + 1..e].replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
                    n.attrs.push((key, unescape(&raw)));
                    j = e + 1;
                } else {
                    n.attrs.push((key, String::new()));
                }
            }
            i = j;
            if closed {
                stack.last_mut()?.children.push(n);
            } else {
                stack.push(n);
            }
        }
    }
    // tolerate unclosed elements at the end
    while stack.len() > 1 {
        let n = stack.pop()?;
        stack.last_mut()?.children.push(n);
    }
    Some(stack.pop()?.children)
}

/// Whitespace-separated numbers; `None` if any token is not a number.
pub fn numbers(t: &str) -> Option<Vec<f64>> {
    let v: Option<Vec<f64>> = t.split_ascii_whitespace().map(|x| x.parse::<f64>().ok()).collect();
    v.filter(|v| !v.is_empty())
}

fn num_value(x: f64) -> Value {
    // f32-precision values written with 6 decimals: keep them as written
    json!((x * 1e6).round() / 1e6)
}

fn nums_value(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|&x| num_value(x)).collect())
}

fn scalar(t: &str) -> Value {
    match t {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    match numbers(t) {
        Some(v) if v.len() == 1 => num_value(v[0]),
        Some(v) => nums_value(&v),
        None => Value::String(t.to_string()),
    }
}

/// Generic element -> JSON for the weather and cloud files: `value="…"`
/// becomes the value, `x/y/z/w` attributes an array, `Item` lists arrays,
/// repeated children arrays, other attributes `@name` keys.
pub fn to_json(n: &Node) -> Value {
    if n.children.is_empty() {
        if n.attrs.len() == 1 && n.attrs[0].0 == "value" && n.text.is_empty() {
            return scalar(&n.attrs[0].1);
        }
        let xyzw = ["x", "y", "z", "w"];
        if !n.attrs.is_empty() && n.text.is_empty() && n.attrs.iter().enumerate().all(|(i, (k, _))| xyzw.get(i) == Some(&k.as_str())) {
            let v: Option<Vec<f64>> = n.attrs.iter().map(|(_, v)| v.parse().ok()).collect();
            if let Some(v) = v {
                return nums_value(&v);
            }
        }
        if n.attrs.is_empty() {
            return if n.text.is_empty() { Value::Null } else { scalar(&n.text) };
        }
    }
    let item_list = !n.children.is_empty() && n.children.iter().all(|c| c.name.eq_ignore_ascii_case("item"));
    if item_list && n.attrs.is_empty() {
        return Value::Array(n.children.iter().map(to_json).collect());
    }
    let mut m = Map::new();
    for (k, v) in &n.attrs {
        m.insert(format!("@{k}"), scalar(v));
    }
    if !n.text.is_empty() {
        m.insert("#text".into(), scalar(&n.text));
    }
    for c in &n.children {
        let v = to_json(c);
        if n.children.iter().filter(|x| x.name == c.name).count() > 1 {
            if let Value::Array(a) = m.entry(c.name.clone()).or_insert_with(|| Value::Array(vec![])) {
                a.push(v);
            }
        } else {
            m.insert(c.name.clone(), v);
        }
    }
    Value::Object(m)
}

/// Kind of a timecycle-related document, from its root element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Modifiers,
    Cycle,
    TimeSamples,
    Weather,
    CustomClouds,
    FileList,
}

pub fn doc_kind(root: &Node) -> Option<DocKind> {
    Some(match root.name.as_str() {
        "timecycle_modifier_data" => DocKind::Modifiers,
        "timecycle_keyframe_data" => DocKind::Cycle,
        "time_data" => DocKind::TimeSamples,
        "CContentsOfWeatherXmlFile" => DocKind::Weather,
        "CustomClouds" => DocKind::CustomClouds,
        "CDataFileMgr__ContentChangeSetDef" if !modifier_file_list(root).is_empty() => DocKind::FileList,
        _ => return None,
    })
}

/// The `TIMECYCLEMOD_FILE` file names of a content change set, in order.
pub fn modifier_file_list(root: &Node) -> Vec<String> {
    let Some(fl) = root.child("fileList") else { return vec![] };
    fl.children
        .iter()
        .filter(|it| it.child("fileType").is_some_and(|t| t.text == "TIMECYCLEMOD_FILE"))
        .filter_map(|it| it.child("filename").map(|f| f.text.clone()))
        .collect()
}

/// A timecycle modifier.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub name: String,
    /// Attributes in file order (`description`, `groupApplyTag`, …).
    pub attrs: Vec<(String, String)>,
    pub submodifiers: Vec<String>,
    /// Variable -> the two numbers of the record, in file order.
    pub vars: Vec<(String, Vec<f64>)>,
    pub axes: Vec<Axis>,
    /// Structured blocks (weather modules map high-level weather variables:
    /// `<Fog><input_min>…</input_min>…</Fog>`), as generic JSON.
    pub blocks: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub attrs: Vec<(String, String)>,
    /// Variable -> one number per keyframe (`keyframe_t_value` included).
    pub vars: Vec<(String, Vec<f64>)>,
}

fn var_list(n: &Node, skip: &[&str]) -> Vec<(String, Vec<f64>)> {
    n.children.iter().filter(|c| c.children.is_empty() && !skip.contains(&c.name.as_str())).filter_map(|c| Some((c.name.clone(), numbers(&c.text)?))).collect()
}

pub fn modifiers(root: &Node) -> Vec<Modifier> {
    root.children
        .iter()
        .filter(|m| m.name == "modifier")
        .map(|m| Modifier {
            name: m.attr("name").unwrap_or_default().to_string(),
            attrs: m.attrs.iter().filter(|(k, _)| k != "name").cloned().collect(),
            submodifiers: m.child("submodifiers").map(|s| s.children.iter().map(|c| c.text.clone()).filter(|t| !t.is_empty()).collect()).unwrap_or_default(),
            vars: var_list(m, &[]),
            axes: m.children.iter().filter(|c| c.name == "axis").map(|a| Axis { attrs: a.attrs.clone(), vars: var_list(a, &[]) }).collect(),
            blocks: m.children.iter().filter(|c| !c.children.is_empty() && c.name != "axis" && c.name != "submodifiers").map(|c| (c.name.clone(), to_json(c))).collect(),
        })
        .collect()
}

fn snake(k: &str) -> String {
    let mut o = String::new();
    for (i, c) in k.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                o.push('_');
            }
            o.push(c.to_ascii_lowercase());
        } else {
            o.push(c);
        }
    }
    o
}

fn attr_json(attrs: &[(String, String)], m: &mut Map<String, Value>) {
    for (k, v) in attrs {
        let val = match v.parse::<i64>() {
            Ok(i) => json!(i),
            Err(_) => json!(v),
        };
        m.insert(snake(k), val);
    }
}

impl Modifier {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("name".into(), json!(self.name));
        attr_json(&self.attrs, &mut m);
        if !self.submodifiers.is_empty() {
            m.insert("submodifiers".into(), json!(self.submodifiers));
        }
        let mut vars = Map::new();
        // a variable written twice with different numbers: the JSON keeps the
        // last, `repeated` lists every copy (which one the game uses is not known)
        let mut repeated = Map::new();
        for (k, v) in &self.vars {
            let copies: Vec<&Vec<f64>> = self.vars.iter().filter(|x| &x.0 == k).map(|x| &x.1).collect();
            if copies.iter().any(|c| *c != v) {
                repeated.insert(k.clone(), Value::Array(copies.iter().map(|c| nums_value(c)).collect()));
            }
            let mut o = Map::new();
            o.insert("value".into(), v.first().map(|&x| num_value(x)).unwrap_or(Value::Null));
            if v.len() > 1 {
                o.insert("b".into(), num_value(v[1]));
            }
            if v.len() > 2 {
                o.insert("extra".into(), nums_value(&v[2..]));
            }
            vars.insert(k.clone(), Value::Object(o));
        }
        m.insert("vars".into(), Value::Object(vars));
        if !repeated.is_empty() {
            m.insert("repeated".into(), Value::Object(repeated));
        }
        if !self.blocks.is_empty() {
            m.insert("high_level_vars".into(), Value::Object(self.blocks.iter().cloned().collect()));
        }
        if !self.axes.is_empty() {
            let axes: Vec<Value> = self
                .axes
                .iter()
                .map(|a| {
                    let mut o = Map::new();
                    attr_json(&a.attrs, &mut o);
                    let mut vars = Map::new();
                    for (k, v) in &a.vars {
                        vars.insert(k.clone(), nums_value(v));
                    }
                    o.insert("vars".into(), Value::Object(vars));
                    Value::Object(o)
                })
                .collect();
            m.insert("axes".into(), json!(axes));
        }
        Value::Object(m)
    }
}

/// A weather cycle (one keyframe file).
#[derive(Debug, Clone, PartialEq)]
pub struct Cycle {
    pub name: String,
    pub high_level: Vec<(String, Vec<f64>)>,
    /// Region name -> variable -> one number per time sample.
    pub regions: Vec<(String, Vec<(String, Vec<f64>)>)>,
}

pub fn cycle(root: &Node) -> Option<Cycle> {
    let c = root.child("cycle")?;
    Some(Cycle {
        name: c.attr("name").unwrap_or_default().to_string(),
        high_level: root.child("High_Level_Vars").map(|h| var_list(h, &[])).unwrap_or_default(),
        regions: c.children.iter().filter(|r| r.name == "region").map(|r| (r.attr("name").unwrap_or_default().to_string(), var_list(r, &[]))).collect(),
    })
}

impl Cycle {
    pub fn to_json(&self) -> Value {
        let mut hl = Map::new();
        for (k, v) in &self.high_level {
            hl.insert(k.clone(), if v.len() == 1 { num_value(v[0]) } else { nums_value(v) });
        }
        let mut regions = Map::new();
        for (r, vars) in &self.regions {
            let mut o = Map::new();
            for (k, v) in vars {
                o.insert(k.clone(), nums_value(v));
            }
            regions.insert(r.clone(), Value::Object(o));
        }
        json!({"name": self.name, "high_level_vars": hl, "regions": regions})
    }
}

/// The time samples: `[{name, hour, uw_tc_mod, interpolation_type}]`.
pub fn time_samples(root: &Node) -> Value {
    let samples: Vec<Value> = root
        .children
        .iter()
        .filter(|s| s.name == "sample")
        .map(|s| {
            let mut m = Map::new();
            attr_json(&s.attrs, &mut m);
            Value::Object(m)
        })
        .collect();
    let regions: Vec<&str> = root.children.iter().filter(|s| s.name == "region").filter_map(|r| r.attr("name")).collect();
    json!({"mode": root.attr("mode"), "samples": samples, "regions": regions})
}

fn key_tokens(file: &str) -> (Option<String>, Vec<String>) {
    let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = base.strip_suffix(".xml").or_else(|| base.strip_suffix(".XML")).unwrap_or(base);
    let lower = stem.to_ascii_lowercase();
    if let Some(code) = lower.strip_prefix("tc_mods_area_") {
        return (Some(code.to_string()), vec![]);
    }
    let rest = lower.strip_prefix("tc_mods_").unwrap_or(if lower.starts_with("timecycle_mods") { "" } else { &lower });
    (None, rest.split('_').filter(|t| t.len() >= 4).map(str::to_string).collect())
}

/// How well a modifier file's names fit a listed file name (-0.5..1): the
/// share of modifiers whose name starts with the area code
/// (`TC_mods_area_DN.xml` ~ `dn28_off_1_f4a`) or contains a word of it
/// (`TC_mods_Arcade_Games.xml` ~ `ARCADE_A_1`); an area file none of whose
/// names carry the code scores -0.5.
pub fn fit(file: &str, names: &[String]) -> f64 {
    if names.is_empty() {
        return 0.0;
    }
    let (code, words) = key_tokens(file);
    let hits = names
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            match &code {
                Some(c) => l.strip_prefix(c.as_str()).is_some_and(|r| r.starts_with(|ch: char| ch.is_ascii_digit() || ch == '_')),
                None => words.iter().any(|w| l.contains(w.as_str())),
            }
        })
        .count();
    if hits == 0 && code.is_some() {
        // area files name their modifiers after the area: no hit is evidence against
        return -0.5;
    }
    hits as f64 / names.len() as f64
}

/// Name the unnamed modifier files. The archive packs files in name order
/// (case-insensitive), so the files in offset order are a subsequence of
/// the sorted list; the alignment that keeps that order and fits the
/// modifier names best ([`fit`]) gives each file its name. Returns per file
/// `(listed name, fit)`; listed names left over are not in the archive.
pub fn name_files(listed: &[String], files: &[Vec<String>]) -> Vec<Option<(String, f64)>> {
    let mut l: Vec<&String> = listed.iter().collect();
    l.sort_by_key(|s| s.to_ascii_lowercase());
    l.dedup();
    let (n, m) = (files.len(), l.len());
    if n == 0 || n > m {
        return vec![None; n];
    }
    let score: Vec<Vec<f64>> = files.iter().map(|f| l.iter().map(|x| fit(x, f)).collect()).collect();
    // best[i][j]: best total for files[i..] using l[j..]
    let mut best = vec![vec![f64::NEG_INFINITY; m + 1]; n + 1];
    for b in best[n].iter_mut() {
        *b = 0.0;
    }
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let take = score[i][j] + best[i + 1][j + 1];
            let skip = best[i][j + 1];
            best[i][j] = take.max(skip);
        }
    }
    let mut out = Vec::with_capacity(n);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let take = score[i][j] + best[i + 1][j + 1];
        if take >= best[i][j + 1] && best[i + 1][j + 1].is_finite() {
            out.push(Some((l[j].clone(), score[i][j])));
            i += 1;
        }
        j += 1;
    }
    out.resize(n, None);
    out
}

/// Everything decoded from one archive.
#[derive(Debug, Default)]
pub struct Decoded {
    /// Per document: source id, kind, assigned file name, fit, record count.
    pub files: Vec<Value>,
    /// Listed modifier files not found in the archive.
    pub missing: Vec<String>,
    pub modifiers: Vec<Value>,
    pub cycles: Vec<Value>,
    pub time: Value,
    pub weather: Value,
    pub clouds: Value,
}

fn base_name(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

/// Read every XML entry of an archive and decode the timecycle documents.
pub fn decode(archive: &std::path::Path, entries: &[&crate::scan::Entry]) -> anyhow::Result<Decoded> {
    use rayon::prelude::*;
    // (source, root, kind); the source is "<entry id>" or "<entry id>#<n>"
    // for the n-th (0-based) further document of an entry
    let mut docs: Vec<(String, Node, DocKind)> = entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::Xml)
        .flat_map_iter(|e| {
            let b = crate::scan::read_entry(archive, e).ok();
            let roots = b.and_then(|b| parse_xml_all(&String::from_utf8_lossy(&b))).unwrap_or_default();
            roots.into_iter().enumerate().filter_map(|(n, r)| {
                let k = doc_kind(&r)?;
                Some((if n == 0 { e.id.clone() } else { format!("{}#{n}", e.id) }, r, k))
            }).collect::<Vec<_>>()
        })
        .collect();
    let off = |id: &str| {
        let (a, n) = id.split_once('#').unwrap_or((id, "0"));
        (u64::from_str_radix(a.rsplit('@').next().unwrap_or("0"), 16).unwrap_or(0), n.parse::<u32>().unwrap_or(0))
    };
    docs.sort_by_key(|d| off(&d.0));
    let mut d = Decoded::default();
    // the list also names keyframe files (UNDERWATER_DEEP.xml): drop the cycles
    let cycle_names: std::collections::BTreeSet<String> = docs.iter().filter(|x| x.2 == DocKind::Cycle).filter_map(|x| x.1.child("cycle")?.attr("name").map(|n| format!("{}.xml", n.to_ascii_lowercase()))).collect();
    let all_listed: Vec<String> = docs.iter().filter(|x| x.2 == DocKind::FileList).flat_map(|x| modifier_file_list(&x.1)).collect();
    let listed: Vec<String> = all_listed.iter().filter(|l| !cycle_names.contains(&base_name(l).to_ascii_lowercase())).cloned().collect();
    let mods: Vec<(usize, Vec<Modifier>)> = docs.iter().enumerate().filter(|x| x.1 .2 == DocKind::Modifiers).map(|(i, x)| (i, modifiers(&x.1))).collect();
    let names: Vec<Vec<String>> = mods.iter().map(|(_, m)| m.iter().map(|x| x.name.clone()).collect()).collect();
    let mod_files = name_files(&listed, &names);
    let mut used = std::collections::BTreeSet::new();
    for ((i, ms), nf) in mods.iter().zip(&mod_files) {
        let file = nf.as_ref().map(|x| base_name(&x.0));
        if let Some(f) = &file {
            used.insert(f.to_ascii_lowercase());
        }
        d.files.push(json!({"source": docs[*i].0, "kind": "modifiers", "file": file, "fit": nf.as_ref().map(|x| (x.1 * 100.0).round() / 100.0), "count": ms.len()}));
        for m in ms {
            let mut j = m.to_json();
            j["file"] = json!(file);
            j["source"] = json!(docs[*i].0);
            d.modifiers.push(j);
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for l in &listed {
        let b = base_name(l);
        let low = b.to_ascii_lowercase();
        if !used.contains(&low) && seen.insert(low) {
            d.missing.push(b);
        }
    }
    // weather list: cycle file names
    let mut cycle_files: Vec<String> = all_listed.iter().map(|l| base_name(l)).collect();
    for (id, root, k) in &docs {
        match k {
            DocKind::Weather => {
                if let Some(l) = root.child("WeatherTimeCycleFileNames") {
                    cycle_files.extend(l.children.iter().map(|c| base_name(&c.text)));
                }
                d.files.push(json!({"source": id, "kind": "weather", "file": null}));
                d.weather = to_json(root);
            }
            DocKind::CustomClouds => {
                let v = to_json(root);
                let n = v.pointer("/CustomCloudGroupList").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
                d.files.push(json!({"source": id, "kind": "custom_clouds", "file": null, "count": n}));
                d.clouds = v;
            }
            DocKind::TimeSamples => {
                d.files.push(json!({"source": id, "kind": "time_samples", "file": null}));
                d.time = time_samples(root);
            }
            DocKind::FileList => d.files.push(json!({"source": id, "kind": "file_list", "file": root.child("id").map(|x| base_name(&x.text)), "count": modifier_file_list(root).len()})),
            _ => {}
        }
    }
    for (id, root, k) in &docs {
        if *k != DocKind::Cycle {
            continue;
        }
        let Some(c) = cycle(root) else { continue };
        let lc = c.name.to_ascii_lowercase();
        let file = cycle_files.iter().find(|f| {
            let fl = f.to_ascii_lowercase();
            fl == format!("w_{lc}.xml") || fl == format!("{lc}.xml")
        });
        let mut j = c.to_json();
        j["file"] = json!(file);
        j["source"] = json!(id);
        d.files.push(json!({"source": id, "kind": "cycle", "file": file, "cycle": c.name}));
        d.cycles.push(j);
    }
    d.files.sort_by_key(|f| off(f["source"].as_str().unwrap_or("")));
    Ok(d)
}

/// The modifiers a scene needs: those whose name or file matches `re`,
/// plus every submodifier they pull in.
pub fn select(mods: &[Value], re: &regex::Regex) -> Vec<Value> {
    let by_name: std::collections::HashMap<&str, &Value> = mods.iter().filter_map(|m| Some((m["name"].as_str()?, m))).collect();
    let mut want: Vec<&str> = mods.iter().filter(|m| re.is_match(m["name"].as_str().unwrap_or("")) || re.is_match(m["file"].as_str().unwrap_or(""))).filter_map(|m| m["name"].as_str()).collect();
    let mut seen: std::collections::BTreeSet<&str> = want.iter().copied().collect();
    let mut i = 0;
    while i < want.len() {
        if let Some(subs) = by_name.get(want[i]).and_then(|m| m["submodifiers"].as_array()) {
            for s in subs.iter().filter_map(Value::as_str) {
                if by_name.contains_key(s) && seen.insert(s) {
                    want.push(s);
                }
            }
        }
        i += 1;
    }
    want.iter().filter_map(|n| by_name.get(n).map(|v| (*v).clone())).collect()
}

/// Variable index: per variable name, its family (text before the first
/// `_`) and how many modifiers and cycles set it.
pub fn variables(d: &Decoded) -> Value {
    let mut m: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for x in &d.modifiers {
        let mut names: Vec<&String> = x["vars"].as_object().map(|o| o.keys().collect()).unwrap_or_default();
        if let Some(axes) = x["axes"].as_array() {
            names.extend(axes.iter().filter_map(|a| a["vars"].as_object()).flat_map(|o| o.keys()));
        }
        names.sort();
        names.dedup();
        for n in names {
            m.entry(n.clone()).or_default().0 += 1;
        }
    }
    for c in &d.cycles {
        let names: std::collections::BTreeSet<&String> = c["regions"].as_object().into_iter().flat_map(|r| r.values()).filter_map(Value::as_object).flat_map(|o| o.keys()).collect();
        for n in names {
            m.entry(n.clone()).or_default().1 += 1;
        }
    }
    Value::Object(
        m.into_iter()
            .map(|(k, (a, b))| {
                let fam = k.split('_').next().unwrap_or("").to_string();
                (k, json!({"family": fam, "modifiers": a, "cycles": b}))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODS: &str = "\u{feff}<?xml version=\"1.0\"?>\n<timecycle_modifier_data version=\"1.000000\">\n  <!-- c -->\n  <modifier\n    name=\"Overlay_ArcadeCabinetCRT\"\n    description=\"\"\n    groupApplyTag=\"animpostfx\"\n    localApplyIndex=\"0\"\n    userFlags=\"0\"\n  >\n    <postfx_noise_size>0.600000 0.000000</postfx_noise_size>\n    <postfx_noise>1.000000 0.000000</postfx_noise>\n  </modifier>\n  <modifier name=\"Empty\" userFlags=\"4\"\n   />\n  <modifier name=\"Module\">\n    <Fog>\n      <input_min>0.7</input_min>\n      <output_max>0.9</output_max>\n    </Fog>\n  </modifier>\n  <modifier name=\"Anim\" numAxes=\"1\" numKeyframes=\"2\">\n    <submodifiers>\n      <item>Sub_A</item>\n    </submodifiers>\n    <axis controlValue=\"heartbeat\" blendType=\"cubicEaseIn\">\n      <keyframe_t_value>0.0 1.0</keyframe_t_value>\n      <voltage_fluctuation_range>10.0 20.0</voltage_fluctuation_range>\n    </axis>\n  </modifier>\n</timecycle_modifier_data>\n";

    #[test]
    fn xml_reader() {
        assert_eq!(parse_xml("<a n=\"p\r\n\"/>").unwrap().attr("n"), Some("p "));
        let r = parse_xml("<a x=\"1\" y='&amp;2'><b>t &lt; u</b><c/><!-- no --><d><![CDATA[<raw>]]></d></a>").unwrap();
        assert_eq!(r.name, "a");
        assert_eq!(r.attr("y"), Some("&2"));
        assert_eq!(r.child("b").unwrap().text, "t < u");
        assert!(r.child("c").unwrap().children.is_empty());
        assert_eq!(r.child("d").unwrap().text, "<raw>");
    }

    #[test]
    fn several_documents() {
        let v = parse_xml_all("<?xml version=\"1.0\"?>\n<a><m name=\"x\"/></a><?xml version=\"1.0\"?>\r\n<a><m name=\"y\"/></a>").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].children[0].attr("name"), Some("y"));
    }

    #[test]
    fn modifier_records() {
        let r = parse_xml(MODS).unwrap();
        assert_eq!(doc_kind(&r), Some(DocKind::Modifiers));
        let m = modifiers(&r);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].name, "Overlay_ArcadeCabinetCRT");
        assert_eq!(m[0].vars, vec![("postfx_noise_size".to_string(), vec![0.6, 0.0]), ("postfx_noise".to_string(), vec![1.0, 0.0])]);
        let j = m[0].to_json();
        assert_eq!(j["group_apply_tag"], "animpostfx");
        assert_eq!(j["user_flags"], 0);
        assert_eq!(j["vars"]["postfx_noise_size"]["value"], 0.6);
        assert!(m[1].vars.is_empty());
        let mut d = m[0].clone();
        d.vars.push(("postfx_noise".into(), vec![2.0, 0.0]));
        let j = d.to_json();
        assert_eq!(j["vars"]["postfx_noise"]["value"], 2.0);
        assert_eq!(j["repeated"]["postfx_noise"], json!([[1.0, 0.0], [2.0, 0.0]]));
        assert!(m[0].to_json().get("repeated").is_none());
        assert!(m[2].vars.is_empty());
        assert_eq!(m[2].to_json()["high_level_vars"]["Fog"], json!({"input_min": 0.7, "output_max": 0.9}));
        assert_eq!(m[3].submodifiers, vec!["Sub_A"]);
        assert_eq!(m[3].axes[0].vars[1], ("voltage_fluctuation_range".to_string(), vec![10.0, 20.0]));
        assert_eq!(m[3].to_json()["axes"][0]["control_value"], "heartbeat");
    }

    #[test]
    fn cycle_and_samples() {
        let r = parse_xml("<timecycle_keyframe_data><High_Level_Vars><Fog> 0.5</Fog></High_Level_Vars><cycle name=\"Sunny\" regions=\"2\"><region name=\"GLOBAL\"><fog_density> 0.1 0.2 0.3</fog_density></region><region name=\"URBAN\"/></cycle></timecycle_keyframe_data>").unwrap();
        assert_eq!(doc_kind(&r), Some(DocKind::Cycle));
        let c = cycle(&r).unwrap();
        assert_eq!(c.name, "Sunny");
        assert_eq!(c.regions[0].1[0], ("fog_density".to_string(), vec![0.1, 0.2, 0.3]));
        assert_eq!(c.to_json()["high_level_vars"]["Fog"], 0.5);
        let t = parse_xml("<time_data mode=\"dynamic\"><sample name=\"00:00\" hour=\"0\" uw_tc_mod=\"underwater\"/><region name=\"URBAN\"/></time_data>").unwrap();
        let s = time_samples(&t);
        assert_eq!(s["samples"][0]["hour"], 0);
        assert_eq!(s["samples"][0]["uw_tc_mod"], "underwater");
        assert_eq!(s["regions"][0], "URBAN");
    }

    #[test]
    fn generic_json() {
        let r = parse_xml("<G><Name>arcade</Name><Position x=\"1.5\" y=\"2\"/><Density value=\"-10.0\"/><On value=\"true\"/><L><Item>a</Item><Item>b</Item></L></G>").unwrap();
        let j = to_json(&r);
        assert_eq!(j["Name"], "arcade");
        assert_eq!(j["Position"], json!([1.5, 2.0]));
        assert_eq!(j["Density"], -10.0);
        assert_eq!(j["On"], true);
        assert_eq!(j["L"], json!(["a", "b"]));
    }

    #[test]
    fn file_naming() {
        let listed: Vec<String> = ["timecycle_mods_1.xml", "TC_mods_area_DN.xml", "TC_mods_Arcade_Games.xml", "TC_mods_APFX.xml", "TC_mods_area_DS.xml", "TC_mods_area_BK.xml"].iter().map(|s| format!("$(common)/data/timecycle/{s}")).collect();
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // in offset order: APFX, Arcade, BK, DN, (DS absent), timecycle_mods_1
        let files = vec![v(&["Access_HighContrast"]), v(&["ARCADE_A_1", "ARCADE_B_1"]), v(&["bk04_foy_1"]), v(&["dn28_off_1"]), v(&["NoAmbientmult"])];
        let n = name_files(&listed, &files);
        let names: Vec<&str> = n.iter().map(|x| x.as_ref().unwrap().0.rsplit('/').next().unwrap()).collect();
        assert_eq!(names, ["TC_mods_APFX.xml", "TC_mods_Arcade_Games.xml", "TC_mods_area_BK.xml", "TC_mods_area_DN.xml", "timecycle_mods_1.xml"]);
        assert_eq!(n[1].as_ref().unwrap().1, 1.0);
        assert!(fit("TC_mods_area_TW2.xml", &v(&["tw2_06_cnv"])) == 1.0 && fit("TC_mods_area_TW2.xml", &v(&["tw3_08_trl"])) < 0.0);
    }
}
