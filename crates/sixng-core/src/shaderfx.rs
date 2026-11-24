//! Shader programs and effects of `shaders.rpf` (its uncompressed part):
//! which resources and constants each compiled program declares, and which
//! textures and constants each effect (a material's shader) has.
//!
//! Layout, all little-endian (decoded from the data; counts are from one
//! install):
//!
//! * Six program sections follow one another, one per stage class: vertex
//!   and mesh, pixel, geometry, two empty ones, compute. A section is
//!   `u32 count` and `count` program records.
//! * A program record: `u8 n`, a name of `n - 1` bytes, NUL, `u8 kind`,
//!   `u32 size`, `u32 0x000ddce7`, `u32 size`, `u32 unpacked size`, then
//!   `size` bytes (a raw deflate stream that inflates to the compiled
//!   program, and a 12-byte trailer: `u64 id`, `u32 mask`), a 140-byte state
//!   block (not decoded) and a variable table.
//! * A variable table: `u32 size` (of what follows), `u16 resources`,
//!   `u16 members`, `u16 ?`, `u16 size again`, then 24-byte records and a
//!   name table. A record is `u8 kind, u8 dimension, u16 index`, `u32 a`,
//!   `u32 b`, `u32 c`, `u32 name`, `u32 alias` (both joaat hashes; the alias
//!   differs only where a material parameter is named differently from the
//!   shader variable: variable `SpecTexture`, parameter `speculartex`).
//!   Resources come first, then the members of the constant buffers. A
//!   constant buffer's `a` holds its member count (low byte) and the byte
//!   distance from its own record to its first member (high half); `c` is
//!   its size. A member's `a` is its byte offset in the buffer.
//! * After the sections: `u32 count` and the effect records. An effect
//!   record: `u8 n`, name, NUL, `u32 ?`, six program lists (`u32 n`, n
//!   program numbers within the section of the same order), a body whose
//!   first part is not decoded, a variable table for the whole effect
//!   (preceded by `u32 0x80000000`), and last a name table (`u32 size`,
//!   strings) with technique and pass names. In the effect's table a
//!   material texture is a resource with flag 0x80: the low byte of its
//!   index is its slot in the material's texture list, and a sampler's `b`
//!   is the distance from its record to its name in the table's name table.
//!
//! Techniques and passes of an effect sit in the part of the body that is
//! otherwise not decoded: `u32 techniques`, one `u32` per technique (bits
//! 0..14 = offset of its name in the effect's last name table, bits 15..23
//! = index of its first pass, bits 24..31 = number of passes), `u32
//! passes`, then 24 bytes per pass: six bytes with the pass's program per
//! section (1-based into the effect's program list of that section, 0 =
//! none), eight flag bytes, `u16` name offset, and six bytes that are not
//! understood. The table is found by these rules (the first name offset is
//! 1, offsets rise, each technique's first pass follows the previous one's
//! passes, the pass count is their sum, program numbers are in range), not
//! by decoding what precedes it.
//!
//! The archive's one compressed entry is a pipeline cache (magic
//! `PS\0\x09`): a 0x80-byte header with a version string at 0x24 and six
//! `u32` counts at 0x50 (graphics pipelines, compute pipelines, metadata
//! blobs, code blobs, metadata bytes, code bytes); graphics pipeline
//! records of 0x70 bytes (seven `(u32 code, u32 metadata)` pairs from 0x38,
//! `ffffffff` = none); compute pipeline records of 0x18 bytes (code at
//! 0x10, metadata at 0x14); a metadata directory and a code directory of
//! `(u32 offset, u32 size)`; the metadata area; the code area. A stage's
//! metadata blob is a 16- or 24-byte header and 32-byte records: `0x41`
//! (where the code finds a resource; not decoded further), `0x42` (a
//! resource the code uses: `u8 access` at 8 (1 read, 2 write), `u8 type`
//! at 10, `u32 register` at 12, `u32 space` at 16) and one `0x43` with the
//! 16-byte digest that is also bytes 4..20 of the compiled program's `HASH`
//! chunk. That digest joins a pipeline stage to a program record.
//!
//! A read record's (space, register) is the declared resource with flags
//! 0x40 = space 1, 0x80 = space 2, else space 0, and register = index, plus
//! 256 when the kind has bit 0x80. This holds for every read record of
//! every joined stage (333,965). Write records use another numbering that
//! was not worked out.
//!
//! Effect records have no length field that was understood, so the next
//! record is found by trying each position: its header must parse with
//! program numbers in range, and the bytes before it must be a name table
//! that ends exactly there.

use anyhow::{anyhow, Result};
use serde::Serialize;

const MAGIC: [u8; 4] = [0xe7, 0xdc, 0x0d, 0x00];
const STATE: usize = 140;

/// One record of a variable table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Var {
    /// joaat of the shader variable's name (lower case).
    pub name: u32,
    /// joaat of the material parameter's name; equal to `name` for most.
    pub alias: u32,
    pub kind: u8,
    pub dim: u8,
    pub index: u16,
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Var {
    /// Resource kinds that were checked against known cases: `texture`
    /// (0x02; 0x42 = an array), `sampler` (0x23), `cbuffer` (0x30, 0xb0),
    /// `rw_texture` (0x1c). Others are given as `kind_xx`.
    pub fn resource_kind(&self) -> String {
        match self.kind {
            0x02 | 0x82 => "texture".into(),
            0x42 | 0xc2 => "texture_array".into(),
            0x1c => "rw_texture".into(),
            0x23 => "sampler".into(),
            0x30 | 0xb0 => "cbuffer".into(),
            k => format!("kind_{k:02x}"),
        }
    }
    pub fn is_cbuffer(&self) -> bool {
        self.kind & 0x7f == 0x30
    }
    /// Scope flags of a resource (second byte of `a`): 0x80 = material,
    /// 0x40 and 0x02 seen on shared resources (meaning not pinned).
    pub fn flags(&self) -> u8 {
        (self.a >> 8) as u8
    }
    /// Register space of a resource in the compiled code: 2 for the
    /// material's (flag 0x80), 1 for flag 0x40, else 0.
    pub fn space(&self) -> u32 {
        match self.flags() {
            f if f & 0x80 != 0 => 2,
            f if f & 0x40 != 0 => 1,
            _ => 0,
        }
    }
    /// Register of a resource in the compiled code's read records.
    pub fn register(&self) -> u32 {
        self.index as u32 + if self.kind & 0x80 != 0 { 256 } else { 0 }
    }
    /// A material texture of an effect table: (slot in the material's
    /// texture list, is an array).
    pub fn material_texture(&self) -> Option<(u8, bool)> {
        (self.flags() & 0x80 != 0 && matches!(self.kind & 0x3f, 0x02)).then_some((self.index as u8, self.kind & 0x40 != 0))
    }
    /// Type of a constant buffer member, where the size was checked from
    /// the spacing of offsets: float (0x09), float2, float3, float4,
    /// float4x4 (0x0e). Others are given as `type_xx`.
    pub fn member_type(&self) -> String {
        match self.kind {
            0x09 => "float".into(),
            0x0a => "float2".into(),
            0x0b => "float3".into(),
            0x0c => "float4".into(),
            0x0e => "float4x4".into(),
            k => format!("type_{k:02x}"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Table {
    pub resources: Vec<Var>,
    pub members: Vec<Var>,
    #[serde(skip)]
    pub names: Vec<u8>,
}

impl Table {
    /// The members of the constant buffer at `resources[i]`.
    pub fn members_of(&self, i: usize) -> &[Var] {
        let Some(r) = self.resources.get(i).filter(|r| r.is_cbuffer()) else { return &[] };
        let (n, first) = ((r.a & 0xff) as usize, i + (r.a >> 16) as usize / 24);
        let Some(first) = first.checked_sub(self.resources.len()) else { return &[] };
        self.members.get(first..first + n).unwrap_or(&[])
    }
    /// The name of the sampler at `resources[i]`, from the table's name
    /// table (effect tables): its `b` is the distance from its own record
    /// to the name.
    pub fn sampler_name(&self, i: usize) -> Option<&str> {
        let r = self.resources.get(i).filter(|r| r.kind == 0x23)?;
        let at = (24 * i + r.b as usize).checked_sub(24 * (self.resources.len() + self.members.len()))?;
        let s = self.names.get(at..)?;
        std::str::from_utf8(&s[..s.iter().position(|&c| c == 0)?]).ok().filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    /// Stage class: 0 vertex or mesh, 1 pixel, 2 geometry, 5 compute.
    pub section: u8,
    pub name: String,
    pub kind: u8,
    pub id: u64,
    pub mask: u32,
    /// File offset and length of the deflate stream.
    pub data: (usize, usize),
    pub table: Table,
}

/// One pass of a technique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pass {
    pub name: String,
    /// Per section the pass's program: an index into the effect's
    /// `programs` list of that section.
    pub programs: [Option<u8>; 6],
    /// The eight flag bytes (not decoded).
    pub flags: [u8; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Technique {
    pub name: String,
    pub passes: Vec<Pass>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Effect {
    pub name: String,
    /// Program numbers per section (same order as the sections).
    pub programs: [Vec<u32>; 6],
    pub table: Option<Table>,
    /// Empty when the effect has no programs or its tables were not found.
    pub techniques: Vec<Technique>,
}

#[derive(Debug, Default)]
pub struct Shaders {
    pub programs: Vec<Program>,
    /// Index of each section's first program in `programs`, and its count.
    pub sections: [(usize, usize); 6],
    pub effects: Vec<Effect>,
    /// The effect count the file states.
    pub effects_stated: usize,
}

fn u16_at(b: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(p..p + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

/// A variable table at `p` (its `u32 size`): the table and the offset
/// after it.
fn table_at(b: &[u8], p: usize) -> Option<(Table, usize)> {
    let size = u32_at(b, p)? as usize;
    let (na, nb, size2) = (u16_at(b, p + 4)? as usize, u16_at(b, p + 6)? as usize, u16_at(b, p + 10)? as usize);
    let recs = 12 + 24 * (na + nb);
    if size2 != size & 0xffff || size < recs - 4 {
        return None;
    }
    let end = p + 4 + size;
    let var = |i: usize| -> Option<Var> {
        let q = p + 12 + 24 * i;
        let t = u32_at(b, q)?;
        Some(Var { kind: t as u8, dim: (t >> 8) as u8, index: (t >> 16) as u16, a: u32_at(b, q + 4)?, b: u32_at(b, q + 8)?, c: u32_at(b, q + 12)?, name: u32_at(b, q + 16)?, alias: u32_at(b, q + 20)? })
    };
    let resources = (0..na).map(var).collect::<Option<Vec<_>>>()?;
    let members = (na..na + nb).map(var).collect::<Option<Vec<_>>>()?;
    Some((Table { resources, members, names: b.get(p + recs..end)?.to_vec() }, end))
}

/// `u8 n`, name, NUL at `p`: the name and the offset after the NUL.
fn name_at(b: &[u8], p: usize) -> Option<(&str, usize)> {
    let n = *b.get(p)? as usize;
    if n < 2 || *b.get(p + n)? != 0 {
        return None;
    }
    let s = b.get(p + 1..p + n)?;
    s.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.')).then(|| std::str::from_utf8(s).ok().map(|s| (s, p + n + 1))).flatten()
}

fn program_at(b: &[u8], p: usize, section: u8) -> Option<(Program, usize)> {
    let (name, q) = name_at(b, p)?;
    let (kind, size) = (*b.get(q)?, u32_at(b, q + 1)? as usize);
    if b.get(q + 5..q + 9)? != MAGIC || u32_at(b, q + 9)? as usize != size || size < 12 {
        return None;
    }
    let data = q + 17;
    let end = data + size;
    let id = u64::from_le_bytes(b.get(end - 12..end - 4)?.try_into().ok()?);
    let (table, next) = table_at(b, end + STATE)?;
    Some((Program { section, name: name.to_string(), kind, id, mask: u32_at(b, end - 4)?, data: (data, size - 12), table }, next))
}

/// Effect header at `p`: name, six program lists, offset of the body.
fn effect_header(b: &[u8], p: usize, counts: &[usize; 6]) -> Option<(String, [Vec<u32>; 6], usize)> {
    let (name, mut q) = name_at(b, p)?;
    if name.contains(':') {
        return None;
    }
    q += 4;
    let mut lists: [Vec<u32>; 6] = Default::default();
    for (k, l) in lists.iter_mut().enumerate() {
        let n = u32_at(b, q)? as usize;
        q += 4;
        if n > counts[k] {
            return None;
        }
        for i in 0..n {
            let v = u32_at(b, q + 4 * i)?;
            if v as usize >= counts[k] {
                return None;
            }
            l.push(v);
        }
        q += 4 * n;
    }
    Some((name.to_string(), lists, q))
}

/// True when a name table (`u32 size`, NUL-separated strings) ends at `p`.
fn name_table_ends_at(b: &[u8], p: usize, floor: usize) -> bool {
    if p == 0 || b[p - 1] != 0 {
        return false;
    }
    (1..=0x10000usize).take_while(|l| p >= floor + 4 + l).any(|l| u32_at(b, p - 4 - l) == Some(l as u32) && b[p - l..p].iter().all(|&c| c == 0 || (0x20..0x7f).contains(&c)))
}

/// The effect's own variable table inside its body.
fn effect_table(body: &[u8]) -> Option<Table> {
    (0..body.len().saturating_sub(16)).filter(|&k| body[k..k + 4] == [0, 0, 0, 0x80]).find_map(|k| {
        let (t, _) = table_at(body, k + 4)?;
        let all: Vec<&Var> = t.resources.iter().chain(&t.members).collect();
        (!all.is_empty() && all.iter().filter(|v| v.name == v.alias).count() * 2 >= all.len()).then_some(t)
    })
}

/// The technique and pass tables inside an effect's body, named from the
/// name table that follows them.
fn techniques(body: &[u8], lists: &[Vec<u32>; 6]) -> Vec<Technique> {
    let ident = |at: usize| body.get(at).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_');
    for q in 0..body.len().saturating_sub(12) {
        let Some(nt) = u32_at(body, q).map(|n| n as usize).filter(|n| (1..=400).contains(n)) else { continue };
        let Some(ents) = (0..nt).map(|k| u32_at(body, q + 4 + 4 * k)).collect::<Option<Vec<u32>>>() else { continue };
        let (off, first, count) = (|e: u32| (e & 0x7fff) as usize, |e: u32| ((e >> 15) & 0x1ff) as usize, |e: u32| (e >> 24) as usize);
        let np = ents.iter().map(|&e| count(e)).sum::<usize>();
        let mut next = 0;
        let in_order = ents.iter().all(|&e| {
            let ok = first(e) == next & 0x1ff;
            next += count(e);
            ok
        });
        if off(ents[0]) != 1 || !in_order || np == 0 || ents.windows(2).any(|w| off(w[1]) <= off(w[0])) || u32_at(body, q + 4 + 4 * nt) != Some(np as u32) {
            continue;
        }
        let pq = q + 8 + 4 * nt;
        let Some(passes) = body.get(pq..pq + 24 * np) else { continue };
        if passes.chunks_exact(24).any(|x| (0..6).any(|j| x[j] as usize > lists[j].len())) || !passes.chunks_exact(24).any(|x| x[..6].iter().any(|&b| b != 0)) {
            continue;
        }
        // the name table: `u32 size`, strings; every name offset must start an identifier right after a NUL
        let mut need: Vec<usize> = ents.iter().map(|&e| off(e)).chain(passes.chunks_exact(24).map(|x| u16::from_le_bytes([x[14], x[15]]) as usize)).collect();
        need.sort_unstable();
        let Some(names) = (pq + 24 * np + 4..body.len()).find_map(|st| {
            let l = u32_at(body, st - 4)? as usize;
            let t = body.get(st..st + l)?;
            (t.first() == Some(&0) && *need.last()? < l && need.iter().all(|&o| o >= 1 && t[o - 1] == 0 && ident(st + o))).then_some(t)
        }) else { continue };
        let name = |o: usize| String::from_utf8_lossy(&names[o..o + names[o..].iter().position(|&c| c == 0).unwrap_or(0)]).into_owned();
        let mut it = passes.chunks_exact(24);
        return ents
            .iter()
            .map(|&e| Technique {
                name: name(off(e)),
                passes: it
                    .by_ref()
                    .take(count(e))
                    .map(|x| Pass { name: name(u16::from_le_bytes([x[14], x[15]]) as usize), programs: std::array::from_fn(|j| x[j].checked_sub(1)), flags: x[6..14].try_into().unwrap() })
                    .collect(),
            })
            .collect();
    }
    Vec::new()
}

/// Reads the program sections and the effect records from the bytes of
/// `shaders.rpf`.
pub fn read(b: &[u8]) -> Result<Shaders> {
    // the first program record: its magic, then back to its name length; the section count is the u32 before it
    let first = b.windows(4).position(|w| w == MAGIC).ok_or_else(|| anyhow!("no shader program record found"))?;
    let start = (first.saturating_sub(262)..first.saturating_sub(6)).rev().find(|&s| b[s] as usize + 6 == first - s && name_at(b, s).is_some()).ok_or_else(|| anyhow!("first program record does not parse"))?;
    let mut out = Shaders::default();
    let mut p = start.checked_sub(4).ok_or_else(|| anyhow!("no section count"))?;
    for s in 0..6 {
        let n = u32_at(b, p).ok_or_else(|| anyhow!("section {s}: truncated"))? as usize;
        p += 4;
        out.sections[s] = (out.programs.len(), n);
        for i in 0..n {
            let (prog, next) = program_at(b, p, s as u8).ok_or_else(|| anyhow!("section {s}: program {i} at {p:#x} does not parse"))?;
            out.programs.push(prog);
            p = next;
        }
    }
    let counts = out.sections.map(|s| s.1);
    out.effects_stated = u32_at(b, p).ok_or_else(|| anyhow!("no effect count"))? as usize;
    p += 4;
    let mut cur = effect_header(b, p, &counts).map(|h| (p, h));
    while let Some((_, (name, programs, body))) = cur.take() {
        // the next record: the nearest position after this body whose header parses and that a name table ends at
        let limit = (body + (64 << 20)).min(b.len());
        let next = if out.effects.len() + 1 >= out.effects_stated { None } else { (body + 1..limit).find_map(|q| if b[q - 1] == 0 && b[q] >= 2 { effect_header(b, q, &counts).filter(|_| name_table_ends_at(b, q, body)).map(|h| (q, h)) } else { None }) };
        let end = next.as_ref().map_or(limit.min(body + (4 << 20)), |n| n.0);
        let techniques = if programs.iter().all(Vec::is_empty) { Vec::new() } else { techniques(&b[body..end], &programs) };
        out.effects.push(Effect { name, table: effect_table(&b[body..end]), techniques, programs });
        cur = next;
    }
    Ok(out)
}

impl Program {
    /// The 16-byte digest of the compiled program (bytes 4..20 of its
    /// `HASH` chunk), which the pipeline cache names a stage by. `file` is
    /// the bytes `read` was given.
    pub fn digest(&self, file: &[u8]) -> Option<[u8; 16]> {
        use std::io::Read;
        let mut d = Vec::new();
        flate2::read::DeflateDecoder::new(file.get(self.data.0..self.data.0 + self.data.1)?).read_to_end(&mut d).ok()?;
        if d.get(..4)? != b"DXBC" {
            return None;
        }
        (0..u32_at(&d, 28)? as usize).find_map(|i| {
            let at = u32_at(&d, 32 + 4 * i)? as usize;
            (d.get(at..at + 4)? == b"HASH").then(|| d.get(at + 12..at + 28)?.try_into().ok()).flatten()
        })
    }
}

/// A resource that a compiled stage uses (a `0x42` record of its metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Use {
    pub write: bool,
    pub kind: u8,
    pub register: u32,
    pub space: u32,
}

/// One stage's metadata blob of the pipeline cache.
#[derive(Debug, Clone, Default)]
pub struct Stage {
    pub digest: [u8; 16],
    pub uses: Vec<Use>,
    /// Number of `0x41` records.
    pub locations: usize,
    /// Pipelines whose stage this is.
    pub pipelines: usize,
}

#[derive(Debug, Default)]
pub struct Cache {
    pub version: String,
    pub graphics: usize,
    pub compute: usize,
    pub metadata_blobs: usize,
    pub code_blobs: usize,
    /// The stage blobs, by metadata index.
    pub stages: std::collections::BTreeMap<u32, Stage>,
}

fn stage_at(b: &[u8]) -> Option<Stage> {
    if !matches!(u32_at(b, 0)?, 2 | 0x403) {
        return None;
    }
    let head = if b.len() > 16 && (b.len() - 16) % 32 == 0 { 16 } else { 24 };
    if b.len() <= head || (b.len() - head) % 32 != 0 {
        return None;
    }
    let mut st = Stage::default();
    let mut digest = None;
    for r in b[head..].chunks_exact(32) {
        match u32_at(r, 0)? {
            0x41 => st.locations += 1,
            0x42 => st.uses.push(Use { write: r[8] == 2, kind: r[10], register: u32_at(r, 12)?, space: u32_at(r, 16)? }),
            0x43 => digest = Some(r[4..20].try_into().ok()?),
            _ => return None,
        }
    }
    st.digest = digest?;
    Some(st)
}

/// Reads the pipeline cache (the unpacked bytes of the archive's compressed
/// entry).
pub fn read_cache(b: &[u8]) -> Result<Cache> {
    if b.get(..4) != Some(b"PS\0\x09") || b.len() < 0x80 {
        return Err(anyhow!("not a pipeline cache"));
    }
    let n = |i: usize| u32_at(b, 0x50 + 4 * i).unwrap_or(0) as usize;
    let (ng, nc, nm, nk, meta_bytes, code_bytes) = (n(0), n(1), n(2), n(3), n(4), n(5));
    let (g, c) = (0x80, 0x80 + ng * 0x70);
    let md = c + nc * 0x18;
    let kd = md + nm * 8;
    let ma = kd + nk * 8;
    if ma + meta_bytes + code_bytes != b.len() {
        return Err(anyhow!("pipeline cache: the areas do not add up to the file size"));
    }
    let version = b[0x24..0x48].split(|&x| x == 0).next().map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_default();
    let mut out = Cache { version, graphics: ng, compute: nc, metadata_blobs: nm, code_blobs: nk, ..Default::default() };
    for i in 0..nm {
        let (Some(o), Some(sz)) = (u32_at(b, md + 8 * i), u32_at(b, md + 8 * i + 4)) else { continue };
        if let Some(st) = b.get(ma + o as usize..ma + o as usize + sz as usize).and_then(stage_at) {
            out.stages.insert(i as u32, st);
        }
    }
    let metas = (0..ng).flat_map(|i| (0..7).map(move |k| g + i * 0x70 + 0x38 + 8 * k + 4)).chain((0..nc).map(|i| c + i * 0x18 + 0x14));
    for at in metas {
        if let Some(st) = u32_at(b, at).and_then(|m| out.stages.get_mut(&m)) {
            st.pipelines += 1;
        }
    }
    Ok(out)
}

/// What the compiled code of one program uses, from the cache.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Compiled {
    /// Pipelines with this program as a stage.
    pub pipelines: usize,
    /// Per declared resource: true when a read record names it.
    pub read: Vec<bool>,
    /// Read records that match no declared resource.
    pub unmatched_reads: usize,
    /// Write records (their numbering is not matched to the declarations).
    pub writes: Vec<Use>,
}

impl Shaders {
    /// Joins the cache's stages to the programs by digest: for each
    /// program (same order as `programs`) what its compiled code uses, or
    /// None when no pipeline has it. `file` is the bytes `read` was given.
    pub fn compiled(&self, cache: &Cache, file: &[u8]) -> Vec<Option<Compiled>> {
        use rayon::prelude::*;
        let mut by: std::collections::HashMap<[u8; 16], Vec<&Stage>> = Default::default();
        for st in cache.stages.values() {
            by.entry(st.digest).or_default().push(st);
        }
        self.programs
            .par_iter()
            .map(|p| {
                let stages = by.get(&p.digest(file)?)?;
                let mut c = Compiled { pipelines: stages.iter().map(|s| s.pipelines).sum(), read: vec![false; p.table.resources.len()], ..Default::default() };
                // the blobs of one program carry the same records: read the first
                for u in &stages[0].uses {
                    if u.write {
                        c.writes.push(*u);
                    } else if let Some(i) = p.table.resources.iter().position(|r| r.kind != 0x23 && r.space() == u.space && r.register() == u.register && r.is_cbuffer() == (u.kind == 0x0d)) {
                        c.read[i] = true;
                    } else {
                        c.unmatched_reads += 1;
                    }
                }
                Some(c)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(kind: u8, dim: u8, index: u16, a: u32, b: u32, c: u32, name: u32, alias: u32) -> Vec<u8> {
        let mut v = (kind as u32 | (dim as u32) << 8 | (index as u32) << 16).to_le_bytes().to_vec();
        for x in [a, b, c, name, alias] {
            v.extend(x.to_le_bytes());
        }
        v
    }
    fn table(res: &[Vec<u8>], mem: &[Vec<u8>], names: &[u8]) -> Vec<u8> {
        let body: Vec<u8> = res.iter().chain(mem).flatten().copied().chain(names.iter().copied()).collect();
        let size = (8 + body.len()) as u32;
        let mut v = size.to_le_bytes().to_vec();
        v.extend((res.len() as u16).to_le_bytes());
        v.extend((mem.len() as u16).to_le_bytes());
        v.extend(0u16.to_le_bytes());
        v.extend((size as u16).to_le_bytes());
        v.extend(body);
        v
    }
    fn program(name: &str, t: &[u8]) -> Vec<u8> {
        let mut v = vec![name.len() as u8 + 1];
        v.extend(name.bytes());
        v.extend([0, 0x32]);
        let payload = [vec![0xaau8; 5], 7u64.to_le_bytes().to_vec(), 1u32.to_le_bytes().to_vec()].concat();
        v.extend((payload.len() as u32).to_le_bytes());
        v.extend(MAGIC);
        v.extend((payload.len() as u32).to_le_bytes());
        v.extend(99u32.to_le_bytes());
        v.extend(payload);
        v.extend([0u8; STATE]);
        v.extend(t);
        v
    }
    fn effect(name: &str, lists: [&[u32]; 6], body: &[u8], names: &[u8]) -> Vec<u8> {
        let mut v = vec![name.len() as u8 + 1];
        v.extend(name.bytes());
        v.extend([0, 1, 2, 3, 4]);
        for l in lists {
            v.extend((l.len() as u32).to_le_bytes());
            v.extend(l.iter().flat_map(|x| x.to_le_bytes()));
        }
        v.extend(body);
        v.extend((names.len() as u32).to_le_bytes());
        v.extend(names);
        v
    }

    #[test]
    fn programs_effects_and_their_tables() {
        // a pixel program: one texture, one constant buffer of 0x20 bytes with two members (its record is 1 before the first member)
        let t = table(&[var(0x02, 1, 237, 0x200, 0, 0, 0x11, 0x11), var(0x30, 4, 4, 2 | 0x82 << 8 | 24 << 16, 0, 0x20, 0x22, 0x22)], &[var(0x09, 0, 0, 0, 0, 0, 0x31, 0x31), var(0x0c, 0, 0, 0x10, 0, 0, 0x32, 0x32)], &[0]);
        let empty = table(&[], &[], &[0]);
        // the effect's table: a material texture in slot 3 (an array) whose parameter name differs, and a named sampler
        let et = table(&[var(0x42, 1, 0x203, 0x8000, 0, 0, 0x41, 0x42), var(0x23, 4, 5, 0x4000, 25, 0, 0x51, 0x51)], &[], b"\0DiffuseSampler\0");
        let mut b = vec![0xffu8; 7];
        b.extend(1u32.to_le_bytes());
        b.extend(program("VS_A_00000001_Wrapped", &empty));
        b.extend(2u32.to_le_bytes());
        b.extend(program("PS_A_00000001_Wrapped", &t));
        b.extend(program("PS_B_00000002_Wrapped", &empty));
        for _ in 0..4 {
            b.extend(0u32.to_le_bytes());
        }
        b.extend(2u32.to_le_bytes());
        // two techniques: `draw` (one pass, vertex 1 + pixel 2) and `depth` (two passes)
        let pass = |progs: [u8; 6], name: u16| [progs.to_vec(), vec![0, 0, 0, 0, 0, 0xff, 0, 0], name.to_le_bytes().to_vec(), vec![0u8; 8]].concat();
        let tech = [2u32.to_le_bytes().to_vec(), (1u32 | 0 << 15 | 1 << 24).to_le_bytes().to_vec(), (6u32 | 1 << 15 | 2 << 24).to_le_bytes().to_vec(), 3u32.to_le_bytes().to_vec(), pass([1, 2, 0, 0, 0, 0], 12), pass([1, 0, 0, 0, 0, 0], 12), pass([0, 1, 0, 0, 0, 0], 15)].concat();
        let body = [vec![9u8; 40], tech, 0x8000_0000u32.to_le_bytes().to_vec(), et].concat();
        b.extend(effect("standard", [&[0], &[0, 1], &[], &[], &[], &[]], &body, b"\0draw\0depth\0p0\0p1\0"));
        b.extend(effect("stub", [&[], &[1], &[], &[], &[], &[]], &[0u8; 30], b"\0"));
        let s = read(&b).unwrap();
        assert_eq!((s.programs.len(), s.sections[1], s.effects_stated, s.effects.len()), (3, (1, 2), 2, 2));
        let p = &s.programs[1];
        assert_eq!((p.name.as_str(), p.section, p.id, p.mask, p.data.1), ("PS_A_00000001_Wrapped", 1, 7, 1, 5));
        assert_eq!((p.table.resources[0].resource_kind().as_str(), p.table.resources[0].index, p.table.resources[1].is_cbuffer(), p.table.resources[1].c), ("texture", 237, true, 0x20));
        let m = p.table.members_of(1);
        assert_eq!((m.len(), m[1].name, m[1].a, m[1].member_type().as_str()), (2, 0x32, 0x10, "float4"));
        assert!(p.table.members_of(0).is_empty());
        let e = &s.effects[0];
        assert_eq!((e.name.as_str(), e.programs[1].as_slice(), s.effects[1].name.as_str(), s.effects[1].table.is_none()), ("standard", &[0u32, 1][..], "stub", true));
        let et = e.table.as_ref().unwrap();
        assert_eq!((et.resources[0].material_texture(), et.resources[0].name, et.resources[0].alias, et.resources[1].material_texture()), (Some((3, true)), 0x41, 0x42, None));
        assert_eq!((et.sampler_name(1), et.sampler_name(0)), (Some("DiffuseSampler"), None));
        let t = &e.techniques;
        assert_eq!((t.len(), t[0].name.as_str(), t[0].passes.len(), t[1].name.as_str(), t[1].passes.len()), (2, "draw", 1, "depth", 2));
        assert_eq!((t[0].passes[0].name.as_str(), t[0].passes[0].programs, t[0].passes[0].flags[5]), ("p0", [Some(0), Some(1), None, None, None, None], 0xff));
        assert_eq!((t[1].passes[1].name.as_str(), t[1].passes[1].programs[1]), ("p1", Some(0)));
        assert!(s.effects[1].techniques.is_empty());
    }

    #[test]
    fn cache_stages_join_programs_by_digest() {
        use std::io::Write;
        // a compiled program: a container with a HASH chunk, deflated
        let digest = [7u8; 16];
        let mut dxbc = b"DXBC".to_vec();
        dxbc.extend([0u8; 24]);
        dxbc.extend(1u32.to_le_bytes());
        dxbc.extend(36u32.to_le_bytes());
        dxbc.extend(b"HASH");
        dxbc.extend(20u32.to_le_bytes());
        dxbc.extend(0u32.to_le_bytes());
        dxbc.extend(digest);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&dxbc).unwrap();
        let deflated = enc.finish().unwrap();
        // declared: a texture at index 5 in space 1, one at 300 (kind bit 0x80, index 44) in space 0, a constant buffer 4 in space 2, an unused texture
        let t = Table { resources: vec![
            Var { name: 1, alias: 1, kind: 0x02, dim: 1, index: 5, a: 0x4200, b: 0, c: 0 },
            Var { name: 2, alias: 2, kind: 0x82, dim: 1, index: 44, a: 0x0200, b: 0, c: 0 },
            Var { name: 3, alias: 3, kind: 0x30, dim: 4, index: 4, a: 0x8200, b: 0, c: 0x10 },
            Var { name: 4, alias: 4, kind: 0x02, dim: 1, index: 9, a: 0x0200, b: 0, c: 0 },
        ], members: vec![], names: vec![] };
        let file = [vec![0u8; 3], deflated.clone()].concat();
        let sh = Shaders { programs: vec![
            Program { section: 1, name: "PS_A".into(), kind: 0x32, id: 1, mask: 0, data: (3, deflated.len()), table: t },
            Program { section: 1, name: "PS_B".into(), kind: 0x32, id: 2, mask: 0, data: (0, 2), table: Table::default() },
        ], ..Default::default() };
        assert_eq!(sh.programs[0].digest(&file), Some(digest));
        // the cache: one graphics pipeline whose pixel stage is metadata 1; metadata 0 is not a stage blob
        let rec = |tag: u32, f: &dyn Fn(&mut [u8; 32])| {
            let mut r = [0u8; 32];
            r[..4].copy_from_slice(&tag.to_le_bytes());
            f(&mut r);
            r.to_vec()
        };
        let used = |access: u8, kind: u8, reg: u32, space: u32| rec(0x42, &move |r| {
            r[8] = access;
            r[10] = kind;
            r[12..16].copy_from_slice(&reg.to_le_bytes());
            r[16..20].copy_from_slice(&space.to_le_bytes());
        });
        let stage = [2u32.to_le_bytes().to_vec(), vec![0u8; 12], rec(0x41, &|_| {}), used(1, 2, 5, 1), used(1, 2, 300, 0), used(1, 0x0d, 4, 2), used(1, 2, 77, 0), used(2, 2, 0, 0), rec(0x43, &|r| r[4..20].copy_from_slice(&[7u8; 16]))].concat();
        let other = vec![9u8; 20];
        let mut c = b"PS\0\x09".to_vec();
        c.resize(0x24, 0);
        c.extend(b"930.048.020");
        c.resize(0x50, 0);
        for v in [1u32, 0, 2, 0, (other.len() + stage.len()) as u32, 0] {
            c.extend(v.to_le_bytes());
        }
        c.resize(0x80, 0);
        let mut g = vec![0u8; 0x70];
        g[0x38..].fill(0xff);
        g[0x3c..0x40].copy_from_slice(&1u32.to_le_bytes());
        c.extend(g);
        for (o, n) in [(0, other.len()), (other.len(), stage.len())] {
            c.extend((o as u32).to_le_bytes());
            c.extend((n as u32).to_le_bytes());
        }
        c.extend(other);
        c.extend(stage);
        let cache = read_cache(&c).unwrap();
        assert_eq!((cache.version.as_str(), cache.graphics, cache.stages.len(), cache.stages[&1].pipelines, cache.stages[&1].locations), ("930.048.020", 1, 1, 1, 1));
        let j = sh.compiled(&cache, &file);
        let a = j[0].as_ref().unwrap();
        assert_eq!((a.pipelines, a.read.as_slice(), a.unmatched_reads, a.writes.len()), (1, &[true, true, true, false][..], 1, 1));
        assert!(j[1].is_none());
        assert!(read_cache(&c[..c.len() - 1]).is_err());
    }
}
