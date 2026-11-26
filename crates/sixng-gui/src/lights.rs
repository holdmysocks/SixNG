//! The map's placed lights for the HDR shading (Map 3D).
//!
//! Each frame the lights near the camera are packed into two storage
//! buffers of the mesh shaders' group 0 (bindings 22 and 23):
//!
//! * `lights`: per light `pos_range` (world position, range in m) and
//!   `col_fe` (linear colour times intensity, falloff exponent);
//! * `grid`: a square grid of `N` x `N` cells of `CELL` m centred on the
//!   camera. Its header holds the grid's world origin (x, z) and cell size
//!   as f32 bits and `N`; then per cell the first index and the count into
//!   the index list that follows. A cell lists the lights whose range
//!   reaches it, at most `PER_CELL`, brightest first.
//!
//! Far lights only glow (the coronas, [`Glow`]): they are drawn as small
//! additive sprites to a few kilometres.

use glam::Vec3;

/// Grid cells per side.
pub const N: usize = 64;
/// Grid cell size (m): the grid covers N * CELL = 768 m around the camera.
pub const CELL: f32 = 12.0;
/// Most lights listed in one cell.
pub const PER_CELL: usize = 32;
/// Most lights shaded per frame.
pub const MAX_LIGHTS: usize = 16384;
/// Most cell entries per frame.
pub const MAX_INDICES: usize = 262_144;
/// Header words of the grid buffer.
pub const HEADER: usize = 4;
/// Grid buffer size in u32 words.
pub const GRID_WORDS: usize = HEADER + N * N * 2 + MAX_INDICES;

/// One shaded light (shader `Light`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// world position (m), range (m)
    pub pos_range: [f32; 4],
    /// linear colour times intensity, falloff exponent
    pub col_fe: [f32; 4],
}

/// One corona sprite (shader `GlowIn`): an instance of the glow pipeline.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Glow {
    /// world position (m), intensity scale
    pub pos: [f32; 4],
    /// linear colour (times intensity), unused
    pub col: [f32; 4],
}

/// The lights to shade around `centre` (world, m) packed as the two
/// buffers' contents. `lights` are the candidates (any order); those whose
/// range does not reach the grid are left out.
pub fn pack(centre: Vec3, lights: &[GpuLight]) -> (Vec<GpuLight>, Vec<u32>) {
    let half = N as f32 * CELL * 0.5;
    let ox = (centre.x / CELL).floor() * CELL - half;
    let oz = (centre.z / CELL).floor() * CELL - half;
    let reach = |l: &GpuLight| {
        let (x, z, r) = (l.pos_range[0], l.pos_range[2], l.pos_range[3]);
        x + r >= ox && x - r <= ox + N as f32 * CELL && z + r >= oz && z - r <= oz + N as f32 * CELL
    };
    let lum = |l: &GpuLight| l.col_fe[0] * 0.2126 + l.col_fe[1] * 0.7152 + l.col_fe[2] * 0.0722;
    let mut kept: Vec<GpuLight> = lights.iter().filter(|l| reach(l) && lum(l) > 0.0).copied().collect();
    // brightest first, so a full cell keeps the strongest lights
    kept.sort_by(|a, b| (lum(b) * b.pos_range[3]).total_cmp(&(lum(a) * a.pos_range[3])));
    kept.truncate(MAX_LIGHTS);
    let mut cells: Vec<Vec<u32>> = vec![Vec::new(); N * N];
    for (i, l) in kept.iter().enumerate() {
        let (x, z, r) = (l.pos_range[0], l.pos_range[2], l.pos_range[3]);
        let c0 = (((x - r - ox) / CELL).floor().max(0.0) as usize).min(N - 1);
        let c1 = (((x + r - ox) / CELL).floor().max(0.0) as usize).min(N - 1);
        let r0 = (((z - r - oz) / CELL).floor().max(0.0) as usize).min(N - 1);
        let r1 = (((z + r - oz) / CELL).floor().max(0.0) as usize).min(N - 1);
        for row in r0..=r1 {
            for col in c0..=c1 {
                // the light's sphere against the cell's square
                let cx = (x - ox).clamp(col as f32 * CELL, (col + 1) as f32 * CELL);
                let cz = (z - oz).clamp(row as f32 * CELL, (row + 1) as f32 * CELL);
                let (dx, dz) = (x - ox - cx, z - oz - cz);
                let cell = &mut cells[row * N + col];
                if dx * dx + dz * dz <= r * r && cell.len() < PER_CELL {
                    cell.push(i as u32);
                }
            }
        }
    }
    let mut grid = Vec::with_capacity(HEADER + N * N * 2 + kept.len() * 2);
    grid.extend([ox.to_bits(), oz.to_bits(), CELL.to_bits(), N as u32]);
    let mut list = Vec::new();
    for c in &cells {
        let take = c.len().min(MAX_INDICES.saturating_sub(list.len()));
        grid.extend([list.len() as u32, take as u32]);
        list.extend_from_slice(&c[..take]);
    }
    grid.extend(list);
    (kept, grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(x: f32, z: f32, r: f32, i: f32) -> GpuLight {
        GpuLight { pos_range: [x, 5.0, z, r], col_fe: [i, i, i, 8.0] }
    }

    #[test]
    fn grid_lists_lights_in_reach() {
        let (kept, grid) = pack(Vec3::ZERO, &[light(1.0, 1.0, 5.0, 10.0), light(5000.0, 0.0, 5.0, 10.0), light(-30.0, 2.0, 20.0, 1.0)]);
        assert_eq!(kept.len(), 2, "the far light is left out");
        let ox = f32::from_bits(grid[0]);
        let oz = f32::from_bits(grid[1]);
        assert_eq!(grid[3] as usize, N);
        let cell_of = |x: f32, z: f32| (((z - oz) / CELL) as usize) * N + ((x - ox) / CELL) as usize;
        let list = |c: usize| {
            let (o, n) = (grid[HEADER + c * 2] as usize, grid[HEADER + c * 2 + 1] as usize);
            grid[HEADER + N * N * 2 + o..HEADER + N * N * 2 + o + n].to_vec()
        };
        // the bright light first; both reach the origin's cell
        let at0 = list(cell_of(0.5, 0.5));
        assert_eq!(at0.len(), 2);
        assert_eq!(kept[at0[0] as usize].col_fe[0], 10.0);
        // 100 m away nothing reaches
        assert!(list(cell_of(100.0, 100.0)).is_empty());
    }

    #[test]
    fn cells_cap_their_lists() {
        let many: Vec<GpuLight> = (0..100).map(|i| light(0.0, 0.0, 3.0, 1.0 + i as f32)).collect();
        let (_, grid) = pack(Vec3::ZERO, &many);
        let most = (0..N * N).map(|c| grid[HEADER + c * 2 + 1] as usize).max().unwrap();
        assert_eq!(most, PER_CELL);
    }
}
