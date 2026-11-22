//! RAGE resource access (system page 0x5xxxxxxx, graphics pages 0x6xxxxxxx).
use crate::scan::{read_stream, Entry};
use anyhow::{bail, Result};
use std::fs::File;
use std::path::Path;

pub struct Resource {
    pub sys: Vec<u8>,
    pub gfx: Vec<u8>,
    /// When > 0 (see `load_meta`), the graphics pages are also appended to
    /// `sys` at this offset, so `0x6xxxxxxx` pointers resolve into `sys`.
    pub gfx_base: usize,
}

pub const SYS_BASE: u64 = 0x5000_0000;
pub const GFX_BASE: u64 = 0x6000_0000;

impl Resource {
    pub fn load(archive: &Path, e: &Entry) -> Result<Self> {
        Self::load_parts(archive, e, true)
    }

    /// Load a meta (0x7a) resource: system page plus graphics pages appended
    /// to it, because some instance blocks (e.g. batched prop instances)
    /// live in the graphics pages.
    pub fn load_meta(archive: &Path, e: &Entry) -> Result<Self> {
        let mut r = Self::load_parts(archive, e, true)?;
        // Small graphics pages are stored uncompressed right after the last
        // zlib stream (16-byte aligned), so the stream index does not see
        // them. Read as much as the meta block table needs from there.
        if r.gfx.is_empty() && r.sys.get(0x10..0x15) == Some(b"0DRPz") {
            let need = r.meta_gfx_extent();
            if need > 0 && need <= 64 << 20 {
                if let Some(last) = e.streams.last() {
                    use std::io::{Read, Seek, SeekFrom};
                    let pos = (last[0] + last[1] + 15) & !15;
                    let mut f = File::open(archive)?;
                    f.seek(SeekFrom::Start(pos))?;
                    let mut buf = vec![0u8; need];
                    let n = f.read(&mut buf)?;
                    buf.truncate(n);
                    r.gfx = buf;
                }
            }
        }
        if !r.gfx.is_empty() {
            let base = (r.sys.len() + 15) & !15;
            r.sys.resize(base, 0);
            r.sys.extend_from_slice(&r.gfx);
            r.gfx_base = base;
            r.gfx = Vec::new();
        }
        Ok(r)
    }

    /// Bytes of graphics page that a meta resource's instance blocks reach.
    fn meta_gfx_extent(&self) -> usize {
        let Some(it) = self.sys_off(self.u64(0x38)) else { return 0 };
        (0..self.u16(0x4C) as usize)
            .filter_map(|i| {
                let p = self.u64(it + 16 * i + 8);
                (p >> 28 == 6).then(|| (p & 0x0FFF_FFFF) as usize + self.u32(it + 16 * i + 4) as usize)
            })
            .max()
            .unwrap_or(0)
    }

    /// Resolve a sys pointer, or a graphics pointer when `gfx_base` is set.
    pub fn meta_off(&self, ptr: u64) -> Option<usize> {
        self.sys_off(ptr).or_else(|| {
            let o = self.gfx_base + (ptr & 0x0FFF_FFFF) as usize;
            (ptr >> 28 == 6 && self.gfx_base > 0 && o < self.sys.len()).then_some(o)
        })
    }

    /// Load only the system page (cheap; graphics pages can be tens of MB).
    pub fn load_sys(archive: &Path, e: &Entry) -> Result<Self> {
        Self::load_parts(archive, e, false)
    }

    fn load_parts(archive: &Path, e: &Entry, with_gfx: bool) -> Result<Self> {
        let mut f = File::open(archive)?;
        if e.streams.is_empty() {
            bail!("entry has no streams");
        }
        let (sys, n) = read_sys(&mut f, e)?;
        let mut gfx = Vec::new();
        if with_gfx {
            for st in &e.streams[n..] {
                gfx.extend(read_stream(&mut f, st)?);
            }
        }
        Ok(Resource { sys, gfx, gfx_base: 0 })
    }

    pub fn u16(&self, o: usize) -> u16 {
        self.sys.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0)
    }
    pub fn u32(&self, o: usize) -> u32 {
        self.sys.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
    }
    pub fn u64(&self, o: usize) -> u64 {
        self.sys.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
    }
    pub fn f32(&self, o: usize) -> f32 {
        f32::from_bits(self.u32(o))
    }
    /// Resolve a system pointer to an offset into `sys`.
    pub fn sys_off(&self, ptr: u64) -> Option<usize> {
        if ptr >> 28 == 5 {
            let o = (ptr & 0x0FFF_FFFF) as usize;
            if o < self.sys.len() {
                return Some(o);
            }
        }
        None
    }
    /// Resolve a graphics pointer to an offset into `gfx`.
    pub fn gfx_off(&self, ptr: u64) -> Option<usize> {
        if ptr >> 28 == 6 {
            let o = (ptr & 0x0FFF_FFFF) as usize;
            if o <= self.gfx.len() {
                return Some(o);
            }
        }
        None
    }
    pub fn cstr(&self, o: usize) -> Option<String> {
        let b = self.sys.get(o..)?;
        let end = b.iter().position(|&c| c == 0)?;
        let s = &b[..end];
        if s.is_empty() || s.len() > 256 || !s.iter().all(|&c| (0x20..0x7f).contains(&c)) {
            return None;
        }
        Some(String::from_utf8_lossy(s).to_string())
    }
}

/// Find "<name>.#<tag>" in a system page; returns (name, tag).
pub fn find_resource_name(sys: &[u8]) -> Option<(String, String)> {
    let mut i = 0;
    while let Some(p) = memfind(&sys[i..], b".#") {
        let at = i + p;
        let tag = sys.get(at + 2..at + 4)?;
        if tag.iter().all(|c| c.is_ascii_lowercase()) {
            let mut s = at;
            while s > 0 && is_name_char(sys[s - 1]) {
                s -= 1;
            }
            if at - s >= 2 {
                return Some((
                    String::from_utf8_lossy(&sys[s..at]).to_string(),
                    String::from_utf8_lossy(tag).to_string(),
                ));
            }
        }
        i = at + 2;
    }
    None
}

/// Resource names are lowercase. Stopping at anything else (including
/// uppercase) avoids picking up stray bytes in front of the string.
pub(crate) fn is_name_char(c: u8) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
}

pub fn memfind(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    h.windows(n.len()).position(|w| w == n)
}

/// Read the system page of a resource entry. Usually that is stream 0, but
/// large resources split it over several streams: keep appending streams
/// until the page-map pointer (u64 at +0x08) falls inside what we have.
/// Returns the bytes and how many streams were used.
pub fn read_sys(f: &mut File, e: &Entry) -> Result<(Vec<u8>, usize)> {
    let mut sys = read_stream(f, &e.streams[0])?;
    let mut n = 1;
    let pm = sys.get(8..16).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    if pm >> 28 == 5 {
        let need = (pm & 0x0FFF_FFFF) as usize + 8;
        while sys.len() < need && n < e.streams.len() {
            sys.extend(read_stream(f, &e.streams[n])?);
            n += 1;
        }
    }
    // The page-map pointer is not enough: some resources have a small second
    // sys stream (e.g. 0x50000 + 1168 bytes) that only other sys pointers
    // reach. Keep extending while aligned sys pointers target the next
    // stream; otherwise every gfx offset is shifted by that stream's size
    // and no geometry is found.
    // Research toggle: SIXNG_SYS_ANY_ALIGN=1 restores the pre-2026-11-20 rule.
    let any_align = std::env::var_os("SIXNG_SYS_ANY_ALIGN").is_some();
    while n < e.streams.len() {
        let len = sys.len();
        let next = e.streams[n][2] as usize;
        // Count 8-aligned targets only: an unaligned value repeated three
        // times (0x50007646 in prop_am_bikerack_02b) once swallowed a whole
        // gfx stream into sys, leaving no geometry. Repeats of a real
        // pointer still count (requiring distinct targets lost ~1.5 K
        // archetype definitions from meta resources).
        let hits = sys
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .filter(|&p| p >> 28 == 5 && (p & 3 == 0 || any_align) && ((p & 0x0FFF_FFFF) as usize) >= len && ((p & 0x0FFF_FFFF) as usize) < len + next)
            .count();
        // Stray pointer-like values (hashes, packed data) can hit a large gfx
        // stream; real sys continuations are referenced densely.
        if hits < 2.max(next / 65536) {
            break;
        }
        sys.extend(read_stream(f, &e.streams[n])?);
        n += 1;
    }
    Ok((sys, n))
}
