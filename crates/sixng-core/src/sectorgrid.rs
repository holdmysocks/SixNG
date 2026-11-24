//! World sector grids: the first eight PSO files of `levels.rpf`. Each
//! lists cells of one square world grid; the cell size is not stored.
//!
//! Each file's root (struct `d56f1e2c`) holds one array of records of
//! struct `9c834886`, 32 bytes each (little endian):
//!
//! | offset | type | field (joaat)        | meaning                                    |
//! |--------|------|----------------------|--------------------------------------------|
//! | 0x00   | i32  | `x` (`9303a5e5`)     | cell column: floor(world x / cell size)    |
//! | 0x04   | i32  | `y` (`80950108`)     | cell row: floor(world y / cell size)       |
//! | 0x08   | u32  | `53149a19`           | always 0                                   |
//! | 0x0C   | u32  | `8fab3f0b`           | bundle key (see below; name unknown)       |
//! | 0x10   | u32  | `56f5c799`           | index of the record inside its bundle      |
//! | 0x14   | 4    | padding              |                                            |
//! | 0x18   | u64  | `f24b342d`           | 0, or a map group id (map file `65876a4d`) |
//!
//! `x` and `y` are real names: their joaat hashes match.
//!
//! **Three levels, two kinds.** The files come in three cell sizes. Each
//! size has a cell list (64-bit field 0, every cell once) and a map-group
//! list (64-bit field set; a cell appears once per map group that reaches
//! it):
//!
//! | file (archive order) | records | cell size | kind                      |
//! |----------------------|---------|-----------|---------------------------|
//! | 1                    | 204     | 1024 m    | cells                     |
//! | 2                    | 63      | 1024 m    | map groups                |
//! | 3                    | 765     | 512 m     | cells                     |
//! | 4, 6                 | 75 each | 512 m     | map groups (same records) |
//! | 5                    | 135     | 512 m     | cells, a subset of file 3 |
//! | 7                    | 26,212  | 60 m      | cells (the world grid)    |
//! | 8                    | 3,269   | 60 m      | map groups                |
//!
//! How the sizes were measured, against the placements of every world map
//! file (1.67 million entities):
//! - 60 m, world grid: 94.3 % of world entities fall in a listed cell and
//!   96.8 % of cells hold one (64 m: 87.8 % / 96.0 %; 50 m: 64 %). Counting
//!   batches and road nodes too, 98 % of cells hold something.
//! - 60 m, map groups: the 64-bit field equals the map files' 64-bit field
//!   `65876a4d` (67 of 277 values occur in the decoded map files). For 37
//!   of 46 such values, the value's cells are exactly the cells under the
//!   union of its map files' entity extents (`1c75bcf1` .. `6d074437`) at
//!   60 m; at 59 m it is 5, at 61 m 7, at 64 m 1. Shifted by half a cell
//!   it is 5 of 46, so a cell's origin is its south-west corner:
//!   cell = floor(position / size).
//! - 512 m and 1024 m: the same exact-extent test peaks at 510..512 m (21
//!   of 25) and 1020..1024 m (23 of 25); the powers of two are taken. File
//!   5 is 512 m: all its cells are cells of file 3, and at 1024 m it would
//!   reach past the world box. File 1 at 1024 m and file 3 at 512 m each
//!   hold 98.8 % of the entities.
//! - The levels nest only loosely: a coarse list is not the 60 m list
//!   scaled down.
//!
//! File 5 is mostly the dense cells (median 5,056 entities per 512 m cell,
//! against 254 in file 3's other cells), but no entity or batch count
//! threshold separates it better than 89 %. Files 4 and 6 hold the same
//! cells and map groups and differ only in the bundle key. Map groups in
//! the coarse files have longer streaming extents (median 970 m against
//! 686 m in file 8).
//!
//! **Bundle key.** Records with the same key are consecutive and number 1
//! to 63; the index counts 0, 1, … inside it. In the world grid, records
//! are sorted by the text of x, then y, with negatives after positives (0,
//! 1, 10, …, 9, -1, -10, …). The keys cut that order into runs of about
//! equal content: a key's cells hold a median of 220 to 360 entities
//! whether it has 1 or 63 cells (293 entities per cell in single-cell
//! bundles, 8.7 in the largest). So a key names a bundle of cells packed in
//! name order up to a size budget. In the other files every record has its
//! own key, and the same cell has different keys in different files. The
//! keys match no recovered name, dictionary name or map file name hash. The
//! archive's entry names live in its encrypted table of contents, which
//! this project does not read.
use crate::pso;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;

/// Low 32 bits of the record struct hash (`442e838d9c834886`).
pub const RECORD: u32 = 0x9c83_4886;
/// Low 32 bits of the root struct hash (`2be82171d56f1e2c`).
pub const ROOT: u32 = 0xd56f_1e2c;
pub const RECORD_SIZE: usize = 32;
/// Measured cell size of the world grid in metres.
pub const WORLD_CELL_M: f32 = 60.0;
/// Measured cell sizes of the eight grid files of `levels.rpf`, in archive
/// order (see the module notes).
pub const LEVELS_CELL_M: [f32; 8] = [1024.0, 1024.0, 512.0, 512.0, 512.0, 512.0, 60.0, 60.0];

const F_X: u32 = 0x9303_a5e5;
const F_Y: u32 = 0x8095_0108;
const F_ZERO: u32 = 0x5314_9a19;
const F_GROUP: u32 = 0x8fab_3f0b;
const F_INDEX: u32 = 0x56f5_c799;
const F_HASH64: u32 = 0xf24b_342d;

/// The expected record schema: (field hash, offset, type code).
const SCHEMA: [(u32, u32, u8); 6] = [(F_X, 0, 20), (F_Y, 4, 20), (F_ZERO, 8, 21), (F_GROUP, 12, 21), (F_INDEX, 16, 21), (F_HASH64, 24, 79)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sector {
    pub x: i32,
    pub y: i32,
    /// Field `53149a19` (always 0 in the game's files).
    pub zero: u32,
    /// Field `8fab3f0b`: bundle key.
    pub group: u32,
    /// Field `56f5c799`: position inside the bundle.
    pub index: u32,
    /// Field `f24b342d`: 0 in cell lists; in map-group lists the map
    /// group id (the map files' `65876a4d`).
    pub hash64: u64,
}

impl Sector {
    /// World-space box of the cell: (min x, min y), (max x, max y) in metres.
    pub fn bounds(&self, cell_m: f32) -> ([f32; 2], [f32; 2]) {
        let (x, y) = (self.x as f32 * cell_m, self.y as f32 * cell_m);
        ([x, y], [x + cell_m, y + cell_m])
    }
}

/// The cell holding a world position.
pub fn cell_of(x: f32, y: f32, cell_m: f32) -> (i32, i32) {
    ((x / cell_m).floor() as i32, (y / cell_m).floor() as i32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Every 64-bit field is 0: a list of cells.
    Cells,
    /// The 64-bit field is set: map group id -> cells it reaches.
    MapGroups,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectorGrid {
    /// Records in file order.
    pub sectors: Vec<Sector>,
    /// Cell size in metres: measured, not stored. `decode` sets the world
    /// grid's 60 m; `load_all` sets each `levels.rpf` file's own size.
    pub cell_m: f32,
}

impl SectorGrid {
    pub fn kind(&self) -> Kind {
        if self.sectors.iter().any(|s| s.hash64 != 0) {
            Kind::MapGroups
        } else {
            Kind::Cells
        }
    }
    /// (min x, min y, max x, max y) over the cell indices.
    pub fn cell_range(&self) -> Option<[i32; 4]> {
        let f = self.sectors.first()?;
        Some(self.sectors.iter().fold([f.x, f.y, f.x, f.y], |b, s| [b[0].min(s.x), b[1].min(s.y), b[2].max(s.x), b[3].max(s.y)]))
    }
    /// World box of all cells: (min x, min y), (max x, max y) in metres.
    pub fn world_bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        let [x0, y0, x1, y1] = self.cell_range()?;
        let c = self.cell_m;
        Some(([x0 as f32 * c, y0 as f32 * c], [(x1 + 1) as f32 * c, (y1 + 1) as f32 * c]))
    }
    /// Number of distinct bundle keys.
    pub fn groups(&self) -> usize {
        let mut g: Vec<u32> = self.sectors.iter().map(|s| s.group).collect();
        g.sort_unstable();
        g.dedup();
        g.len()
    }
    /// Number of distinct non-zero map group ids.
    pub fn map_groups(&self) -> usize {
        let mut g: Vec<u64> = self.sectors.iter().map(|s| s.hash64).filter(|&h| h != 0).collect();
        g.sort_unstable();
        g.dedup();
        g.len()
    }
    /// The distinct cells (a map-group list repeats cells).
    pub fn cells(&self) -> std::collections::HashSet<(i32, i32)> {
        self.sectors.iter().map(|s| (s.x, s.y)).collect()
    }
    /// The cells of one map group.
    pub fn cells_of(&self, map_group: u64) -> Vec<(i32, i32)> {
        self.sectors.iter().filter(|s| s.hash64 == map_group).map(|s| (s.x, s.y)).collect()
    }
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Whether a PSO file declares the sector record struct (cheap check on the
/// parsed schema).
pub fn is_sector_grid(p: &pso::Pso) -> bool {
    p.structs.contains_key(&RECORD)
}

/// Decodes the sector records of a PSO file. The cell size is set to the
/// world grid's 60 m; `load_all` sets the measured size of each file.
pub fn decode(d: &[u8]) -> Result<SectorGrid> {
    let p = pso::parse(d).ok_or_else(|| anyhow!("not a PSO file"))?;
    decode_pso(&p)
}

pub fn decode_pso(p: &pso::Pso) -> Result<SectorGrid> {
    let sd = p.structs.get(&RECORD).ok_or_else(|| anyhow!("no sector record struct {RECORD:08x} in the schema"))?;
    if sd.size as usize != RECORD_SIZE {
        bail!("sector record is {} bytes, expected {RECORD_SIZE}", sd.size);
    }
    for (h, off, ty) in SCHEMA {
        match sd.members.iter().find(|m| m.hash == h) {
            Some(m) if m.off == off && m.ty == ty => {}
            Some(m) => bail!("sector field {h:08x}: offset {} type {}, expected {off} type {ty}", m.off, m.ty),
            None => bail!("sector field {h:08x} missing"),
        }
    }
    let sys = &p.res.sys;
    let mut sectors = Vec::new();
    for &(h, len, o) in &p.blocks {
        if h != RECORD {
            continue;
        }
        let len = len as usize;
        if o == usize::MAX || len % RECORD_SIZE != 0 || o + len > sys.len() {
            bail!("sector block at {o:#x} ({len} bytes) out of range or not a multiple of {RECORD_SIZE}");
        }
        for r in (o..o + len).step_by(RECORD_SIZE) {
            sectors.push(Sector {
                x: le32(sys, r) as i32,
                y: le32(sys, r + 4) as i32,
                zero: le32(sys, r + 8),
                group: le32(sys, r + 12),
                index: le32(sys, r + 16),
                hash64: u64::from_le_bytes(sys[r + 24..r + 32].try_into().unwrap()),
            });
        }
    }
    if sectors.is_empty() {
        bail!("no sector records");
    }
    Ok(SectorGrid { sectors, cell_m: WORLD_CELL_M })
}

/// Every sector grid of an indexed archive (normally `levels`), in archive
/// order, with its entry id. `levels.rpf` has eight; when exactly eight are
/// found, each gets its measured cell size (`LEVELS_CELL_M`), otherwise all
/// keep 60 m.
pub fn load_all(ws: &crate::export::Workspace, archive: &str) -> Result<Vec<(String, SectorGrid)>> {
    let a = ws.resolve(archive)?;
    let ix = ws.load_index(&a)?;
    // The record struct hash, as it appears in the PSO block table.
    let needle = RECORD.to_le_bytes();
    let pso_entries: Vec<_> = ix.entries.iter().filter(|e| e.kind == crate::classify::Kind::Pso && e.usize as usize >= RECORD_SIZE).collect();
    use rayon::prelude::*;
    let found: Vec<Option<(String, SectorGrid)>> = pso_entries
        .par_iter()
        .map(|e| -> Result<Option<(String, SectorGrid)>> {
            let b = crate::scan::read_entry(&a.path, e)?;
            if !b.windows(4).take(4096).any(|w| w == needle) {
                return Ok(None);
            }
            Ok(match pso::parse(&b).filter(is_sector_grid) {
                Some(p) => Some((e.id.clone(), decode_pso(&p)?)),
                None => None,
            })
        })
        .collect::<Result<_>>()?;
    // par_iter + collect keeps archive order.
    let mut out: Vec<_> = found.into_iter().flatten().collect();
    if out.len() == LEVELS_CELL_M.len() {
        for ((_, g), c) in out.iter_mut().zip(LEVELS_CELL_M) {
            g.cell_m = c;
        }
    }
    Ok(out)
}

/// Index of the grid with the most records (the world grid).
pub fn largest(grids: &[(String, SectorGrid)]) -> Option<usize> {
    (0..grids.len()).max_by_key(|&i| grids[i].1.sectors.len())
}

/// CSV text: one line per record, file order.
pub fn to_csv(g: &SectorGrid) -> String {
    let mut s = String::from("x,y,min_x_m,min_y_m,max_x_m,max_y_m,group,index,zero,hash64\n");
    for c in &g.sectors {
        let (lo, hi) = c.bounds(g.cell_m);
        s.push_str(&format!("{},{},{},{},{},{},{:08x},{},{},{:016x}\n", c.x, c.y, lo[0], lo[1], hi[0], hi[1], c.group, c.index, c.zero, c.hash64));
    }
    s
}

/// The occupied cells as an 8-bit mask, north up, one pixel per cell
/// (255 = listed). Pixel (c, r) is cell (min x + c, max y - r).
pub fn mask_north_up(g: &SectorGrid) -> Option<(u32, u32, Vec<u8>)> {
    let [x0, y0, x1, y1] = g.cell_range()?;
    let (w, h) = ((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32);
    let mut px = vec![0u8; (w * h) as usize];
    for s in &g.sectors {
        px[((y1 - s.y) as u32 * w + (s.x - x0) as u32) as usize] = 255;
    }
    Some((w, h, px))
}

pub fn write_mask_png(g: &SectorGrid, path: &std::path::Path) -> Result<()> {
    let (w, h, px) = mask_north_up(g).ok_or_else(|| anyhow!("empty grid"))?;
    image::save_buffer(path, &px, w, h, image::ColorType::L8)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PSO file: PSIN with one record block, LSCH with the record
    /// schema.
    pub(crate) fn build(records: &[(i32, i32, u32, u32)]) -> Vec<u8> {
        let mut blocks = Vec::new();
        for &(x, y, g, i) in records {
            let mut r = [0u8; RECORD_SIZE];
            r[0..4].copy_from_slice(&x.to_le_bytes());
            r[4..8].copy_from_slice(&y.to_le_bytes());
            r[12..16].copy_from_slice(&g.to_le_bytes());
            r[16..20].copy_from_slice(&i.to_le_bytes());
            blocks.extend_from_slice(&r);
        }
        // PSIN: header, one table entry, padding to 16, data.
        let mut psin = Vec::new();
        psin.extend_from_slice(&RECORD.to_le_bytes());
        psin.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
        psin.extend_from_slice(&[0xaa; 8]);
        psin.extend_from_slice(&[0x70; 8]); // table end, pad to 16
        psin.extend_from_slice(&blocks);
        let mut out = Vec::new();
        out.extend_from_slice(b"PSIN");
        out.extend_from_slice(&((psin.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(&psin);
        // LSCH: one struct entry and its members.
        let mut s = Vec::new();
        s.extend_from_slice(&(0x442e_838d_0000_0000u64 | RECORD as u64).to_le_bytes());
        s.extend_from_slice(&0x300u32.to_le_bytes());
        s.extend_from_slice(&0u32.to_le_bytes());
        s.extend_from_slice(&[0xaa; 8]);
        s.extend_from_slice(&(RECORD_SIZE as u32).to_le_bytes());
        s.extend_from_slice(&0u16.to_le_bytes());
        s.extend_from_slice(&(SCHEMA.len() as u16).to_le_bytes());
        for (h, off, ty) in SCHEMA {
            s.extend_from_slice(&h.to_le_bytes());
            s.extend_from_slice(&off.to_le_bytes());
            s.extend_from_slice(&[ty, if ty == 79 { 15 } else { 0 }, 0, 0]);
            s.extend_from_slice(&0u32.to_le_bytes());
        }
        out.extend_from_slice(b"LSCH");
        out.extend_from_slice(&((s.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(&s);
        out
    }

    #[test]
    fn field_names_are_x_and_y() {
        assert_eq!(crate::meta7a::joaat("x"), F_X);
        assert_eq!(crate::meta7a::joaat("y"), F_Y);
    }

    #[test]
    fn decodes_records() {
        let d = build(&[(0, 0, 0x7a53_e373, 0), (0, 1, 0x7a53_e373, 1), (-200, 172, 7, 0)]);
        let g = decode(&d).unwrap();
        assert_eq!(g.sectors.len(), 3);
        assert_eq!(g.sectors[1], Sector { x: 0, y: 1, zero: 0, group: 0x7a53_e373, index: 1, hash64: 0 });
        assert_eq!(g.sectors[2].x, -200);
        assert_eq!(g.cell_range(), Some([-200, 0, 0, 172]));
        assert_eq!(g.world_bounds(), Some(([-12000.0, 0.0], [60.0, 10380.0])));
        assert_eq!(g.groups(), 2);
        assert_eq!(g.sectors[2].bounds(g.cell_m), ([-12000.0, 10320.0], [-11940.0, 10380.0]));
        assert_eq!(g.kind(), Kind::Cells);
        let csv = to_csv(&g);
        assert_eq!(csv.lines().nth(2), Some("0,1,0,60,60,120,7a53e373,1,0,0000000000000000"));
    }

    #[test]
    fn reads_hash64() {
        let mut d = build(&[(5, -3, 9, 0)]);
        let id = crate::meta7a::asset_id("some_model");
        let rec = d.windows(4).position(|w| w == 5i32.to_le_bytes()).unwrap();
        d[rec + 24..rec + 32].copy_from_slice(&id.to_le_bytes());
        let g = decode(&d).unwrap();
        assert_eq!(g.sectors[0].hash64, id);
        assert_eq!(g.kind(), Kind::MapGroups);
        assert_eq!(g.map_groups(), 1);
        assert_eq!(g.cells_of(id), vec![(5, -3)]);
        assert_eq!((g.sectors[0].x, g.sectors[0].y), (5, -3));
    }

    #[test]
    fn mask_is_north_up() {
        let g = decode(&build(&[(0, 0, 1, 0), (2, 1, 1, 1)])).unwrap();
        let (w, h, px) = mask_north_up(&g).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(px, vec![0, 0, 255, 255, 0, 0]);
    }

    #[test]
    fn cell_of_rounds_down() {
        assert_eq!(cell_of(0.0, 59.9, 60.0), (0, 0));
        assert_eq!(cell_of(-0.1, 60.0, 60.0), (-1, 1));
        assert_eq!(cell_of(-12000.0, 10379.0, 60.0), (-200, 172));
        assert_eq!(cell_of(-1.0, 1023.0, 1024.0), (-1, 0));
    }

    #[test]
    fn rejects_other_schema() {
        let mut d = build(&[(1, 2, 3, 0)]);
        // Move field x to offset 4: the decoder must refuse it.
        let lsch = d.windows(4).position(|w| w == b"LSCH").unwrap();
        let first_member = lsch + 8 + 32;
        d[first_member + 4..first_member + 8].copy_from_slice(&4u32.to_le_bytes());
        assert!(decode(&d).is_err());
    }
}
