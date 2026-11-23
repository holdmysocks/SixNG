//! Animation clips from `anim.rpf` (`crClipDictionary` resources).
//!
//! Contains ported MIT-licensed code: see THIRD_PARTY_NOTICES.md.
//!
//! Layout (system page pointers, `0x5xxxxxxx`):
//! - Clip dictionary: +0x38 name, +0x48 / +0x50 `atMap` of clips (buckets of
//!   24-byte nodes `{u32 hash, u32, ptr value, ptr next}`), +0x70 / +0x78
//!   animation dictionaries (each +0x38 name, +0x48 / +0x50 `atMap` of
//!   animations).
//! - Clip (0x88 bytes): +0x28 name (`pack:/<name>.clip`), +0x68 pointer to
//!   `{ptr bindings, u16 count}`, +0x78 duration. A binding (0x38 bytes)
//!   points at an animation and plays its window [start (+0x28), end
//!   (+0x2C)] at `rate` (+0x30): clip time = (animation time − start) / rate.
//!   A clip usually binds three animations sampled at 30, 15 and 10 Hz.
//! - Animation: +0x24 frame count, +0x28 duration, +0x50 / +0x58 blocks,
//!   +0x60 / +0x68 tracks of 12 bytes `{u32 bone id, u32 track type, u32
//!   value type}`. The bone id is joaat(lowercase bone name), 0 = mover;
//!   the track type is joaat of its name (`bonerotation`, `bonetranslation`
//!   …); value type 0 = quaternion, 2 = vector3, 4 = float. Track k owns float
//!   slots 4k..4k+3.
//! - Block (`crAnimSequence`): 0x20 header, channel descriptors per class,
//!   a per-frame bit stream, then 11 u16 class counts and the slot index
//!   list of each class. Classes: 0 static quaternion (xyz, w rebuilt), 1
//!   static vector3, 2 static float, 4 quantised per frame, 5 quantised
//!   through a lookup table, 7 rebuild a dropped quaternion component, 8
//!   normalise a quaternion, 10 a 64-frame second-order delta curve.
//!   Consecutive blocks share one boundary frame.
use anyhow::{anyhow, bail, Result};

fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(b.get(o..o + 2).ok_or_else(|| anyhow!("read past end at 0x{o:x}"))?.try_into().unwrap()))
}
fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(b.get(o..o + 4).ok_or_else(|| anyhow!("read past end at 0x{o:x}"))?.try_into().unwrap()))
}
fn f32_at(b: &[u8], o: usize) -> Result<f32> {
    u32_at(b, o).map(f32::from_bits)
}
/// System page pointer at `o` -> offset into `b`.
fn ptr(b: &[u8], o: usize) -> Option<usize> {
    let v = u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?);
    let off = (v & 0x0fff_ffff) as usize;
    (v >> 28 == 5 && off < b.len()).then_some(off)
}
/// A 16-byte string field: a pointer to the text (u64, then u16 length and
/// capacity), or, for names up to 15 bytes, the text itself with byte 15 =
/// 0x80 | length (map clip dictionaries: `td_clip`, 0x87).
fn str_field(b: &[u8], o: usize) -> Option<String> {
    let tag = *b.get(o + 15)?;
    if tag & 0x80 != 0 {
        let n = (tag & 0x7f) as usize;
        let t = b.get(o..o + n.min(15))?;
        return (n <= 15 && t.iter().all(|c| c.is_ascii_graphic() || *c == b' ')).then(|| String::from_utf8_lossy(t).into_owned());
    }
    ptr(b, o).and_then(|p| cstr(b, p))
}

fn cstr(b: &[u8], o: usize) -> Option<String> {
    let s = b.get(o..)?;
    let end = s.iter().position(|&c| c == 0)?;
    Some(String::from_utf8_lossy(&s[..end]).into_owned())
}
/// `n` (0..=32) bits LSB-first at bit offset `bit`.
fn bits(b: &[u8], bit: usize, n: usize) -> u32 {
    if n == 0 {
        return 0;
    }
    let (byte, shift) = (bit >> 3, bit & 7);
    let mut acc = 0u64;
    for i in 0..8 {
        if let Some(&x) = b.get(byte + i) {
            acc |= (x as u64) << (8 * i);
        }
    }
    ((acc >> shift) & if n >= 32 { u32::MAX as u64 } else { (1u64 << n) - 1 }) as u32
}

/// Value type of a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Quaternion,
    Vector3,
    Float,
}

#[derive(Clone, Debug)]
pub struct Track {
    /// joaat(lowercase bone name); 0 = the mover (root motion).
    pub bone: u32,
    /// joaat of the track type name (`track::ROTATION` …).
    pub kind: u32,
    pub value: ValueType,
    /// Per-frame values: xyzw (quaternion), xyz_ (vector3) or x___ (float).
    pub frames: Vec<[f32; 4]>,
}

/// Track type hashes (joaat of the lowercase name).
pub mod track {
    /// `bonerotation`
    pub const ROTATION: u32 = 0x98e1750c;
    /// `bonetranslation`
    pub const TRANSLATION: u32 = 0xf791a95f;
    /// joaat("bonescale").
    pub fn scale() -> u32 {
        crate::meta7a::joaat("bonescale")
    }
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub hash: u32,
    pub name: String,
    pub frames: usize,
    pub duration: f32,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug)]
pub struct Binding {
    /// Index into `ClipDictionary::animations`.
    pub animation: usize,
    pub start: f32,
    pub end: f32,
    pub rate: f32,
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub hash: u32,
    /// Clip type (+0x10): 1 = animations (`crClipAnimations`), the only one
    /// decoded; other types have no name or bindings here.
    pub kind: u32,
    /// As stored (`pack:/<name>.clip`).
    pub name: String,
    pub duration: f32,
    pub bindings: Vec<Binding>,
}

impl Clip {
    /// Name without the `pack:/` prefix and `.clip` suffix.
    pub fn short_name(&self) -> &str {
        let n = self.name.rsplit(['/', ':']).next().unwrap_or(&self.name);
        n.strip_suffix(".clip").unwrap_or(n)
    }
}

#[derive(Clone, Debug)]
pub struct ClipDictionary {
    pub name: String,
    pub clips: Vec<Clip>,
    pub animations: Vec<Animation>,
}

/// Does this system page look like a clip dictionary? (pointers at +0x08,
/// +0x38, +0x48, zeros at +0x10..+0x38, a name at +0x38.)
pub fn is_clip_dictionary(b: &[u8]) -> bool {
    b.len() > 0x88 && b[0x10..0x38].iter().all(|&x| x == 0) && ptr(b, 0x08).is_some() && ptr(b, 0x48).is_some() && str_field(b, 0x38).is_some_and(|s| !s.is_empty() && s.len() < 256)
}

/// Name of a clip dictionary (+0x38), without decoding it.
pub fn dictionary_name(b: &[u8]) -> Option<String> {
    is_clip_dictionary(b).then(|| str_field(b, 0x38)).flatten()
}

/// (key hash, value offset) of every node of an `atMap`.
fn walk_map(b: &[u8], buckets: usize, count: usize) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    for i in 0..count {
        let mut node = ptr(b, buckets + 8 * i);
        let mut guard = 0;
        while let Some(n) = node {
            if n + 24 > b.len() || guard > 4096 {
                break;
            }
            if let (Ok(h), Some(v)) = (u32_at(b, n), ptr(b, n + 8)) {
                out.push((h, v));
            }
            node = ptr(b, n + 16);
            guard += 1;
        }
    }
    out
}

/// Class 10 channel: per 64-frame superblock an anchor value, then
/// zigzag second differences stored in 8 interleaved lanes with unary
/// group bit widths. Checks that each superblock lands on the next anchor.
fn class10(frames: usize, raw: &[u8]) -> Result<Vec<f32>> {
    let dw = u32_at(raw, 0)? as usize;
    let (b4, b5, b6) = (raw[4] as usize, raw[5] as usize, raw[6] as usize);
    let (q, off) = (f32_at(raw, 8)?, f32_at(raw, 12)?);
    if dw < 4 || dw * 4 > raw.len() || 4 + 8 * b6 > dw {
        bail!("class 10 sizes out of range");
    }
    let npref = dw - 4 - 8 * b6;
    let pref = &raw[16..16 + 4 * npref];
    let lane_base = 16 + 4 * npref;
    let lanes: Vec<Vec<u8>> = (0..8).map(|l| (0..b6).flat_map(|s| raw[lane_base + 4 * (8 * s + l)..][..4].to_vec()).collect()).collect();
    let nsuper = frames.div_ceil(64);
    let offsets: Vec<usize> = (0..nsuper).map(|i| bits(pref, i * b4, b4) as usize).collect();
    let anchors: Vec<i32> = (0..nsuper).map(|i| bits(pref, nsuper * b4 + i * b5, b5) as i32).collect();
    // unary group widths up to the top set bit of the prefix
    let top = pref.iter().rposition(|&x| x != 0).map_or(0, |i| 8 * i + 8 - pref[i].leading_zeros() as usize);
    let ngroups = frames.saturating_sub(1).div_ceil(8);
    let mut runs = Vec::with_capacity(ngroups);
    let mut p = nsuper * (b4 + b5);
    while p < top {
        let mut z = 0;
        while p < top && bits(pref, p, 1) == 0 {
            z += 1;
            p += 1;
        }
        if p < top {
            p += 1;
            runs.push(z);
        }
    }
    if runs.len() != ngroups || (0..nsuper).any(|sb| runs[..(8 * sb).min(runs.len())].iter().sum::<usize>() != offsets[sb]) {
        bail!("class 10 group widths do not match the superblock offsets");
    }
    let mut d = Vec::with_capacity(8 * runs.len());
    let mut bp = 0;
    for &w in &runs {
        for lane in &lanes {
            let u = bits(lane, bp, w);
            d.push(((u >> 1) as i32) ^ -((u & 1) as i32));
        }
        bp += w;
    }
    let mut v = vec![0i32; frames];
    for sb in 0..nsuper {
        let t0 = 64 * sb;
        let (mut pos, mut vel) = (anchors[sb], 0i32);
        v[t0] = pos;
        for t in t0 + 1..frames.min(t0 + 64) {
            vel = vel.wrapping_add(d.get(t - 1).copied().unwrap_or(0));
            pos = pos.wrapping_add(vel);
            v[t] = pos;
        }
        if sb + 1 < nsuper {
            vel = vel.wrapping_add(d.get(t0 + 63).copied().unwrap_or(0));
            if pos.wrapping_add(vel) != anchors[sb + 1] {
                bail!("class 10 superblock {sb} misses the next anchor");
            }
        }
    }
    Ok(v.into_iter().map(|x| off + x as f32 * q).collect())
}

/// Decode one block: float slots × frames.
fn block(b: &[u8], o: usize, ntracks: usize) -> Result<(usize, Vec<Vec<f32>>)> {
    let size = u32_at(b, o + 4)? as usize;
    let data_off = u32_at(b, o + 0x0c)? as usize;
    let index_end = u32_at(b, o + 0x10)? as usize;
    let nf = u16_at(b, o + 0x16)? as usize;
    let stride = u16_at(b, o + 0x18)? as usize;
    let bpf = u16_at(b, o + 0x1c)? as usize;
    if *b.get(o + 0x1e).unwrap_or(&0) != 11 || o + 0x20 + size > b.len() {
        bail!("block at 0x{o:x}: not an 11-class sequence");
    }
    let blk = &b[o..o + 0x20 + size];
    let counts_at = 0x20 + data_off + nf * stride;
    let counts: Vec<usize> = (0..11).map(|i| u16_at(blk, counts_at + 2 * i).map(|x| x as usize)).collect::<Result<_>>()?;
    let mut ip = counts_at + 22;
    let mut idx: Vec<Vec<usize>> = vec![Vec::new(); 11];
    for c in 0..11 {
        if counts[c] > 0 {
            idx[c] = (0..counts[c]).map(|k| u16_at(blk, ip + 2 * k).map(|x| x as usize)).collect::<Result<_>>()?;
            ip += 2 * ((counts[c] + 3) & !3);
        }
    }
    if ip != index_end {
        bail!("block at 0x{o:x}: index table ends at 0x{ip:x}, header says 0x{index_end:x}");
    }
    let nslots = (4 * ntracks).max(idx.iter().flatten().max().map_or(0, |m| (m + 4) & !3));
    let mut sl = vec![vec![0f32; nf]; nslots];
    let mut pos = 0x20;
    for &s in &idx[0] {
        let (x, y, z) = (f32_at(blk, pos)?, f32_at(blk, pos + 4)?, f32_at(blk, pos + 8)?);
        pos += 12;
        let w = (1.0 - (x * x + y * y + z * z)).max(0.0).sqrt();
        for (k, v) in [x, y, z, w].into_iter().enumerate() {
            if let Some(c) = sl.get_mut(s + k) {
                c.fill(v);
            }
        }
    }
    for &s in &idx[1] {
        for k in 0..3 {
            let v = f32_at(blk, pos + 4 * k)?;
            if let Some(c) = sl.get_mut(s + k) {
                c.fill(v);
            }
        }
        pos += 12;
    }
    for &s in &idx[2] {
        let v = f32_at(blk, pos)?;
        pos += 4;
        if let Some(c) = sl.get_mut(s) {
            c.fill(v);
        }
    }
    let mut q4 = Vec::new();
    for _ in &idx[4] {
        q4.push((u32_at(blk, pos)? as usize, f32_at(blk, pos + 4)?, f32_at(blk, pos + 8)?));
        pos += 12;
    }
    if q4.iter().map(|x| x.0).sum::<usize>() != bpf {
        bail!("block at 0x{o:x}: class 4 bits do not add up to the header's bits per frame");
    }
    let mut c5 = Vec::new();
    for _ in &idx[5] {
        let (ib, vb, dw) = (u32_at(blk, pos)? as usize, u32_at(blk, pos + 4)? as usize, u32_at(blk, pos + 8)? as usize);
        let (q, off) = (f32_at(blk, pos + 12)?, f32_at(blk, pos + 16)?);
        let lut_raw = blk.get(pos + 20..pos + 20 + 4 * dw).ok_or_else(|| anyhow!("class 5 table past the block"))?;
        let lut: Vec<f32> = if vb == 0 { Vec::new() } else { (0..32 * dw / vb).map(|i| off + bits(lut_raw, i * vb, vb) as f32 * q).collect() };
        c5.push((ib, lut));
        pos += 20 + 4 * dw;
    }
    for &s in &idx[10] {
        let dw = u32_at(blk, pos)? as usize;
        let raw = blk.get(pos..pos + 4 * dw).ok_or_else(|| anyhow!("class 10 channel past the block"))?;
        let v = class10(nf, raw)?;
        if let Some(c) = sl.get_mut(s) {
            *c = v;
        }
        pos += 4 * dw;
    }
    if pos != 0x20 + data_off {
        bail!("block at 0x{o:x}: channel descriptors end at 0x{pos:x}, expected 0x{:x}", 0x20 + data_off);
    }
    if !q4.is_empty() || !c5.is_empty() {
        let base = 0x20 + data_off;
        for f in 0..nf {
            let fr = &blk[base + f * stride..base + (f + 1) * stride];
            let mut bp = 0;
            for (k, &(n, q, off)) in q4.iter().enumerate() {
                let u = bits(fr, bp, n);
                bp += n;
                if let Some(c) = sl.get_mut(idx[4][k]) {
                    c[f] = off + u as f32 * q;
                }
            }
            for (k, (n, lut)) in c5.iter().enumerate() {
                let u = bits(fr, bp, *n) as usize;
                bp += n;
                if let (Some(c), Some(&v)) = (sl.get_mut(idx[5][k]), lut.get(u)) {
                    c[f] = v;
                }
            }
        }
    }
    // class 7: the component at slot & 3 was dropped; rebuild and normalise
    for &s in &idx[7] {
        let (base, k) = (s & !3, s & 3);
        if base + 3 >= nslots {
            continue;
        }
        for f in 0..nf {
            let (a, b2, c) = (sl[base][f], sl[base + 1][f], sl[base + 2][f]);
            let r = (1.0 - (a * a + b2 * b2 + c * c)).max(0.0).sqrt();
            let q = match k {
                0 => [r, a, b2, c],
                1 => [a, r, b2, c],
                2 => [a, b2, r, c],
                _ => [a, b2, c, r],
            };
            let n = (q.iter().map(|x| x * x).sum::<f32>()).sqrt();
            for i in 0..4 {
                sl[base + i][f] = if n > 1e-8 { q[i] / n } else { q[i] };
            }
        }
    }
    for &s in &idx[8] {
        let base = s & !3;
        if base + 3 >= nslots {
            continue;
        }
        for f in 0..nf {
            let n = (0..4).map(|i| sl[base + i][f] * sl[base + i][f]).sum::<f32>().sqrt();
            if n > 1e-8 {
                for i in 0..4 {
                    sl[base + i][f] /= n;
                }
            }
        }
    }
    Ok((nf, sl))
}

fn animation(b: &[u8], o: usize, hash: u32) -> Result<Animation> {
    let frames = u16_at(b, o + 0x24)? as usize;
    let duration = f32_at(b, o + 0x28)?;
    let name = str_field(b, o + 0x30).unwrap_or_else(|| format!("anim_{hash:08x}"));
    // Some animations have no blocks or tracks here (empty, or stored
    // elsewhere): they decode to no frames.
    let (Some(blocks), Some(tracks_at)) = (ptr(b, o + 0x50), ptr(b, o + 0x60)) else {
        return Ok(Animation { hash, name, frames: 0, duration, tracks: Vec::new() });
    };
    let nblocks = u16_at(b, o + 0x58)? as usize;
    let ntracks = u16_at(b, o + 0x68)? as usize;
    let mut tracks = Vec::with_capacity(ntracks);
    for t in 0..ntracks {
        let p = tracks_at + 12 * t;
        let value = match u32_at(b, p + 8)? {
            0 => ValueType::Quaternion,
            2 => ValueType::Vector3,
            _ => ValueType::Float,
        };
        tracks.push(Track { bone: u32_at(b, p)?, kind: u32_at(b, p + 4)?, value, frames: Vec::with_capacity(frames) });
    }
    for k in 0..nblocks {
        let bo = ptr(b, blocks + 8 * k).ok_or_else(|| anyhow!("bad block pointer {:#x}", u64::from_le_bytes(b.get(blocks + 8 * k..blocks + 8 * k + 8).and_then(|x| x.try_into().ok()).unwrap_or([0; 8]))))?;
        let (nf, sl) = block(b, bo, ntracks)?;
        // blocks overlap by one boundary frame
        for f in (if k == 0 { 0 } else { 1 })..nf {
            for (t, tr) in tracks.iter_mut().enumerate() {
                let g = |i: usize, d: f32| sl.get(4 * t + i).map_or(d, |c| c[f]);
                tr.frames.push([g(0, 0.0), g(1, 0.0), g(2, 0.0), g(3, if tr.value == ValueType::Quaternion { 1.0 } else { 0.0 })]);
            }
        }
    }
    Ok(Animation { hash, name, frames, duration, tracks })
}

impl ClipDictionary {
    /// Decode a clip dictionary from its system page.
    pub fn parse(b: &[u8]) -> Result<Self> {
        if !is_clip_dictionary(b) {
            bail!("not a clip dictionary");
        }
        let name = str_field(b, 0x38).unwrap_or_default();
        let mut animations = Vec::new();
        let mut anim_at: std::collections::HashMap<usize, usize> = Default::default();
        if let Some(arr) = ptr(b, 0x70) {
            for i in 0..u16_at(b, 0x78)? as usize {
                let Some(ad) = ptr(b, arr + 16 * i) else { continue };
                let Some(buckets) = ptr(b, ad + 0x48) else { continue };
                for (h, ao) in walk_map(b, buckets, u16_at(b, ad + 0x50)? as usize) {
                    anim_at.insert(ao, animations.len());
                    animations.push(animation(b, ao, h)?);
                }
            }
        }
        let buckets = ptr(b, 0x48).ok_or_else(|| anyhow!("clip map missing"))?;
        let mut clips = Vec::new();
        for (h, co) in walk_map(b, buckets, u16_at(b, 0x50)? as usize) {
            let kind = u32_at(b, co + 0x10)?;
            if kind != 1 {
                clips.push(Clip { hash: h, kind, name: format!("clip_{h:08x}"), duration: 0.0, bindings: Vec::new() });
                continue;
            }
            let name = str_field(b, co + 0x28).unwrap_or_else(|| format!("clip_{h:08x}"));
            let mut duration = f32_at(b, co + 0x78)?;
            let mut bindings = Vec::new();
            // Single-animation clips (map object clips such as swaying
            // cables): +0x68 points at the animation itself, with start,
            // end and rate at +0x90 and the duration field left 0.
            if let Some(&animation) = ptr(b, co + 0x68).and_then(|ao| anim_at.get(&ao)) {
                let (start, end, rate) = (f32_at(b, co + 0x90)?, f32_at(b, co + 0x94)?, f32_at(b, co + 0x98)?);
                if duration <= 0.0 {
                    duration = (end - start) / if rate.abs() > 1e-6 { rate } else { 1.0 };
                }
                bindings.push(Binding { animation, start, end, rate });
            } else if let Some(holder) = ptr(b, co + 0x68) {
                if let Some(arr) = ptr(b, holder) {
                    for i in 0..u16_at(b, holder + 8)? as usize {
                        let e = arr + 0x38 * i;
                        let Some(&animation) = ptr(b, e).and_then(|ao| anim_at.get(&ao)) else { continue };
                        bindings.push(Binding { animation, start: f32_at(b, e + 0x28)?, end: f32_at(b, e + 0x2c)?, rate: f32_at(b, e + 0x30)? });
                    }
                }
            }
            clips.push(Clip { hash: h, kind, name, duration, bindings });
        }
        clips.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ClipDictionary { name, clips, animations })
    }

    /// A clip by short name (case-insensitive), full name or hex hash.
    pub fn clip(&self, q: &str) -> Option<&Clip> {
        let ql = q.to_ascii_lowercase();
        let h = u32::from_str_radix(q.trim_start_matches("0x"), 16).ok();
        self.clips.iter().find(|c| c.short_name().eq_ignore_ascii_case(&ql) || c.name.eq_ignore_ascii_case(&ql) || Some(c.hash) == h || c.hash == crate::meta7a::joaat(&ql))
    }
}

/// Keyframes of one bone track over a clip's time line.
pub struct ClipTrack {
    pub bone: u32,
    pub kind: u32,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

/// The bone tracks of a clip on the clip's time line (seconds from 0 to
/// the clip duration). Each binding contributes its animation's frames
/// inside [start, end]; when two bindings carry the same (bone, kind), the
/// first wins. Mover tracks (bone 0) and float tracks are left out.
pub fn clip_tracks(d: &ClipDictionary, c: &Clip) -> Vec<ClipTrack> {
    let mut out: Vec<ClipTrack> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for bd in &c.bindings {
        let a = &d.animations[bd.animation];
        if a.frames < 1 {
            continue;
        }
        let dt = if a.frames > 1 { a.duration / (a.frames - 1) as f32 } else { 0.0 };
        let rate = if bd.rate.abs() > 1e-6 { bd.rate } else { 1.0 };
        let eps = 1e-3 * dt.max(1e-3);
        let mut keys: Vec<(usize, f32)> = Vec::new();
        for f in 0..a.frames {
            let t = f as f32 * dt;
            if t + eps < bd.start || t - eps > bd.end {
                continue;
            }
            let ct = ((t - bd.start) / rate).clamp(0.0, c.duration.max(0.0));
            if keys.last().is_none_or(|k| ct > k.1) {
                keys.push((f, ct));
            }
        }
        if keys.is_empty() {
            continue;
        }
        for tr in &a.tracks {
            if tr.bone == 0 || tr.value == ValueType::Float || tr.frames.len() < a.frames || !seen.insert((tr.bone, tr.kind)) {
                continue;
            }
            out.push(ClipTrack {
                bone: tr.bone,
                kind: tr.kind,
                times: keys.iter().map(|k| k.1).collect(),
                values: keys.iter().map(|k| tr.frames.get(k.0).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])).collect(),
            });
        }
    }
    out
}

/// Every bone track of a clip, mover and float tracks included, for
/// coverage counts: (bone, kind).
pub fn all_tracks(d: &ClipDictionary, c: &Clip) -> Vec<(u32, u32, ValueType)> {
    let mut v: Vec<(u32, u32, ValueType)> = c.bindings.iter().flat_map(|bd| d.animations[bd.animation].tracks.iter().map(|t| (t.bone, t.kind, t.value))).collect();
    v.sort_by_key(|x| (x.0, x.1));
    v.dedup_by_key(|x| (x.0, x.1));
    v
}

/// A clip dictionary in `anim.rpf`: its name and index entry id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DictEntry {
    pub name: String,
    pub id: String,
}

/// File (in the index folder) caching the clip dictionary names.
pub const DICT_CACHE: &str = "anim.clipdicts.json";

/// Every clip dictionary of `anim.rpf` by name. The first call reads the
/// system page of each resource in the anim.rpf index (about a minute) and
/// caches the list as `<work>/index/anim.clipdicts.json`.
pub fn dictionaries(game_root: &std::path::Path, work: &std::path::Path) -> Result<Vec<DictEntry>> {
    use rayon::prelude::*;
    let cache = work.join("index").join(DICT_CACHE);
    if let Ok(b) = std::fs::read(&cache) {
        if let Ok(v) = serde_json::from_slice::<Vec<DictEntry>>(&b) {
            return Ok(v);
        }
    }
    let ixp = crate::scan::index_path(&work.join("index"), "anim.rpf");
    let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("anim.rpf index ({}): {e}; run `sixng index anim` first", ixp.display()))?;
    let arch = game_root.join("anim.rpf");
    let mut v: Vec<DictEntry> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::RageObj && !e.streams.is_empty())
        .filter_map(|e| {
            let r = crate::resource::Resource::load_sys(&arch, e).ok()?;
            Some(DictEntry { name: dictionary_name(&r.sys)?, id: e.id.clone() })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let _ = std::fs::write(&cache, serde_json::to_vec(&v)?);
    Ok(v)
}

/// Load a clip dictionary by entry id (`anim.rpf@1a2b3c`) or by name
/// (exact, case-insensitive; see `dictionaries`). An entry id of another
/// archive also works: map archives keep unnamed clip dictionaries for
/// animated map objects (cables, canopies, flags: 39 in the minigame pack,
/// `pack/shared_minigame000/shared_minigame000.rpf@273af010`).
/// Cache of the clip dictionaries in `levels.rpf`, in `<work>/index`.
pub const LEVEL_DICT_CACHE: &str = "levels.clipdicts.json";

/// The clip dictionaries of `levels.rpf` (170: map object clips and the
/// vehicle dictionaries `va_<model>`). They are entries without a recovered
/// name, so every unnamed object's system page is read once and the names
/// are cached in `<work>/index/levels.clipdicts.json`.
pub fn level_dictionaries(game_root: &std::path::Path, work: &std::path::Path) -> Result<Vec<DictEntry>> {
    use rayon::prelude::*;
    let cache = work.join("index").join(LEVEL_DICT_CACHE);
    if let Ok(b) = std::fs::read(&cache) {
        if let Ok(v) = serde_json::from_slice::<Vec<DictEntry>>(&b) {
            return Ok(v);
        }
    }
    let ixp = crate::scan::index_path(&work.join("index"), "levels.rpf");
    let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("levels.rpf index ({}): {e}; run `sixng index levels` first", ixp.display()))?;
    let arch = game_root.join("levels.rpf");
    let mut v: Vec<DictEntry> = ix
        .entries
        .par_iter()
        .filter(|e| e.kind == crate::classify::Kind::RageObj && !e.streams.is_empty() && e.rtype.is_none() && e.name.is_none())
        .filter_map(|e| {
            let r = crate::resource::Resource::load_sys(&arch, e).ok()?;
            Some(DictEntry { name: dictionary_name(&r.sys)?, id: e.id.clone() })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let _ = std::fs::write(&cache, serde_json::to_vec(&v)?);
    Ok(v)
}

/// A convertible's roof animation. The vehicle's model info names it as in
/// GTA V: `animConvRoofDictName` = joaat("va_<model>") and `animConvRoofName`
/// = joaat("roof"). The dictionary is in `levels.rpf`; its one clip moves the
/// bones under `convertible_roof` from roof up (time 0) to roof stowed.
/// `model` may carry a LOD suffix (`_hi`, `_mid`).
pub fn vehicle_roof(game_root: &std::path::Path, work: &std::path::Path, model: &str) -> Result<Option<(DictEntry, ClipDictionary)>> {
    let m = model.to_ascii_lowercase();
    let m = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
    let want = format!("va_{m}");
    let Some(de) = level_dictionaries(game_root, work)?.into_iter().find(|d| d.name.eq_ignore_ascii_case(&want)) else { return Ok(None) };
    let (de, d) = load_dictionary(game_root, work, &de.id)?;
    Ok(d.clip("roof").is_some().then_some((de, d)))
}

/// A vehicle's own clip dictionary `va_<model>` with whatever clips it holds
/// (`roof` on a convertible, `sunroof_r_anim` on one SUV).
pub fn vehicle_clips(game_root: &std::path::Path, work: &std::path::Path, model: &str) -> Result<Option<(DictEntry, ClipDictionary)>> {
    let m = model.to_ascii_lowercase();
    let m = m.strip_suffix("_hi").or_else(|| m.strip_suffix("_mid")).unwrap_or(&m);
    let want = format!("va_{m}");
    let Some(de) = level_dictionaries(game_root, work)?.into_iter().find(|d| d.name.eq_ignore_ascii_case(&want)) else { return Ok(None) };
    load_dictionary(game_root, work, &de.id).map(Some)
}

pub fn load_dictionary(game_root: &std::path::Path, work: &std::path::Path, q: &str) -> Result<(DictEntry, ClipDictionary)> {
    if let Some((arch, _)) = q.split_once(".rpf@").filter(|(a, _)| *a != "anim") {
        let arch = format!("{arch}.rpf");
        let ixp = crate::scan::index_path(&work.join("index"), &arch);
        let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("{arch} index ({}): {e}; index it first", ixp.display()))?;
        let e = ix.entries.iter().find(|e| e.id == q).ok_or_else(|| anyhow!("{q} is not in the {arch} index"))?;
        let d = ClipDictionary::parse(&dictionary_bytes(&game_root.join(&arch), e)?)?;
        return Ok((DictEntry { name: d.name.clone(), id: q.to_string() }, d));
    }
    let ixp = crate::scan::index_path(&work.join("index"), "anim.rpf");
    let ix = crate::scan::load_index(&ixp).map_err(|e| anyhow!("anim.rpf index ({}): {e}; run `sixng index anim` first", ixp.display()))?;
    let id = if q.starts_with("anim.rpf@") {
        q.to_string()
    } else {
        let all = dictionaries(game_root, work)?;
        all.iter().find(|d| d.name.eq_ignore_ascii_case(q)).map(|d| d.id.clone()).ok_or_else(|| anyhow!("no clip dictionary named {q:?} (`sixng clips --name <regex>` lists them)"))?
    };
    let e = ix.entries.iter().find(|e| e.id == id).ok_or_else(|| anyhow!("{id} is not in the anim.rpf index"))?;
    let d = ClipDictionary::parse(&dictionary_bytes(&game_root.join("anim.rpf"), e)?)?;
    Ok((DictEntry { name: d.name.clone(), id }, d))
}

/// The bytes a clip dictionary's system pointers address. Large
/// dictionaries continue past the first system page in the next stream (the
/// index counts it as a graphics page; their blocks point there, at
/// 0x5000_0000 + the system page size), so all streams are joined.
pub fn dictionary_bytes(archive: &std::path::Path, e: &crate::scan::Entry) -> Result<Vec<u8>> {
    let r = crate::resource::Resource::load(archive, e)?;
    let mut b = r.sys;
    b.extend_from_slice(&r.gfx);
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_hashes() {
        assert_eq!(crate::meta7a::joaat("bonerotation"), track::ROTATION);
        assert_eq!(crate::meta7a::joaat("bonetranslation"), track::TRANSLATION);
        assert_eq!(crate::meta7a::joaat("skel_root"), 0x6686685c);
    }

    #[test]
    fn bits_lsb_first() {
        let b = [0b1010_1100u8, 0b0000_0011];
        assert_eq!(bits(&b, 0, 4), 0b1100);
        assert_eq!(bits(&b, 4, 6), 0b11_1010);
        assert_eq!(bits(&b, 0, 0), 0);
    }

    /// A class 10 curve built by hand: anchors 10 then the integral of
    /// the second differences [1, 1, -1, …] over 9 frames.
    #[test]
    fn class10_curve() {
        // 9 frames, 1 superblock, 1 group of 8 deltas, width 2 bits.
        let frames = 9;
        let deltas = [1i32, 1, -1, 0, 0, 0, 0, 0];
        let zz = |d: i32| ((d << 1) ^ (d >> 31)) as u32;
        // prefix: offsets (b4 = 0), anchor (b5 = 5 bits: 10), unary width 2 = "001"
        let mut pref = 0u32;
        pref |= 10; // 5 bits
        pref |= 0b100 << 5; // two zeros then a one, LSB first
        // lanes: one dword per lane (b6 = 1), lane l holds delta l in 2 bits
        let mut raw = vec![0u8; 16];
        let dw = 4 + 1 + 8; // header + 1 prefix dword + 8 lane dwords
        raw[0..4].copy_from_slice(&(dw as u32).to_le_bytes());
        raw[4] = 0;
        raw[5] = 5;
        raw[6] = 1;
        raw[8..12].copy_from_slice(&0.5f32.to_le_bytes());
        raw[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        raw.extend_from_slice(&pref.to_le_bytes());
        for d in deltas {
            raw.extend_from_slice(&zz(d).to_le_bytes());
        }
        let v = class10(frames, &raw).unwrap();
        // ints: 10, 11, 13, 14, 15, 16, 17, 18, 19
        let ints = [10, 11, 13, 14, 15, 16, 17, 18, 19];
        for (a, b) in v.iter().zip(ints) {
            assert!((a - (1.0 + 0.5 * b as f32)).abs() < 1e-6, "{v:?}");
        }
    }
}
