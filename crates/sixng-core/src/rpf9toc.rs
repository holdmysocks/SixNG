//! RPF9 table of contents.
//!
//! The table is a small column store behind the 0x40-byte header. It is read
//! through an [`ArchiveCipher`] ([`crate::gamecipher`]).
//!
//! ```text
//! 0x00 header, plain (see rpf9.rs): 0x04 toc_end, 0x08 idx_end, 0x1a u16 nsec,
//!      0x1c u32 bsz, 0x20 u16 ncol, 0x22 u16 n22, 0x24 u32 rows, 0x2c u8 w,
//!      0x2e u8 crypto selector, 0x30 u32 tail
//! A = 0x40            section table: nsec x { u16 id, u16, u32 offset from B }
//! B = A + r16(nsec*8) block of bsz bytes holding the sections
//!                     [A, B) and [B, B+bsz-tail): block cipher, built-in key
//!                     picked by the selector, one chained run.
//!                     Section 1 = { u32 key id, u16 type, 16-byte IV at +6 }:
//!                     with that key, [B+bsz-tail, B+bsz) and then
//!                     [B+bsz, idx_end), chained (IV reset for types 3..5).
//! C = r16(B + bsz)    column table: ncol x { u64 name hash, u32 count,
//!                     u32 offset, u16 element size, u16 flags }
//! D = C + ncol*24     row index: for each row, one w-byte element index per
//!                     flag-0 column, in column order; all ones = no value
//! T = align8(D + ..)  column data; a flag-0 column's elements start at
//!                     T + offset. A flag-1 column has one element per row,
//!                     starting at idx_end.
//! ```
//!
//! Columns read so far:
//! * `33f70dfaf2c50989` file entry, 24 bytes: u64 q (low 40 bits = offset / 16,
//!   the rest flags), u64 stored size / 16, u64 unpacked size.
//! * `286030a358a789fb` crypto record, 64 bytes: u32 key id, u16 type (0x11 =
//!   stream cipher), IV at +6, five u64 at +0x18: first, on, off, ?, ?.
//!
//! An encrypted file is one cipher stream over its stored bytes (the keystream
//! position is the file offset), but only some spans are encrypted:
//! `[0, first+on)` and then `on` bytes after every `off` bytes. `off` = 0 means
//! the whole file.
//!
//! A stored file is a row of chunks, each on a 16-byte boundary: a zlib stream
//! for up to 64 KiB of data, or those 64 KiB as they are when they did not
//! compress. Entry flag 0x100 means one raw deflate stream instead.
use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};

pub const COL_ENTRY: u64 = 0x33f7_0dfa_f2c5_0989;
pub const COL_CRYPTO: u64 = 0x2860_30a3_58a7_89fb;
/// Entry flag: the stored bytes are one raw deflate stream.
pub const FLAG_RAW_DEFLATE: u32 = 0x100;
const CHUNK: usize = 0x10000;
/// Larger than any table seen (the biggest is a few MiB); guards bad headers.
const MAX_TOC: u32 = 1 << 28;

/// The decryption an archive needs. Contexts are opaque handles owned by the
/// cipher.
pub trait ArchiveCipher {
    /// Block-cipher context for the built-in key the header selector names.
    fn header_key(&mut self, selector: u8) -> Result<usize>;
    /// Block-cipher context for a key id and IV from a descriptor.
    fn block_key(&mut self, key_id: u32, iv: &[u8; 16]) -> Result<usize>;
    /// Restart the chain of a block-cipher context.
    fn block_reset(&mut self, ctx: usize, iv: &[u8; 16]) -> Result<()>;
    /// Decrypt whole 16-byte blocks in place, continuing the context's chain.
    /// A trailing partial block is left alone.
    fn block_decrypt(&mut self, ctx: usize, buf: &mut [u8]) -> Result<()>;
    /// Decrypt `buf` in place as one stream from position 0.
    fn stream_decrypt(&mut self, key_id: u32, iv: &[u8; 16], buf: &mut [u8]) -> Result<()>;
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    #[serde(serialize_with = "hex64")]
    pub hash: u64,
    pub count: u32,
    pub offset: u32,
    pub elem_size: u16,
    pub flags: u16,
}

fn hex64<S: serde::Serializer>(v: &u64, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{v:016x}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoRecord {
    pub key_id: u32,
    pub kind: u16,
    pub iv: [u8; 16],
    /// first, on, off, and two values not understood yet.
    pub pattern: [u64; 5],
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub row: u32,
    /// Offset from the start of the archive.
    pub offset: u64,
    pub flags: u32,
    pub stored: u64,
    pub unpacked: u64,
    pub crypto: Option<CryptoRecord>,
}

/// A decrypted table of contents.
pub struct Toc {
    /// Header and table, decrypted, `toc_end` bytes.
    pub raw: Vec<u8>,
    pub selector: u8,
    pub rows: u32,
    pub index_width: u8,
    pub columns: Vec<Column>,
    /// Section id -> offset in `raw`.
    pub sections: Vec<(u16, usize)>,
    indexed: Vec<usize>,
    d: usize,
    t: usize,
    idx_end: usize,
}

fn r16(x: usize) -> usize {
    (x + 15) & !15
}
fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn span(b: &mut [u8], from: usize, to: usize) -> Result<&mut [u8]> {
    b.get_mut(from..to).ok_or_else(|| anyhow!("table of contents: span {from:#x}..{to:#x} is outside the table"))
}

/// Read and decrypt the table of contents of the archive that starts at `base`
/// (0 for a file on disk, the entry offset for a nested archive).
pub fn open_toc<R: Read + Seek>(cipher: &mut dyn ArchiveCipher, f: &mut R, base: u64) -> Result<Toc> {
    let mut h = vec![0u8; 0x40];
    f.seek(SeekFrom::Start(base))?;
    f.read_exact(&mut h)?;
    ensure!(&h[..4] == b"9FPR", "not an RPF9 archive at {base:#x}");
    let toc_end = u32le(&h, 4);
    let idx_end = u32le(&h, 8) as usize;
    ensure!((0x40..=MAX_TOC).contains(&toc_end), "table of contents size {toc_end:#x} is not plausible");
    h.resize(toc_end as usize, 0);
    f.read_exact(&mut h[0x40..]).context("table of contents is cut short")?;
    let nsec = u16le(&h, 0x1a) as usize;
    let bsz = u32le(&h, 0x1c) as usize;
    let tail = u32le(&h, 0x30) as usize;
    let selector = h[0x2e];
    let a = 0x40;
    let b = a + r16(nsec * 8);
    ensure!(tail <= bsz && b + bsz <= h.len() && idx_end <= h.len(), "table of contents header does not add up");
    if selector != 0 {
        let mode = (selector & 1) + 1;
        ensure!(mode == 2, "header crypto mode {mode} is not supported");
        let ctx = cipher.header_key(selector >> 1)?;
        cipher.block_decrypt(ctx, span(&mut h, a, b)?)?;
        cipher.block_decrypt(ctx, span(&mut h, b, b + bsz - tail)?)?;
    }
    let mut sections = Vec::with_capacity(nsec);
    for i in 0..nsec {
        let o = a + i * 8;
        sections.push((u16le(&h, o), b + u32le(&h, o + 4) as usize));
    }
    if let Some(&(_, d)) = sections.iter().find(|s| s.0 == 1) {
        ensure!(d + 22 <= h.len(), "key descriptor is outside the table");
        let key_id = u32le(&h, d);
        let kind = u16le(&h, d + 4);
        let iv: [u8; 16] = h[d + 6..d + 22].try_into().unwrap();
        let ctx = cipher.block_key(key_id, &iv)?;
        let s0 = b + bsz - tail;
        if tail != 0 {
            cipher.block_decrypt(ctx, span(&mut h, s0, s0 + tail)?)?;
            if (3..=5).contains(&kind) {
                cipher.block_reset(ctx, &iv)?;
            }
        }
        cipher.block_decrypt(ctx, span(&mut h, b + bsz, idx_end)?)?;
    }
    Toc::parse(h, sections)
}

impl Toc {
    fn parse(raw: Vec<u8>, sections: Vec<(u16, usize)>) -> Result<Toc> {
        let h = &raw;
        let nsec = u16le(h, 0x1a) as usize;
        let bsz = u32le(h, 0x1c) as usize;
        let ncol = u16le(h, 0x20) as usize;
        let n22 = u16le(h, 0x22) as usize;
        let rows = u32le(h, 0x24);
        let w = h[0x2c];
        let idx_end = u32le(h, 8) as usize;
        let c = r16(0x40 + r16(nsec * 8) + bsz);
        let d = c + ncol * 24;
        ensure!(d <= h.len(), "column table is outside the table of contents");
        let columns: Vec<Column> = (0..ncol)
            .map(|i| {
                let o = c + i * 24;
                Column { hash: u64le(h, o), count: u32le(h, o + 8), offset: u32le(h, o + 12), elem_size: u16le(h, o + 16), flags: u16le(h, o + 18) }
            })
            .collect();
        let indexed: Vec<usize> = (0..ncol).filter(|&i| columns[i].flags == 0).collect();
        ensure!(ncol == 0 || (1..=4).contains(&w), "row index width {w} is not supported");
        ensure!(
            indexed.len() + n22 == ncol,
            "column flags do not match the header ({} indexed of {ncol}, header says {n22} direct): wrong key or unknown layout",
            indexed.len()
        );
        let t = (d + indexed.len() * rows as usize * w as usize + 7) & !7;
        ensure!(t <= h.len(), "row index is outside the table of contents");
        Ok(Toc { selector: h[0x2e], rows, index_width: w, columns, sections, indexed, d, t, idx_end, raw })
    }

    pub fn column(&self, hash: u64) -> Option<usize> {
        self.columns.iter().position(|c| c.hash == hash)
    }

    /// Element index of `row` in column `col`; None when the row has no value.
    pub fn index(&self, col: usize, row: u32) -> Option<u32> {
        if row >= self.rows {
            return None;
        }
        if self.columns.get(col)?.flags != 0 {
            return Some(row);
        }
        let j = self.indexed.iter().position(|&c| c == col)?;
        let w = self.index_width as usize;
        let o = self.d + (row as usize * self.indexed.len() + j) * w;
        let mut v = 0u32;
        for (k, &x) in self.raw.get(o..o + w)?.iter().enumerate() {
            v |= (x as u32) << (8 * k);
        }
        let none = if w == 4 { u32::MAX } else { (1u32 << (8 * w)) - 1 };
        (v != none).then_some(v)
    }

    /// The bytes of element `i` of column `col`.
    pub fn cell(&self, col: usize, i: u32) -> Option<&[u8]> {
        let c = self.columns.get(col)?;
        let sz = c.elem_size as usize;
        let base = if c.flags != 0 { self.idx_end } else { self.t + c.offset as usize };
        let o = base + i as usize * sz;
        self.raw.get(o..o + sz)
    }

    pub fn has_entries(&self) -> bool {
        self.column(COL_ENTRY).is_some()
    }

    /// Every row that has a file entry.
    pub fn files(&self) -> Result<Vec<FileEntry>> {
        let ce = self.column(COL_ENTRY).ok_or_else(|| anyhow!("this archive has no file entry column of the known kind"))?;
        ensure!(self.columns[ce].elem_size == 24, "file entry size {} is not 24", self.columns[ce].elem_size);
        let cc = self.column(COL_CRYPTO).filter(|&c| self.columns[c].elem_size == 64);
        let mut out = Vec::new();
        for row in 0..self.rows {
            let Some(i) = self.index(ce, row) else { continue };
            let e = self.cell(ce, i).ok_or_else(|| anyhow!("row {row}: file entry {i} is outside the table"))?;
            let q = u64le(e, 0);
            let crypto = match cc.and_then(|c| self.index(c, row).map(|i| (c, i))) {
                None => None,
                Some((c, i)) => {
                    let d = self.cell(c, i).ok_or_else(|| anyhow!("row {row}: crypto record {i} is outside the table"))?;
                    let mut pattern = [0u64; 5];
                    for (k, p) in pattern.iter_mut().enumerate() {
                        *p = u64le(d, 0x18 + k * 8);
                    }
                    Some(CryptoRecord { key_id: u32le(d, 0), kind: u16le(d, 4), iv: d[6..22].try_into().unwrap(), pattern })
                }
            };
            out.push(FileEntry {
                row,
                offset: (q & 0xff_ffff_ffff) << 4,
                flags: (q >> 40) as u32,
                stored: u64le(e, 8).checked_mul(16).ok_or_else(|| anyhow!("row {row}: stored size overflows"))?,
                unpacked: u64le(e, 16),
                crypto,
            });
        }
        Ok(out)
    }
}

/// The encrypted byte ranges (offset, length) of a stored file of `size` bytes.
pub fn encrypted_spans(size: u64, pattern: &[u64; 5]) -> Vec<(u64, u64)> {
    let (first, on, off) = (pattern[0], pattern[1], pattern[2]);
    if off == 0 || on == 0 {
        return vec![(0, size)];
    }
    let head = first.saturating_add(on);
    let mut out = vec![(0, head.min(size))];
    let mut p = head.saturating_add(off);
    while p < size {
        out.push((p, on.min(size - p)));
        p = p.saturating_add(on).saturating_add(off);
    }
    out
}

/// How the stored bytes of a file were checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Check {
    /// Compressed: every stream ended cleanly, the size matches and all stored
    /// bytes were used.
    Inflated,
    /// Not compressed, so there is no checksum to test the bytes against.
    Stored,
}

fn inflate_one(buf: &[u8], zlib: bool, limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut d = flate2::Decompress::new(zlib);
    let mut out = Vec::with_capacity(limit.min(CHUNK).max(64));
    loop {
        let used = d.total_in() as usize;
        match d.decompress_vec(&buf[used..], &mut out, flate2::FlushDecompress::Finish) {
            Ok(flate2::Status::StreamEnd) => return Some((out, d.total_in() as usize)),
            Ok(_) => {
                if out.len() > limit {
                    return None;
                }
                if out.len() == out.capacity() {
                    out.reserve(out.len().max(CHUNK));
                } else if d.total_in() as usize == used && used == buf.len() {
                    return None; // input ran out before the stream ended
                }
            }
            Err(_) => return None,
        }
    }
}

/// Unpack the stored bytes of a file (after decryption) to `unpacked` bytes.
pub fn unpack(stored: &[u8], unpacked: u64, flags: u32) -> Result<(Vec<u8>, Check)> {
    let want = usize::try_from(unpacked)?;
    if flags & FLAG_RAW_DEFLATE != 0 {
        let (data, used) = inflate_one(stored, false, want).ok_or_else(|| anyhow!("the deflate stream is damaged"))?;
        ensure!(data.len() == want, "unpacked to {} bytes, the table says {want}", data.len());
        ensure!(stored.len() - used < 16, "{} stored bytes were left over", stored.len() - used);
        return Ok((data, Check::Inflated));
    }
    let mut out: Vec<u8> = Vec::with_capacity(want.min(1 << 28));
    let (mut p, mut raw, mut chunks) = (0usize, 0usize, 0usize);
    while p < stored.len() && out.len() < want {
        let left = want - out.len();
        chunks += 1;
        let z = if stored[p..].starts_with(&[0x78, 0xda]) { inflate_one(&stored[p..], true, left) } else { None };
        match z {
            Some((data, used)) => {
                out.extend_from_slice(&data);
                p = r16(p + used);
            }
            None => {
                let n = left.min(CHUNK);
                let part = stored.get(p..p + n).ok_or_else(|| anyhow!("stored bytes end inside a chunk at {p:#x}"))?;
                out.extend_from_slice(part);
                p = r16(p + n);
                raw += 1;
            }
        }
    }
    ensure!(out.len() == want, "unpacked to {} bytes, the table says {want}", out.len());
    ensure!(p == stored.len(), "used {p:#x} of {:#x} stored bytes", stored.len());
    Ok((out, if raw == chunks { Check::Stored } else { Check::Inflated }))
}

/// Decrypt the encrypted spans of a file's stored bytes in place.
pub fn decrypt_stored(cipher: &mut dyn ArchiveCipher, stored: &mut [u8], rec: &CryptoRecord) -> Result<()> {
    ensure!(rec.kind == 0x11, "crypto record type {:#x} is not supported", rec.kind);
    let spans = encrypted_spans(stored.len() as u64, &rec.pattern);
    if spans.len() == 1 && spans[0] == (0, stored.len() as u64) {
        return cipher.stream_decrypt(rec.key_id, &rec.iv, stored);
    }
    // The keystream runs over the whole file, so decrypt a copy up to the end
    // of the last span and keep only the spans.
    let end = spans.last().map(|s| (s.0 + s.1) as usize).unwrap_or(0);
    let mut full = stored[..end].to_vec();
    cipher.stream_decrypt(rec.key_id, &rec.iv, &mut full)?;
    for (o, n) in spans {
        let (o, n) = (o as usize, n as usize);
        stored[o..o + n].copy_from_slice(&full[o..o + n]);
    }
    Ok(())
}

/// Read one file of the archive at `base`, decrypted and unpacked.
pub fn read_file<R: Read + Seek>(cipher: &mut dyn ArchiveCipher, f: &mut R, base: u64, e: &FileEntry) -> Result<(Vec<u8>, Check)> {
    ensure!(e.stored <= 1 << 32, "stored size {:#x} is not plausible", e.stored);
    let mut stored = vec![0u8; e.stored as usize];
    f.seek(SeekFrom::Start(base + e.offset))?;
    f.read_exact(&mut stored).context("file data is cut short")?;
    if let Some(rec) = &e.crypto {
        decrypt_stored(cipher, &mut stored, rec)?;
    }
    unpack(&stored, e.unpacked, e.flags)
}

/// Offsets (from `base`) of the entries that are themselves RPF9 archives.
pub fn nested_archives<R: Read + Seek>(f: &mut R, base: u64, files: &[FileEntry]) -> Vec<u64> {
    let mut out = Vec::new();
    let mut m = [0u8; 4];
    for e in files {
        // Offset 0 is the archive's own header.
        if e.stored < 0x40 || e.offset == 0 {
            continue;
        }
        if f.seek(SeekFrom::Start(base + e.offset)).is_ok() && f.read_exact(&mut m).is_ok() && &m == b"9FPR" {
            out.push(e.offset);
        }
    }
    out
}

/// File extension for decoded bytes, by magic.
pub fn sniff_ext(d: &[u8]) -> &'static str {
    match d.get(..4) {
        Some(b"RBF0") => "rbf",
        Some(b"PSIN") => "pso",
        Some(b"9FPR") => "rpf",
        Some(b"RIFF") => "riff",
        Some(b"\x89PNG") => "png",
        Some(b"DDS ") => "dds",
        _ if d.first() == Some(&b'<') || d.starts_with(&[0xef, 0xbb, 0xbf]) => "xml",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// Stand-in cipher. Block mode chains on the previous ciphertext block so
    /// that a wrong order of runs or a missing IV reset shows; stream mode
    /// depends on the position.
    #[derive(Default)]
    struct Fake {
        ctx: Vec<(u8, [u8; 16])>,
        resets: usize,
    }
    fn key_byte(id: u32) -> u8 {
        (id as u8) ^ 0x5a
    }
    impl Fake {
        fn block_encrypt(key: u8, chain: &mut [u8; 16], buf: &mut [u8]) {
            for blk in buf.chunks_exact_mut(16) {
                for i in 0..16 {
                    blk[i] ^= key ^ chain[i];
                }
                chain.copy_from_slice(blk);
            }
        }
        fn stream(key: u8, iv: &[u8; 16], buf: &mut [u8]) {
            for (i, b) in buf.iter_mut().enumerate() {
                *b ^= key ^ iv[i % 16] ^ (i as u8).wrapping_mul(31) ^ ((i >> 8) as u8);
            }
        }
    }
    const HEADER_IV: [u8; 16] = [7; 16];
    impl ArchiveCipher for Fake {
        fn header_key(&mut self, selector: u8) -> Result<usize> {
            ensure!(selector == 4, "selector {selector}");
            self.ctx.push((key_byte(0x1000_0001), HEADER_IV));
            Ok(self.ctx.len() - 1)
        }
        fn block_key(&mut self, key_id: u32, iv: &[u8; 16]) -> Result<usize> {
            self.ctx.push((key_byte(key_id), *iv));
            Ok(self.ctx.len() - 1)
        }
        fn block_reset(&mut self, ctx: usize, iv: &[u8; 16]) -> Result<()> {
            self.ctx[ctx].1 = *iv;
            self.resets += 1;
            Ok(())
        }
        fn block_decrypt(&mut self, ctx: usize, buf: &mut [u8]) -> Result<()> {
            let (key, chain) = &mut self.ctx[ctx];
            for blk in buf.chunks_exact_mut(16) {
                let c: [u8; 16] = (&*blk).try_into().unwrap();
                for i in 0..16 {
                    blk[i] ^= *key ^ chain[i];
                }
                *chain = c;
            }
            Ok(())
        }
        fn stream_decrypt(&mut self, key_id: u32, iv: &[u8; 16], buf: &mut [u8]) -> Result<()> {
            Fake::stream(key_byte(key_id), iv, buf);
            Ok(())
        }
    }

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 24) as u8
            })
            .collect()
    }

    /// Pack like the game: 64 KiB chunks, zlib level 9, raw when no gain.
    fn pack(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in data.chunks(CHUNK) {
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            z.write_all(c).unwrap();
            let z = z.finish().unwrap();
            assert_eq!(&z[..2], &[0x78, 0xda]);
            out.extend_from_slice(if z.len() < c.len() { &z } else { c });
            out.resize(r16(out.len()), 0);
        }
        out
    }

    struct TestFile {
        data: Vec<u8>,
        raw_deflate: bool,
        crypto: Option<CryptoRecord>,
    }

    /// Build an archive: plain header, encrypted table, files on 16-byte
    /// boundaries after it. `width` is the row index width.
    fn build(files: &[TestFile], width: usize, desc_kind: u16, tail: usize) -> Vec<u8> {
        let nsec = 2usize;
        let desc_iv: [u8; 16] = *b"descriptor-iv-01";
        let desc_key = 0x1000_0007u32;
        // Block B: section 1 (key descriptor) first, section 2 filler. The
        // descriptor has to sit before the tail, which its own key encrypts.
        let bsz = 0x40usize;
        let mut blk = noise(bsz, 1);
        blk[0..4].copy_from_slice(&desc_key.to_le_bytes());
        blk[4..6].copy_from_slice(&desc_kind.to_le_bytes());
        blk[6..0x16].copy_from_slice(&desc_iv);
        let a = 0x40;
        let b = a + r16(nsec * 8);
        let c = r16(b + bsz);
        // Columns: crypto (indexed), entry (indexed), one direct column.
        let ncol = 3;
        let d = c + ncol * 24;
        let rows = files.len() + 1; // one row without a file entry
        let t = (d + 2 * rows * width + 7) & !7;
        let none = (1u64 << (8 * width)) - 1;
        let mut stored: Vec<Vec<u8>> = Vec::new();
        for f in files {
            let mut s = if f.raw_deflate {
                let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
                z.write_all(&f.data).unwrap();
                let mut z = z.finish().unwrap();
                z.resize(r16(z.len()), 0);
                z
            } else {
                pack(&f.data)
            };
            if let Some(rec) = &f.crypto {
                let mut full = s.clone();
                Fake::stream(key_byte(rec.key_id), &rec.iv, &mut full);
                for (o, n) in encrypted_spans(s.len() as u64, &rec.pattern) {
                    let (o, n) = (o as usize, n as usize);
                    s[o..o + n].copy_from_slice(&full[o..o + n]);
                }
            }
            stored.push(s);
        }
        let ncrypto = files.iter().filter(|f| f.crypto.is_some()).count();
        let crypto_off = 0usize;
        let entry_off = ncrypto * 64;
        let idx_end = r16(t + entry_off + files.len() * 24);
        let toc_end = r16(idx_end + rows * 8);
        let mut h = vec![0u8; toc_end];
        h[..4].copy_from_slice(b"9FPR");
        h[4..8].copy_from_slice(&(toc_end as u32).to_le_bytes());
        h[8..12].copy_from_slice(&(idx_end as u32).to_le_bytes());
        h[0x1a..0x1c].copy_from_slice(&(nsec as u16).to_le_bytes());
        h[0x1c..0x20].copy_from_slice(&(bsz as u32).to_le_bytes());
        h[0x20..0x22].copy_from_slice(&(ncol as u16).to_le_bytes());
        h[0x22..0x24].copy_from_slice(&1u16.to_le_bytes());
        h[0x24..0x28].copy_from_slice(&(rows as u32).to_le_bytes());
        h[0x2c] = width as u8;
        h[0x2e] = 0x09;
        h[0x30..0x34].copy_from_slice(&(tail as u32).to_le_bytes());
        for (i, (id, off)) in [(1u16, 0u32), (2, 0x20)].iter().enumerate() {
            h[a + i * 8..a + i * 8 + 2].copy_from_slice(&id.to_le_bytes());
            h[a + i * 8 + 4..a + i * 8 + 8].copy_from_slice(&off.to_le_bytes());
        }
        h[b..b + bsz].copy_from_slice(&blk);
        let cols = [(COL_CRYPTO, ncrypto, crypto_off, 64u16, 0u16), (COL_ENTRY, files.len(), entry_off, 24, 0), (0xb6f5_bdb2_c66d_a2e9, rows, 0, 8, 1)];
        for (i, (hash, count, off, sz, fl)) in cols.iter().enumerate() {
            let o = c + i * 24;
            h[o..o + 8].copy_from_slice(&hash.to_le_bytes());
            h[o + 8..o + 12].copy_from_slice(&(*count as u32).to_le_bytes());
            h[o + 12..o + 16].copy_from_slice(&(*off as u32).to_le_bytes());
            h[o + 16..o + 18].copy_from_slice(&sz.to_le_bytes());
            h[o + 18..o + 20].copy_from_slice(&fl.to_le_bytes());
        }
        let mut pos = r16(toc_end);
        let mut ci = 0u64;
        let mut body = Vec::new();
        for row in 0..rows {
            let o = d + row * 2 * width;
            // Row 0 has no file; file k sits in row k + 1.
            let (cv, ev) = match row.checked_sub(1).map(|k| (k, &files[k])) {
                None => (none, none),
                Some((k, f)) => {
                    let s = &stored[k];
                    let e = t + entry_off + k * 24;
                    let q = (pos as u64 >> 4) | if f.raw_deflate { (FLAG_RAW_DEFLATE as u64) << 40 } else { 0 };
                    h[e..e + 8].copy_from_slice(&q.to_le_bytes());
                    h[e + 8..e + 16].copy_from_slice(&(s.len() as u64 / 16).to_le_bytes());
                    h[e + 16..e + 24].copy_from_slice(&(f.data.len() as u64).to_le_bytes());
                    body.resize(pos - toc_end, 0);
                    body.extend_from_slice(s);
                    pos = r16(pos + s.len());
                    let cv = match &f.crypto {
                        None => none,
                        Some(rec) => {
                            let r = t + crypto_off + ci as usize * 64;
                            h[r..r + 4].copy_from_slice(&rec.key_id.to_le_bytes());
                            h[r + 4..r + 6].copy_from_slice(&rec.kind.to_le_bytes());
                            h[r + 6..r + 22].copy_from_slice(&rec.iv);
                            for (k, p) in rec.pattern.iter().enumerate() {
                                h[r + 0x18 + k * 8..r + 0x20 + k * 8].copy_from_slice(&p.to_le_bytes());
                            }
                            ci += 1;
                            ci - 1
                        }
                    };
                    (cv, k as u64)
                }
            };
            h[o..o + width].copy_from_slice(&cv.to_le_bytes()[..width]);
            h[o + width..o + 2 * width].copy_from_slice(&ev.to_le_bytes()[..width]);
            h[idx_end + row * 8..idx_end + row * 8 + 8].copy_from_slice(&(row as u64 * 3).to_le_bytes());
        }
        // Encrypt as the game stores it.
        let hk = key_byte(0x1000_0001);
        let mut chain = HEADER_IV;
        Fake::block_encrypt(hk, &mut chain, &mut h[a..b]);
        Fake::block_encrypt(hk, &mut chain, &mut h[b..b + bsz - tail]);
        let dk = key_byte(desc_key);
        let mut chain = desc_iv;
        if tail != 0 {
            Fake::block_encrypt(dk, &mut chain, &mut h[b + bsz - tail..b + bsz]);
            if (3..=5).contains(&desc_kind) {
                chain = desc_iv;
            }
        }
        Fake::block_encrypt(dk, &mut chain, &mut h[b + bsz..idx_end]);
        h.extend_from_slice(&body);
        h
    }

    fn rec(pattern: [u64; 5], key_id: u32) -> CryptoRecord {
        CryptoRecord { key_id, kind: 0x11, iv: *b"per-file-iv-0001", pattern }
    }

    fn sample() -> Vec<TestFile> {
        let text: Vec<u8> = (0..300_000u32).flat_map(|i| format!("<row n=\"{}\"/>\n", i % 977).into_bytes()).collect();
        let mut mixed = b"RBF0".to_vec();
        mixed.extend(noise(CHUNK - 4, 3)); // first chunk does not compress
        mixed.extend(std::iter::repeat(0x41).take(CHUNK + 100));
        vec![
            TestFile { data: text.clone(), raw_deflate: false, crypto: Some(rec([0x400, 0x400, 0x1c00, 0, 4], 0x1000_0029)) },
            TestFile { data: b"PSIN plain and short, short, short, short, short, short, short".to_vec(), raw_deflate: false, crypto: None },
            TestFile { data: mixed, raw_deflate: false, crypto: Some(rec([0x400, 0x400, 0x1c00, 0, 4], 0x1000_0030)) },
            TestFile { data: text[..40_000].to_vec(), raw_deflate: true, crypto: Some(rec([0, 0x1000, 0, 0, 4], 0x1000_002a)) },
            TestFile { data: noise(5000, 9), raw_deflate: false, crypto: None },
        ]
    }

    fn round_trip(width: usize, desc_kind: u16, tail: usize) -> Fake {
        let files = sample();
        let img = build(&files, width, desc_kind, tail);
        // As a nested archive: the same bytes at an offset.
        for base in [0u64, 0x230] {
            let mut bytes = vec![0xeeu8; base as usize];
            bytes.extend_from_slice(&img);
            let mut f = Cursor::new(bytes);
            let mut c = Fake::default();
            let toc = open_toc(&mut c, &mut f, base).unwrap();
            assert_eq!(toc.rows as usize, files.len() + 1);
            assert_eq!(toc.index_width as usize, width);
            assert_eq!(toc.sections.iter().map(|s| s.0).collect::<Vec<_>>(), [1, 2]);
            let got = toc.files().unwrap();
            assert_eq!(got.len(), files.len());
            for (e, want) in got.iter().zip(&files) {
                assert_eq!(e.crypto, want.crypto);
                assert_eq!(e.flags & FLAG_RAW_DEFLATE != 0, want.raw_deflate);
                let (data, check) = read_file(&mut c, &mut f, base, e).unwrap();
                assert!(data == want.data, "row {} differs", e.row);
                assert_eq!(check, if e.row == 5 { Check::Stored } else { Check::Inflated });
            }
            // The direct column has one element per row.
            let dc = toc.column(0xb6f5_bdb2_c66d_a2e9).unwrap();
            assert_eq!(toc.cell(dc, toc.index(dc, 2).unwrap()).unwrap(), 6u64.to_le_bytes());
            assert!(nested_archives(&mut f, base, &got).is_empty());
            if base != 0 {
                return c;
            }
        }
        unreachable!()
    }

    #[test]
    fn decodes_table_and_files() {
        let c = round_trip(1, 0x10, 0x10);
        assert_eq!(c.resets, 0);
    }

    #[test]
    fn two_byte_row_index_and_no_tail() {
        round_trip(2, 0x10, 0);
    }

    #[test]
    fn descriptor_types_3_to_5_restart_the_chain() {
        let c = round_trip(1, 4, 0x20);
        assert_eq!(c.resets, 1);
    }

    #[test]
    fn spans() {
        let p = [0x400, 0x400, 0x1c00, 0, 4];
        assert_eq!(encrypted_spans(0x300, &p), [(0, 0x300)]);
        assert_eq!(encrypted_spans(0x2400, &p), [(0, 0x800)]);
        assert_eq!(encrypted_spans(0x4500, &p), [(0, 0x800), (0x2400, 0x400), (0x4400, 0x100)]);
        // No gap: everything.
        assert_eq!(encrypted_spans(0x1270, &[0, 0x1000, 0, 0, 4]), [(0, 0x1270)]);
        assert_eq!(encrypted_spans(0xe00, &[0x1000, 0x1000, 0, 0x1000, 4]), [(0, 0xe00)]);
    }

    #[test]
    fn wrong_bytes_are_refused() {
        let files = sample();
        let mut img = build(&files, 1, 0x10, 0x10);
        let mut c = Fake::default();
        let toc = open_toc(&mut c, &mut Cursor::new(&img), 0).unwrap();
        let e = toc.files().unwrap().remove(0);
        // Damage a byte inside an encrypted span: the zlib checksum catches it.
        img[e.offset as usize + 0x2400 + 5] ^= 1;
        assert!(read_file(&mut c, &mut Cursor::new(&img), 0, &e).is_err());
        // A table decrypted with the wrong key does not parse.
        img[0x50] ^= 0x01; // key id in the descriptor (ciphertext)
        assert!(open_toc(&mut Fake::default(), &mut Cursor::new(&img), 0).and_then(|t| t.files().map(|_| ())).is_err());
        // Not an archive.
        assert!(open_toc(&mut Fake::default(), &mut Cursor::new(vec![0u8; 0x100]), 0).is_err());
    }

    #[test]
    fn sniffs() {
        assert_eq!(sniff_ext(b"RBF0...."), "rbf");
        assert_eq!(sniff_ext(b"<?xml"), "xml");
        assert_eq!(sniff_ext(b"\0\0"), "bin");
    }
}
