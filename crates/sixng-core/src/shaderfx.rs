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

#[derive(Debug, Clone, Serialize)]
pub struct Effect {
    pub name: String,
    /// Program numbers per section (same order as the sections).
    pub programs: [Vec<u32>; 6],
    pub table: Option<Table>,
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
        out.effects.push(Effect { name, programs, table: effect_table(&b[body..end]) });
        cur = next;
    }
    Ok(out)
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
        let body = [vec![9u8; 40], 0x8000_0000u32.to_le_bytes().to_vec(), et].concat();
        b.extend(effect("standard", [&[0], &[0, 1], &[], &[], &[], &[]], &body, b"\0draw\0p0\0"));
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
    }
}
