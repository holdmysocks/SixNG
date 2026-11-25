//! Shader programs and effects of `shaders.rpf` (its uncompressed part):
//! which resources and constants each compiled program declares, and which
//! textures, constants, states, techniques and passes each effect (a
//! material's shader) has.
//!
//! Layout, all little-endian (decoded from the data; counts are from one
//! install). Everything is read in sequence; nothing is searched for but
//! the databases themselves.
//!
//! * The archive holds shader databases (two in the install: 24,804
//!   programs and 910 effects, then 1,699 programs and 22 effects). A
//!   database: magic `SGD5`, `u64 id`, `u32 n` and `n` shared parameter
//!   blocks (`u32 size`, bytes; the `u32` at 16 of a block is the key that
//!   effects refer to it by), a variable table shared by the effects, six
//!   program sections, `u32 count` and the effect records.
//! * The six program sections follow one another, one per stage class:
//!   vertex and mesh, pixel, geometry, two empty ones, compute. A section
//!   is `u32 count` and `count` program records.
//! * A program record: `u8 n`, a name of `n - 1` bytes, NUL, `u8 kind`,
//!   `u32 size`, `u32 0x000ddce7`, `u32 size`, `u32 unpacked size`, then
//!   `size` bytes (a raw deflate stream that inflates to the compiled
//!   program, and a 12-byte trailer: `u64 id`, `u32 mask`), a 140-byte state
//!   block (not decoded) and a variable table. The kind byte takes the
//!   values 50, 60, 64, 65 and 66, which reads as a shader model times ten
//!   (not checked against anything else).
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
//! * An effect record, in order:
//!   - `u8 n`, name, NUL, `u32 loaded size`, six program lists (`u32 n`, n
//!     program numbers within the database's section of the same order);
//!   - 0x50 bytes (not decoded);
//!   - four state tables, each `u32 count` and fixed-size items: rasterizer
//!     (16 bytes), depth-stencil (24), blend (0x44), sampler (0x24). The
//!     items' fields are not decoded;
//!   - `u32 techniques` and one `u32` per technique: bits 0..14 = offset of
//!     its name in the record's name table, bits 15..23 = index of its
//!     first pass, bits 24..31 = number of passes;
//!   - `u32 passes` and 24 bytes per pass: six bytes with the pass's
//!     program per section (1-based into the effect's program list of that
//!     section, 0 = none); bytes 6, 7, 8 = index of its rasterizer,
//!     depth-stencil and blend state; byte 9 = stencil reference; byte 10 =
//!     which of those the pass sets (bit 1 rasterizer, 2 depth-stencil, 4
//!     blend, 8 stencil reference; 0x10 is also seen and not understood);
//!     byte 11 (0, 1 or 255) and bytes 12, 13 (0); `u16` name offset; eight
//!     bytes that are not decoded. Over all 28,798 passes an index is in
//!     range when its bit is set and zero when it is clear. What the states
//!     hold was not compared with the pipeline cache;
//!   - `u32 count` and that many `u32` (not decoded);
//!   - `u32 0x80000000` (in every effect; another value would be the size
//!     of a block stored here);
//!   - the effect's variable table. A material texture is a resource with
//!     flag 0x80: the low byte of its index is its slot in the material's
//!     texture list, and a sampler's `b` is the distance from its record to
//!     its name in the table's name table;
//!   - a second sized block (`u32 size`, bytes; not decoded);
//!   - `u32` whose low 24 bits count the parameter blocks that follow (the
//!     high byte is not decoded). A block is `u32 size` and bytes, or `u32
//!     0x80000000` and the key of a shared block of the database;
//!   - a name table (`u32 size`, strings) with technique and pass names.
//! * The loaded size equals 8 x (the program numbers, plus one per list) +
//!   8 x techniques + 32 x passes + 4 x the `u32` list + the sizes of the
//!   table and the second block, then, when there are parameter blocks,
//!   rounded up to 8 and 8 x blocks + the sizes of the stored ones, + the
//!   size of the name table. This holds for all 932 effects.
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
//! 256 when the kind has bit 0x80. Write records use another numbering that
//! was not worked out.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAGIC: [u8; 4] = [0xe7, 0xdc, 0x0d, 0x00];
const STATE: usize = 140;
const DATABASE: &[u8] = b"SGD5";
const SHARED: u32 = 0x8000_0000;

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
    /// Type of a constant buffer member: see [`member_type`].
    pub fn member_type(&self) -> String {
        member_type(self.kind)
    }
}

/// Type of a constant buffer member from its type code. Floats: 0x09 to
/// 0x0c are float to float4 and 0x0e float4x4 (sizes from the spacing of
/// offsets). Integers: 0x01 is `int` and 0x03 `int3`, checked on real
/// materials (about 20,000 values of code 0x01 are zero or small integers,
/// never a float; code 0x03 takes three components and holds byte
/// patterns); 0x02 and 0x04 are named `int2` and `int4` by that pattern.
/// Others are given as `type_xx`.
pub fn member_type(kind: u8) -> String {
    match kind {
        0x01 => "int".into(),
        0x02 => "int2".into(),
        0x03 => "int3".into(),
        0x04 => "int4".into(),
        0x09 => "float".into(),
        0x0a => "float2".into(),
        0x0b => "float3".into(),
        0x0c => "float4".into(),
        0x0e => "float4x4".into(),
        k => format!("type_{k:02x}"),
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
    /// Bytes 6..14 of the record: three state indices, the stencil
    /// reference, the bits that say which of them the pass sets (see the
    /// methods), one byte that is 0, 1 or 255 and two that are 0.
    pub flags: [u8; 8],
    /// The last eight bytes of the record (not decoded).
    pub tail: [u8; 8],
}

impl Pass {
    fn state(&self, bit: u8, at: usize) -> Option<u8> {
        (self.flags[4] & bit != 0).then_some(self.flags[at])
    }
    /// Index into the effect's rasterizer states, when the pass sets one.
    pub fn rasterizer(&self) -> Option<u8> {
        self.state(1, 0)
    }
    /// Index into the effect's depth-stencil states, when the pass sets one.
    pub fn depth_stencil(&self) -> Option<u8> {
        self.state(2, 1)
    }
    /// Index into the effect's blend states, when the pass sets one.
    pub fn blend(&self) -> Option<u8> {
        self.state(4, 2)
    }
    /// The stencil reference value, when the pass sets one.
    pub fn stencil_ref(&self) -> Option<u8> {
        self.state(8, 3)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Technique {
    pub name: String,
    pub passes: Vec<Pass>,
}

/// The state tables of an effect: raw items (their fields are not decoded).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct States {
    /// 16 bytes each.
    pub rasterizer: Vec<Vec<u8>>,
    /// 24 bytes each.
    pub depth_stencil: Vec<Vec<u8>>,
    /// 0x44 bytes each.
    pub blend: Vec<Vec<u8>>,
    /// 0x24 bytes each.
    pub samplers: Vec<Vec<u8>>,
}

/// A parameter block of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Block {
    /// File offset and length of a block stored in the effect record.
    Inline(usize, usize),
    /// Key of one of the database's shared blocks: see
    /// [`Database::shared_block`].
    Shared(u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct Effect {
    pub name: String,
    /// Index into [`Shaders::databases`].
    pub database: usize,
    /// Program numbers per section (same order as the sections), counted
    /// within the effect's database: see [`Shaders::program`].
    pub programs: [Vec<u32>; 6],
    /// None when the effect's table has no records.
    pub table: Option<Table>,
    /// Empty when the effect has no passes.
    pub techniques: Vec<Technique>,
    /// The `u32` after the name: the record's size once loaded.
    pub loaded_size: u32,
    /// That size worked out from the record's parts (see the module text);
    /// equal to `loaded_size` for every effect of the install it was
    /// checked on.
    pub loaded_size_computed: u32,
    /// The 0x50 bytes before the state tables (not decoded).
    #[serde(skip)]
    pub header: Vec<u8>,
    pub states: States,
    /// The `u32` list after the passes (not decoded).
    #[serde(skip)]
    pub extra: Vec<u32>,
    pub blocks: Vec<Block>,
    /// The high byte of the block count word (not decoded).
    pub block_flags: u8,
}

/// One shader database of the archive.
#[derive(Debug, Clone, Default)]
pub struct Database {
    /// File offset of its magic.
    pub offset: usize,
    /// The `u64` after the magic.
    pub id: u64,
    /// File offset and length of each shared parameter block.
    pub shared_blocks: Vec<(usize, usize)>,
    /// The key of each shared block (its `u32` at 16).
    pub shared_keys: Vec<u32>,
    /// The variable table shared by the database's effects.
    pub table: Option<Table>,
    /// Index of each section's first program in [`Shaders::programs`], and
    /// its count.
    pub sections: [(usize, usize); 6],
    /// Index of the database's first effect in [`Shaders::effects`], and
    /// how many were read.
    pub effects: (usize, usize),
    /// The effect count the database states.
    pub effects_stated: usize,
}

impl Database {
    /// The shared block an effect refers to by key: index into
    /// `shared_blocks`.
    pub fn shared_block(&self, key: u32) -> Option<usize> {
        self.shared_keys.iter().position(|&k| k == key)
    }
}

#[derive(Debug, Default)]
pub struct Shaders {
    /// The programs of every database, in file order.
    pub programs: Vec<Program>,
    pub databases: Vec<Database>,
    /// The effects of every database, in file order.
    pub effects: Vec<Effect>,
}

impl Shaders {
    /// Program number `n` of section `section` of the effect's database:
    /// its index in `programs` and the program.
    pub fn program(&self, e: &Effect, section: usize, n: u32) -> Option<(usize, &Program)> {
        let (first, count) = self.databases.get(e.database)?.sections[section];
        let i = first + n as usize;
        ((n as usize) < count).then(|| self.programs.get(i).map(|p| (i, p))).flatten()
    }
    /// Programs of a section over all databases.
    pub fn section_total(&self, section: usize) -> usize {
        self.databases.iter().map(|d| d.sections[section].1).sum()
    }
    /// The effect count the databases state.
    pub fn effects_stated(&self) -> usize {
        self.databases.iter().map(|d| d.effects_stated).sum()
    }
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

/// Effect header at `p`: name, loaded size, six program lists, offset of
/// the body.
fn effect_header(b: &[u8], p: usize, counts: &[usize; 6]) -> Option<(String, u32, [Vec<u32>; 6], usize)> {
    let (name, mut q) = name_at(b, p)?;
    if name.contains(':') {
        return None;
    }
    let loaded = u32_at(b, q)?;
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
    Some((name.to_string(), loaded, lists, q))
}

/// A read position in the file.
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn u32(&mut self) -> Option<u32> {
        let v = u32_at(self.b, self.p)?;
        self.p += 4;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    /// `u32 count` and `count` items of `size` bytes.
    fn items(&mut self, size: usize) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n.checked_mul(size)?)
    }
    /// `u32 size` and `size` bytes.
    fn sized(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

/// An effect record at `p`: the effect and the offset after it.
fn effect_at(b: &[u8], p: usize, counts: &[usize; 6], database: usize) -> Option<(Effect, usize)> {
    let (name, loaded_size, programs, body) = effect_header(b, p, counts)?;
    let mut c = Cur { b, p: body };
    let header = c.take(0x50)?.to_vec();
    let split = |s: &[u8], n: usize| s.chunks_exact(n).map(<[u8]>::to_vec).collect::<Vec<_>>();
    let states = States { rasterizer: split(c.items(16)?, 16), depth_stencil: split(c.items(24)?, 24), blend: split(c.items(0x44)?, 0x44), samplers: split(c.items(0x24)?, 0x24) };
    let word = |s: &[u8]| s.chunks_exact(4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).collect::<Vec<u32>>();
    let (ents, passes, extra) = (word(c.items(4)?), c.items(24)?, word(c.items(4)?));
    let mut loaded = 8 * programs.iter().map(|l| l.len() + 1).sum::<usize>() + 8 * ents.len() + 32 * (passes.len() / 24) + 4 * extra.len();
    // a block that every effect of the install leaves to the database (`0x80000000`)
    let first = c.u32()?;
    if first != SHARED {
        loaded += c.take(first as usize)?.len();
    }
    let at = c.p;
    loaded += c.sized()?.len();
    let table = table_at(b, at).filter(|(t, end)| *end == c.p && !(t.resources.is_empty() && t.members.is_empty())).map(|x| x.0);
    loaded += c.sized()?.len();
    let count = c.u32()?;
    let n = (count & 0xff_ffff) as usize;
    if n > 0 {
        loaded = loaded.next_multiple_of(8) + 8 * n;
    }
    let mut blocks = Vec::new();
    for _ in 0..n {
        let size = c.u32()?;
        if size == SHARED {
            blocks.push(Block::Shared(c.u32()?));
        } else {
            blocks.push(Block::Inline(c.p, c.take(size as usize)?.len()));
            loaded += size as usize;
        }
    }
    let names = c.sized()?;
    loaded += names.len();
    let name_of = |o: usize| -> Option<String> {
        let s = names.get(o..)?;
        Some(String::from_utf8_lossy(&s[..s.iter().position(|&x| x == 0)?]).into_owned())
    };
    // a technique: bits 0..14 name offset, 15..23 index of its first pass (it wraps: the passes simply follow one another), 24..31 pass count
    let mut it = passes.chunks_exact(24);
    let techniques = ents
        .iter()
        .map(|&e| {
            let passes = it
                .by_ref()
                .take((e >> 24) as usize)
                .map(|x| Some(Pass { name: name_of(u16::from_le_bytes([x[14], x[15]]) as usize)?, programs: std::array::from_fn(|j| x[j].checked_sub(1)), flags: x[6..14].try_into().unwrap(), tail: x[16..24].try_into().unwrap() }))
                .collect::<Option<Vec<_>>>()?;
            Some(Technique { name: name_of((e & 0x7fff) as usize)?, passes })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((Effect { name, database, programs, table, techniques, loaded_size, loaded_size_computed: loaded as u32, header, states, extra, blocks, block_flags: (count >> 24) as u8 }, c.p))
}

/// A database at `p` (its magic): reads its programs and effects into
/// `out` and gives the offset after it. None when `p` is not a database; an
/// error when one starts there and a program record does not parse.
fn database_at(b: &[u8], p: usize, out: &mut Shaders) -> Option<Result<usize>> {
    let mut c = Cur { b, p };
    if c.take(4)? != DATABASE {
        return None;
    }
    let mut db = Database { offset: p, id: u64::from_le_bytes(c.take(8)?.try_into().ok()?), ..Default::default() };
    for _ in 0..c.u32()? {
        let at = c.p + 4;
        let s = c.sized()?;
        db.shared_blocks.push((at, s.len()));
        db.shared_keys.push(u32_at(s, 16)?);
    }
    let at = c.p;
    c.sized()?;
    db.table = table_at(b, at).filter(|x| x.1 == c.p).map(|x| x.0);
    // it is a database when its first section count and, if there is one, its first program parse
    let n0 = u32_at(b, c.p)? as usize;
    if n0 > 0 {
        program_at(b, c.p + 4, 0)?;
    }
    let mut p = c.p;
    for s in 0..6 {
        let Some(n) = u32_at(b, p).map(|n| n as usize) else { return Some(Err(anyhow!("database at {:#x}: section {s} is cut short", db.offset))) };
        p += 4;
        db.sections[s] = (out.programs.len(), n);
        for i in 0..n {
            let Some((prog, next)) = program_at(b, p, s as u8) else { return Some(Err(anyhow!("database at {:#x}: section {s} program {i} at {p:#x} does not parse", db.offset))) };
            out.programs.push(prog);
            p = next;
        }
    }
    let counts = db.sections.map(|s| s.1);
    let Some(stated) = u32_at(b, p) else { return Some(Err(anyhow!("database at {:#x}: no effect count", db.offset))) };
    p += 4;
    db.effects_stated = stated as usize;
    db.effects.0 = out.effects.len();
    // an effect that does not parse ends the list: `effects.1` then stays below `effects_stated`
    for _ in 0..db.effects_stated {
        let Some((e, next)) = effect_at(b, p, &counts, out.databases.len()) else { break };
        out.effects.push(e);
        p = next;
    }
    db.effects.1 = out.effects.len() - db.effects.0;
    out.databases.push(db);
    Some(Ok(p))
}

/// Reads every shader database from the bytes of `shaders.rpf`.
pub fn read(b: &[u8]) -> Result<Shaders> {
    let mut out = Shaders::default();
    let mut p = 0;
    while let Some(at) = b.get(p..).and_then(|s| s.windows(4).position(|w| w == DATABASE)).map(|i| p + i) {
        p = match database_at(b, at, &mut out) {
            Some(end) => end?.max(at + 4),
            None => at + 4,
        };
    }
    if out.databases.is_empty() {
        return Err(anyhow!("no shader database found"));
    }
    Ok(out)
}

/// The material constants of every effect, for readers of a model's
/// materials: effect (hex joaat of its lower-case name, a drawable's
/// shader name hash) -> parameter hash (hex) -> (type code of the constant
/// buffer member, hash of the shader variable's name).
/// `<work>/index/shader_params.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ParamIndex {
    pub version: u32,
    pub effects: BTreeMap<String, BTreeMap<String, (u8, String)>>,
}

const PARAM_INDEX_VERSION: u32 = 2;

impl ParamIndex {
    pub fn build(sh: &Shaders) -> ParamIndex {
        let mut effects = BTreeMap::new();
        for e in &sh.effects {
            let Some(t) = &e.table else { continue };
            let mut m = BTreeMap::new();
            for (i, _) in t.resources.iter().enumerate().filter(|(_, r)| r.is_cbuffer() && r.flags() & 0x80 != 0) {
                for v in t.members_of(i) {
                    // a material names the parameter (the alias); the variable's own name is kept beside it
                    m.insert(format!("{:08x}", v.alias), (v.kind, format!("{:08x}", v.name)));
                    m.entry(format!("{:08x}", v.name)).or_insert((v.kind, format!("{:08x}", v.name)));
                }
            }
            if !m.is_empty() {
                effects.insert(format!("{:08x}", crate::meta7a::joaat(&e.name.to_ascii_lowercase())), m);
            }
        }
        ParamIndex { version: PARAM_INDEX_VERSION, effects }
    }

    /// The type code of a parameter of an effect.
    pub fn kind(&self, effect: &str, param: &str) -> Option<u8> {
        self.effects.get(effect)?.get(param).map(|x| x.0)
    }

    /// The index, read from the work directory or built from `shaders.rpf`
    /// (about 2 s) and written there; rebuilt when the archive is newer.
    pub fn get(game_root: &std::path::Path) -> Result<&'static ParamIndex, String> {
        static T: std::sync::OnceLock<Result<ParamIndex, String>> = std::sync::OnceLock::new();
        T.get_or_init(|| {
            let work = crate::names::work_dir().ok_or("no work directory")?;
            let (cache, archive) = (work.join("index").join("shader_params.json"), game_root.join("shaders.rpf"));
            let stamp = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if let Some(t) = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice::<ParamIndex>(&b).ok()) {
                if t.version == PARAM_INDEX_VERSION && stamp(&cache) >= stamp(&archive) {
                    return Ok(t);
                }
            }
            let bytes = std::fs::read(&archive).map_err(|e| format!("{}: {e}", archive.display()))?;
            let t = ParamIndex::build(&read(&bytes).map_err(|e| format!("shaders.rpf: {e:#}"))?);
            if t.effects.is_empty() {
                return Err("no effect tables found in shaders.rpf".into());
            }
            if let Ok(b) = serde_json::to_vec(&t) {
                let _ = std::fs::create_dir_all(work.join("index"));
                let _ = std::fs::write(&cache, b);
            }
            Ok(t)
        })
        .as_ref()
        .map_err(|e| e.clone())
    }
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
    /// What follows an effect's program lists.
    struct Body<'a> {
        states: [(usize, usize); 4],
        techniques: &'a [u32],
        passes: &'a [Vec<u8>],
        extra: &'a [u32],
        table: &'a [u8],
        second: &'a [u8],
        blocks: &'a [Result<&'a [u8], u32>],
        names: &'a [u8],
    }
    fn effect(name: &str, loaded: u32, lists: [&[u32]; 6], x: &Body) -> Vec<u8> {
        let mut v = vec![name.len() as u8 + 1];
        v.extend(name.bytes());
        v.push(0);
        v.extend(loaded.to_le_bytes());
        for l in lists {
            v.extend((l.len() as u32).to_le_bytes());
            v.extend(l.iter().flat_map(|x| x.to_le_bytes()));
        }
        v.extend([9u8; 0x50]);
        for (k, (n, size)) in x.states.iter().enumerate() {
            v.extend((*n as u32).to_le_bytes());
            v.extend((0..n * size).map(|i| (k * 16 + i / size) as u8));
        }
        v.extend((x.techniques.len() as u32).to_le_bytes());
        v.extend(x.techniques.iter().flat_map(|t| t.to_le_bytes()));
        v.extend((x.passes.len() as u32).to_le_bytes());
        v.extend(x.passes.iter().flatten());
        v.extend((x.extra.len() as u32).to_le_bytes());
        v.extend(x.extra.iter().flat_map(|t| t.to_le_bytes()));
        v.extend(SHARED.to_le_bytes());
        v.extend(x.table);
        v.extend((x.second.len() as u32).to_le_bytes());
        v.extend(x.second);
        v.extend((x.blocks.len() as u32 | 4 << 24).to_le_bytes());
        for b in x.blocks {
            match b {
                Ok(bytes) => {
                    v.extend((bytes.len() as u32).to_le_bytes());
                    v.extend(*bytes);
                }
                Err(key) => {
                    v.extend(SHARED.to_le_bytes());
                    v.extend(key.to_le_bytes());
                }
            }
        }
        v.extend((x.names.len() as u32).to_le_bytes());
        v.extend(x.names);
        v
    }
    fn database(id: u64, shared: &[u32], sections: [&[Vec<u8>]; 6], effects: &[Vec<u8>]) -> Vec<u8> {
        let mut v = DATABASE.to_vec();
        v.extend(id.to_le_bytes());
        v.extend((shared.len() as u32).to_le_bytes());
        for key in shared {
            v.extend(24u32.to_le_bytes());
            v.extend([0u8; 16]);
            v.extend(key.to_le_bytes());
            v.extend([0u8; 4]);
        }
        v.extend(table(&[var(0x23, 4, 1, 0, 0, 0, 0x91, 0x91)], &[], &[0]));
        for s in sections {
            v.extend((s.len() as u32).to_le_bytes());
            v.extend(s.iter().flatten());
        }
        v.extend((effects.len() as u32).to_le_bytes());
        v.extend(effects.iter().flatten());
        v
    }

    #[test]
    fn databases_programs_effects_and_their_tables() {
        // a pixel program: one texture, one constant buffer of 0x20 bytes with two members (its record is 1 before the first member)
        let t = table(&[var(0x02, 1, 237, 0x200, 0, 0, 0x11, 0x11), var(0x30, 4, 4, 2 | 0x82 << 8 | 24 << 16, 0, 0x20, 0x22, 0x22)], &[var(0x09, 0, 0, 0, 0, 0, 0x31, 0x31), var(0x0c, 0, 0, 0x10, 0, 0, 0x32, 0x32)], &[0]);
        let empty = table(&[], &[], &[0]);
        // the effect's table: a material texture in slot 3 (an array) whose parameter name differs, and a named sampler
        let et = table(&[var(0x42, 1, 0x203, 0x8000, 0, 0, 0x41, 0x42), var(0x23, 4, 5, 0x4000, 25, 0, 0x51, 0x51)], &[], b"\0DiffuseSampler\0");
        // a pass: six program bytes, three state indices, stencil reference, the bits that say which are set, three more bytes, name offset, eight bytes
        let pass = |progs: [u8; 6], state: [u8; 5], name: u16| [progs.to_vec(), state.to_vec(), vec![0xff, 0, 0], name.to_le_bytes().to_vec(), vec![3u8; 8]].concat();
        // two techniques: `draw` (one pass, vertex 1 + pixel 2) and `depth` (two passes)
        let standard = effect(
            "standard",
            306,
            [&[0], &[0, 1], &[], &[], &[], &[]],
            &Body {
                states: [(2, 16), (1, 24), (3, 0x44), (1, 0x24)],
                techniques: &[1 | 0 << 15 | 1 << 24, 6 | 1 << 15 | 2 << 24],
                passes: &[pass([1, 2, 0, 0, 0, 0], [1, 0, 2, 7, 1 | 4 | 8], 12), pass([1, 0, 0, 0, 0, 0], [0, 0, 0, 0, 2], 12), pass([0, 1, 0, 0, 0, 0], [0; 5], 15)],
                extra: &[5],
                table: &et,
                second: &[1, 2, 3],
                blocks: &[Err(0xabcd), Ok(&[7u8; 8][..])],
                names: b"\0draw\0depth\0p0\0p1\0",
            },
        );
        let none = Body { states: [(0, 16), (0, 24), (0, 0x44), (0, 0x24)], techniques: &[], passes: &[], extra: &[], table: &empty, second: &[], blocks: &[], names: b"\0" };
        let stub = effect("stub", 0, [&[], &[1], &[], &[], &[], &[]], &none);
        let mut b = vec![0xffu8; 7];
        b.extend(database(0x1122, &[0x99, 0xabcd], [&[program("VS_A_00000001_Wrapped", &empty)], &[program("PS_A_00000001_Wrapped", &t), program("PS_B_00000002_Wrapped", &empty)], &[], &[], &[], &[]], &[standard, stub]));
        // the magic alone is not a database
        b.extend(DATABASE);
        b.extend([0xffu8; 40]);
        // a second database: its effects count programs from its own sections
        let one = Body { techniques: &[1 | 1 << 24], passes: &[pass([0, 1, 0, 0, 0, 0], [0; 5], 6)], names: b"\0draw\0p0\0", ..none };
        b.extend(database(0x3344, &[], [&[], &[program("PS_C_00000003_Wrapped", &empty)], &[], &[], &[], &[]], &[effect("im", 0, [&[], &[0], &[], &[], &[], &[]], &one)]));
        let s = read(&b).unwrap();
        assert_eq!((s.programs.len(), s.databases.len(), s.effects_stated(), s.effects.len(), s.section_total(1)), (4, 2, 3, 3, 3));
        let d = &s.databases;
        assert_eq!((d[0].offset, d[0].id, d[0].sections[1], d[0].effects, d[1].id, d[1].sections[1], d[1].effects), (7, 0x1122, (1, 2), (0, 2), 0x3344, (3, 1), (2, 1)));
        assert_eq!((d[0].shared_block(0xabcd), d[0].shared_block(1), d[0].shared_blocks[1].1, d[0].table.as_ref().unwrap().resources[0].name), (Some(1), None, 24, 0x91));
        let p = &s.programs[1];
        assert_eq!((p.name.as_str(), p.section, p.id, p.mask, p.data.1), ("PS_A_00000001_Wrapped", 1, 7, 1, 5));
        assert_eq!((p.table.resources[0].resource_kind().as_str(), p.table.resources[0].index, p.table.resources[1].is_cbuffer(), p.table.resources[1].c), ("texture", 237, true, 0x20));
        let m = p.table.members_of(1);
        assert_eq!((m.len(), m[1].name, m[1].a, m[1].member_type().as_str()), (2, 0x32, 0x10, "float4"));
        assert_eq!((member_type(0x01).as_str(), member_type(0x03).as_str(), member_type(0x05).as_str()), ("int", "int3", "type_05"));
        // the parameter index: an effect's material constants by parameter (alias) and by variable name
        let mut fx = s.effects[0].clone();
        let mut t = fx.table.take().unwrap();
        t.resources.push(Var { name: 0x61, alias: 0x61, kind: 0x30, dim: 4, index: 4, a: 2 | 0x80 << 8 | 24 << 16, b: 0, c: 0x20 });
        t.members = vec![Var { name: 0x71, alias: 0x72, kind: 0x01, dim: 0, index: 0, a: 0, b: 0, c: 0 }, Var { name: 0x73, alias: 0x73, kind: 0x0c, dim: 0, index: 0, a: 0x10, b: 0, c: 0 }];
        fx.table = Some(t);
        let ix = ParamIndex::build(&Shaders { effects: vec![fx], ..Default::default() });
        let e = format!("{:08x}", crate::meta7a::joaat("standard"));
        assert_eq!((ix.kind(&e, "00000072"), ix.kind(&e, "00000071"), ix.kind(&e, "00000073"), ix.kind(&e, "00000099"), ix.kind("00000000", "00000072")), (Some(1), Some(1), Some(0x0c), None, None));
        assert!(p.table.members_of(0).is_empty());
        let e = &s.effects[0];
        assert_eq!((e.name.as_str(), e.database, e.programs[1].as_slice(), s.effects[1].name.as_str(), s.effects[1].table.is_none()), ("standard", 0, &[0u32, 1][..], "stub", true));
        assert_eq!((e.loaded_size, e.loaded_size_computed, e.header.len(), e.extra.as_slice(), e.block_flags), (306, 306, 0x50, &[5u32][..], 4));
        assert_eq!((e.states.rasterizer.len(), e.states.depth_stencil[0].len(), e.states.blend[2][0], e.states.samplers.len()), (2, 24, 34, 1));
        assert_eq!((e.blocks[0], matches!(e.blocks[1], Block::Inline(at, 8) if b[at..at + 8] == [7u8; 8])), (Block::Shared(0xabcd), true));
        let et = e.table.as_ref().unwrap();
        assert_eq!((et.resources[0].material_texture(), et.resources[0].name, et.resources[0].alias, et.resources[1].material_texture()), (Some((3, true)), 0x41, 0x42, None));
        assert_eq!((et.sampler_name(1), et.sampler_name(0)), (Some("DiffuseSampler"), None));
        let t = &e.techniques;
        assert_eq!((t.len(), t[0].name.as_str(), t[0].passes.len(), t[1].name.as_str(), t[1].passes.len()), (2, "draw", 1, "depth", 2));
        let ps = &t[0].passes[0];
        assert_eq!((ps.name.as_str(), ps.programs, ps.flags[5], ps.tail), ("p0", [Some(0), Some(1), None, None, None, None], 0xff, [3; 8]));
        assert_eq!((ps.rasterizer(), ps.depth_stencil(), ps.blend(), ps.stencil_ref()), (Some(1), None, Some(2), Some(7)));
        assert_eq!((t[1].passes[0].rasterizer(), t[1].passes[0].depth_stencil(), t[1].passes[1].name.as_str(), t[1].passes[1].programs[1]), (None, Some(0), "p1", Some(0)));
        assert!(s.effects[1].techniques.is_empty());
        // a program number is counted within the effect's own database
        let im = &s.effects[2];
        assert_eq!((im.database, s.program(im, 1, 0).map(|x| (x.0, x.1.name.as_str())), s.program(im, 1, 1).is_none(), s.program(e, 1, 1).map(|x| x.1.name.as_str())), (1, Some((3, "PS_C_00000003_Wrapped")), true, Some("PS_B_00000002_Wrapped")));
        assert_eq!(im.techniques[0].passes[0].name, "p0");
        // a record that is cut short ends its database's effects, and an archive without a database is an error
        let cut = read(&b[..b.len() - 3]).unwrap();
        assert_eq!((cut.effects.len(), cut.databases[1].effects, cut.databases[1].effects_stated), (2, (2, 0), 1));
        assert!(read(&[0u8; 64]).is_err());
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
