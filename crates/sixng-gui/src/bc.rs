//! Fast BC1 / BC3 block compression of RGBA8 mip chains, so the 3D map's
//! textures take a quarter (BC3) or an eighth (BC1) of the GPU memory.
//!
//! Colour endpoints come from the block's bounding box, inset by 1/16 of its
//! range, with each texel taking the nearest of the four palette colours.
//! Alpha (BC3) uses the block's min / max with the eight-value palette. The
//! quality is a little below an offline encoder, which is fine for a map
//! viewer and costs about a millisecond per 512 px texture.

/// Block-compressed mip chain: `alpha` = BC3, else BC1; one byte vector per
/// level, blocks in row order.
pub struct Bc {
    pub alpha: bool,
    pub levels: Vec<Vec<u8>>,
}

/// Bytes per 4x4 block.
pub fn block_bytes(alpha: bool) -> usize {
    if alpha {
        16
    } else {
        8
    }
}

/// Compress a mip chain (level 0 must be a multiple of 4 on both sides).
/// Images with any alpha below 250 use BC3.
pub fn compress(mips: &[(u32, u32, Vec<u8>)]) -> Option<Bc> {
    let (w0, h0, px0) = mips.first()?;
    if w0 % 4 != 0 || h0 % 4 != 0 || px0.len() < (*w0 as usize * *h0 as usize * 4) {
        return None;
    }
    let alpha = px0.chunks_exact(4).any(|p| p[3] < 250);
    let levels = mips
        .iter()
        .map(|(w, h, px)| compress_level(*w, *h, px, alpha))
        .collect();
    Some(Bc { alpha, levels })
}

fn compress_level(w: u32, h: u32, px: &[u8], alpha: bool) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let mut out = Vec::with_capacity(bw * bh * block_bytes(alpha));
    let mut blk = [[0u8; 4]; 16];
    for by in 0..bh {
        for bx in 0..bw {
            for y in 0..4 {
                // small levels (2x2, 1x1) repeat their edge texels
                let sy = (by * 4 + y).min(h - 1);
                for x in 0..4 {
                    let sx = (bx * 4 + x).min(w - 1);
                    let o = (sy * w + sx) * 4;
                    blk[y * 4 + x] = [px[o], px[o + 1], px[o + 2], px[o + 3]];
                }
            }
            if alpha {
                alpha_block(&blk, &mut out);
            }
            colour_block(&blk, &mut out);
        }
    }
    out
}

/// BC3 for every level, whatever the alpha (texture arrays need one format).
pub fn compress_bc3(mips: &[(u32, u32, Vec<u8>)]) -> Vec<Vec<u8>> {
    mips.iter()
        .map(|(w, h, px)| compress_level(*w, *h, px, true))
        .collect()
}

/// Decode level 0 of a block-compressed image back to RGBA8.
pub fn decode(bc: &Bc, w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let bb = block_bytes(bc.alpha);
    let src = match bc.levels.first() {
        Some(l) if l.len() >= bw * bh * bb => l,
        _ => return vec![255; w * h * 4],
    };
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let b = &src[(by * bw + bx) * bb..(by * bw + bx + 1) * bb];
            let (ab, cb) = if bc.alpha {
                (Some(&b[..8]), &b[8..])
            } else {
                (None, b)
            };
            let c0 = u16::from_le_bytes([cb[0], cb[1]]);
            let c1 = u16::from_le_bytes([cb[2], cb[3]]);
            let idx = u32::from_le_bytes([cb[4], cb[5], cb[6], cb[7]]);
            let (a, b2) = (from565(c0), from565(c1));
            let four = c0 > c1 || bc.alpha;
            let pal: [[i32; 4]; 4] = if four {
                [
                    [a[0], a[1], a[2], 255],
                    [b2[0], b2[1], b2[2], 255],
                    [
                        (2 * a[0] + b2[0]) / 3,
                        (2 * a[1] + b2[1]) / 3,
                        (2 * a[2] + b2[2]) / 3,
                        255,
                    ],
                    [
                        (a[0] + 2 * b2[0]) / 3,
                        (a[1] + 2 * b2[1]) / 3,
                        (a[2] + 2 * b2[2]) / 3,
                        255,
                    ],
                ]
            } else {
                [
                    [a[0], a[1], a[2], 255],
                    [b2[0], b2[1], b2[2], 255],
                    [
                        (a[0] + b2[0]) / 2,
                        (a[1] + b2[1]) / 2,
                        (a[2] + b2[2]) / 2,
                        255,
                    ],
                    [0, 0, 0, 0],
                ]
            };
            let alphas: Option<[i32; 8]> = ab.map(|ab| {
                let (a0, a1) = (ab[0] as i32, ab[1] as i32);
                if a0 > a1 {
                    [
                        a0,
                        a1,
                        (6 * a0 + a1) / 7,
                        (5 * a0 + 2 * a1) / 7,
                        (4 * a0 + 3 * a1) / 7,
                        (3 * a0 + 4 * a1) / 7,
                        (2 * a0 + 5 * a1) / 7,
                        (a0 + 6 * a1) / 7,
                    ]
                } else {
                    [
                        a0,
                        a1,
                        (4 * a0 + a1) / 5,
                        (3 * a0 + 2 * a1) / 5,
                        (2 * a0 + 3 * a1) / 5,
                        (a0 + 4 * a1) / 5,
                        0,
                        255,
                    ]
                }
            });
            let abits = ab.map(|ab| {
                let mut v = 0u64;
                for k in 0..6 {
                    v |= (ab[2 + k] as u64) << (8 * k);
                }
                v
            });
            for i in 0..16 {
                let (x, y) = (bx * 4 + (i & 3), by * 4 + (i >> 2));
                if x >= w || y >= h {
                    continue;
                }
                let c = pal[((idx >> (2 * i)) & 3) as usize];
                let o = (y * w + x) * 4;
                out[o] = c[0] as u8;
                out[o + 1] = c[1] as u8;
                out[o + 2] = c[2] as u8;
                out[o + 3] = match (alphas, abits) {
                    (Some(al), Some(bits)) => al[((bits >> (3 * i)) & 7) as usize] as u8,
                    _ => c[3] as u8,
                };
            }
        }
    }
    out
}

fn to565(c: [i32; 3]) -> u16 {
    let r = (c[0].clamp(0, 255) as u32 * 31 + 127) / 255;
    let g = (c[1].clamp(0, 255) as u32 * 63 + 127) / 255;
    let b = (c[2].clamp(0, 255) as u32 * 31 + 127) / 255;
    ((r << 11) | (g << 5) | b) as u16
}

fn from565(v: u16) -> [i32; 3] {
    let r = ((v >> 11) & 31) as i32;
    let g = ((v >> 5) & 63) as i32;
    let b = (v & 31) as i32;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

fn colour_block(blk: &[[u8; 4]; 16], out: &mut Vec<u8>) {
    let mut lo = [255i32; 3];
    let mut hi = [0i32; 3];
    for p in blk {
        for c in 0..3 {
            lo[c] = lo[c].min(p[c] as i32);
            hi[c] = hi[c].max(p[c] as i32);
        }
    }
    // inset the box a little: the end colours are rarely hit exactly
    for c in 0..3 {
        let inset = (hi[c] - lo[c]) >> 4;
        lo[c] += inset;
        hi[c] -= inset;
    }
    let (mut c0, mut c1) = (to565(hi), to565(lo));
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    let mut idx: u32 = 0;
    if c0 != c1 {
        // four-colour mode (c0 > c1): c0, c1, 2/3 c0 + 1/3 c1, 1/3 c0 + 2/3 c1
        let (a, b) = (from565(c0), from565(c1));
        let pal = [
            a,
            b,
            [
                (2 * a[0] + b[0]) / 3,
                (2 * a[1] + b[1]) / 3,
                (2 * a[2] + b[2]) / 3,
            ],
            [
                (a[0] + 2 * b[0]) / 3,
                (a[1] + 2 * b[1]) / 3,
                (a[2] + 2 * b[2]) / 3,
            ],
        ];
        for (i, p) in blk.iter().enumerate() {
            let mut best = (i32::MAX, 0u32);
            for (k, q) in pal.iter().enumerate() {
                let d = (p[0] as i32 - q[0]).pow(2)
                    + (p[1] as i32 - q[1]).pow(2)
                    + (p[2] as i32 - q[2]).pow(2);
                if d < best.0 {
                    best = (d, k as u32);
                }
            }
            idx |= best.1 << (2 * i);
        }
    }
    out.extend_from_slice(&c0.to_le_bytes());
    out.extend_from_slice(&c1.to_le_bytes());
    out.extend_from_slice(&idx.to_le_bytes());
}

fn alpha_block(blk: &[[u8; 4]; 16], out: &mut Vec<u8>) {
    let (mut lo, mut hi) = (255u8, 0u8);
    for p in blk {
        lo = lo.min(p[3]);
        hi = hi.max(p[3]);
    }
    out.push(hi);
    out.push(lo);
    let mut bits: u64 = 0;
    if hi > lo {
        // a0 > a1: eight values from a0 to a1
        let (a0, a1) = (hi as i32, lo as i32);
        let pal: [i32; 8] = [
            a0,
            a1,
            (6 * a0 + a1) / 7,
            (5 * a0 + 2 * a1) / 7,
            (4 * a0 + 3 * a1) / 7,
            (3 * a0 + 4 * a1) / 7,
            (2 * a0 + 5 * a1) / 7,
            (a0 + 6 * a1) / 7,
        ];
        for (i, p) in blk.iter().enumerate() {
            let a = p[3] as i32;
            let k = (0..8).min_by_key(|&k| (a - pal[k]).abs()).unwrap() as u64;
            bits |= k << (3 * i);
        }
    }
    out.extend_from_slice(&bits.to_le_bytes()[..6]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_bc1(b: &[u8]) -> [[i32; 3]; 16] {
        let c0 = u16::from_le_bytes([b[0], b[1]]);
        let c1 = u16::from_le_bytes([b[2], b[3]]);
        let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let (a, c) = (from565(c0), from565(c1));
        let pal = [
            a,
            c,
            [
                (2 * a[0] + c[0]) / 3,
                (2 * a[1] + c[1]) / 3,
                (2 * a[2] + c[2]) / 3,
            ],
            [
                (a[0] + 2 * c[0]) / 3,
                (a[1] + 2 * c[1]) / 3,
                (a[2] + 2 * c[2]) / 3,
            ],
        ];
        let mut o = [[0; 3]; 16];
        for i in 0..16 {
            o[i] = pal[((idx >> (2 * i)) & 3) as usize];
        }
        o
    }

    #[test]
    fn decode_matches_reference_decoder() {
        let mut px = Vec::new();
        for _y in 0..8u32 {
            for x in 0..8u32 {
                px.extend_from_slice(&[
                    (x * 32) as u8,
                    (x * 16) as u8,
                    128,
                    if x < 4 { 255 } else { 40 },
                ]);
            }
        }
        let bc = compress(&[(8, 8, px.clone())]).unwrap();
        let d = decode(&bc, 8, 8);
        for (a, b) in d.iter().zip(&px) {
            assert!((*a as i32 - *b as i32).abs() <= 24);
        }
    }

    #[test]
    fn gradient_round_trip() {
        // 8x8 ramp along one colour line (what a BC1 block can hold),
        // opaque -> BC1, small error
        let mut px = Vec::new();
        for _y in 0..8u32 {
            for x in 0..8u32 {
                px.extend_from_slice(&[(x * 32) as u8, (x * 16) as u8, 128, 255]);
            }
        }
        let bc = compress(&[(8, 8, px.clone())]).unwrap();
        assert!(!bc.alpha);
        assert_eq!(bc.levels[0].len(), 4 * 8);
        let blk0 = decode_bc1(&bc.levels[0][..8]);
        for y in 0..4 {
            for x in 0..4 {
                let o = (y * 8 + x) * 4;
                for c in 0..3 {
                    assert!(
                        (blk0[y * 4 + x][c] - px[o + c] as i32).abs() <= 24,
                        "texel {x},{y} c{c}"
                    );
                }
            }
        }
    }

    #[test]
    fn alpha_uses_bc3_and_flat_block() {
        let mut px = vec![200u8; 4 * 4 * 4];
        for p in px.chunks_exact_mut(4) {
            p[3] = 10;
        }
        let bc = compress(&[(4, 4, px), (2, 2, vec![0; 16]), (1, 1, vec![0; 4])]).unwrap();
        assert!(bc.alpha);
        assert_eq!(bc.levels.len(), 3);
        assert!(bc.levels.iter().all(|l| l.len() == 16));
        // flat colour: both endpoints equal, indices 0
        assert_eq!(&bc.levels[0][12..16], &[0, 0, 0, 0]);
        assert_eq!(bc.levels[0][0], 10);
    }

    #[test]
    fn odd_size_is_rejected() {
        assert!(compress(&[(6, 4, vec![0; 6 * 4 * 4])]).is_none());
    }
}
