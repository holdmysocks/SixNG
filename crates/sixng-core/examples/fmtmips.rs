//! Research: census of texture formats in the texture dictionaries of some
//! archives, and for the formats asked for, mip n+1 against mip n
//! downsampled (2x2 box, RGBA8 after decoding), as PSNR in dB.
//! fmtmips <game root> <dxgi,dxgi,...> <index json>...
use rayon::prelude::*;
use sixng_core::{resource::Resource, scan, texture as tx};
use std::collections::BTreeMap;
use std::path::Path;

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let se: f64 = a.iter().zip(b).map(|(&x, &y)| (x as f64 - y as f64).powi(2)).sum();
    let mse = se / a.len().max(1) as f64;
    if mse == 0.0 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

fn down(p: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut o = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let mut s = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                    s += p[((sy * w + sx) * 4 + c) as usize] as u32;
                }
                o[((y * nw + x) * 4 + c) as usize] = ((s + 2) / 4) as u8;
            }
        }
    }
    o
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let root = Path::new(&a[1]);
    let want: Vec<u32> = a[2].split(',').filter_map(|s| s.parse().ok()).collect();
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown: BTreeMap<String, usize> = BTreeMap::new();
    for ix_path in &a[3..] {
        let ix = scan::load_index(Path::new(ix_path))?;
        let arch = root.join(&ix.archive);
        let txds: Vec<_> = ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("txd")).collect();
        let res: Vec<(Vec<String>, Vec<String>, Vec<String>)> = txds
            .par_iter()
            .filter_map(|e| {
                let r = Resource::load(&arch, e).ok()?;
                let (mut fmts, mut unk, mut lines) = (vec![], vec![], vec![]);
                for t in tx::find_textures(&r) {
                    fmts.push(t.format.clone());
                    let fmt = tx::Fmt::from_dxgi(t.dxgi_format);
                    if fmt == tx::Fmt::Unknown {
                        unk.push(t.format.clone());
                    }
                    if !want.contains(&t.dxgi_format) {
                        continue;
                    }
                    let n = tx::exportable_mips(&t);
                    let mut ds = vec![];
                    for m in 0..n.saturating_sub(1) {
                        let (Ok((f0, l0, w0, h0)), Ok((f1, l1, w1, h1))) = (tx::mip_linear(&r, &t, m), tx::mip_linear(&r, &t, m + 1)) else { break };
                        let (Ok(p0), Ok(p1)) = (tx::to_rgba(f0, &l0, w0, h0), tx::to_rgba(f1, &l1, w1, h1)) else { break };
                        ds.push(format!("m{}-{}: {:.1}", m, m + 1, psnr(&down(&p0, w0, h0), &p1)));
                    }
                    // Whole chain in the 4 KiB mip-tail block: untile each mip from it.
                    let bpe = fmt.bytes_per_elem();
                    if let Some(l) = sixng_core::gfx10::Layout::new(sixng_core::gfx10::Swizzle::S4K, t.width, t.height, t.mips, bpe).filter(|l| t.mips > 1 && l.mips[0].tail.is_some()) {
                        let s = t.data_off as usize;
                        if let Some(block) = r.gfx.get(s..s + l.block_bytes as usize) {
                            let px: Vec<_> = (0..t.mips as usize).map(|m| (tx::to_rgba(fmt, &l.untile(block, m), l.mips[m].w, l.mips[m].h).unwrap(), l.mips[m].w, l.mips[m].h)).collect();
                            for m in 0..px.len() - 1 {
                                if px[m].1 >= 4 {
                                    ds.push(format!("tail m{}-{}: {:.1}", m, m + 1, psnr(&down(&px[m].0, px[m].1, px[m].2), &px[m + 1].0)));
                                }
                            }
                        }
                    }
                    // Neighbour smoothness: untiled mip 0 against the same bytes read as linear.
                    let mut smooth = String::new();
                    if let Ok((f, lin)) = tx::mip0_linear(&r, &t, false) {
                        let raw = tx::mip0_linear(&r, &t, true).map(|x| x.1).unwrap_or_default();
                        let grad = |d: &[u8]| -> f64 {
                            let Ok(p) = tx::to_rgba(f, d, t.width, t.height) else { return -1.0 };
                            let w = t.width as usize;
                            let (mut s, mut n) = (0f64, 0f64);
                            for i in 0..p.len() / 4 {
                                if (i + 1) % w != 0 {
                                    s += (0..4).map(|c| (p[i * 4 + c] as f64 - p[(i + 1) * 4 + c] as f64).abs()).sum::<f64>();
                                    n += 1.0;
                                }
                                if i + w < p.len() / 4 {
                                    s += (0..4).map(|c| (p[i * 4 + c] as f64 - p[(i + w) * 4 + c] as f64).abs()).sum::<f64>();
                                    n += 1.0;
                                }
                            }
                            s / n.max(1.0)
                        };
                        smooth = format!("neighbour diff untiled {:.2} vs linear {:.2}", grad(&lin), grad(&raw));
                        // Optional: FMTMIPS_PNG=<dir> writes mip 0 of each as PNG.
                        if let (Ok(dir), Ok(p)) = (std::env::var("FMTMIPS_PNG"), tx::to_rgba(f, &lin, t.width, t.height)) {
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = tx::save_png(&Path::new(&dir).join(format!("{}_{}.png", t.name, t.format)), &p, t.width, t.height);
                        }
                    }
                    lines.push(format!("{}\t{}\t{}\t{}x{} mips {} (exportable {})\t{}\t{}", e.id, t.name, t.format, t.width, t.height, t.mips, n, smooth, ds.join(", ")));
                }
                Some((fmts, unk, lines))
            })
            .collect();
        for (f, u, l) in res {
            f.into_iter().for_each(|x| *census.entry(x).or_default() += 1);
            u.into_iter().for_each(|x| *unknown.entry(x).or_default() += 1);
            l.iter().for_each(|x| println!("{x}"));
        }
    }
    // The bulk stores: same check per tile mode.
    let set = sixng_core::ttpf::global(root);
    for s in &set.stores {
        for t in s.textures.iter().filter(|t| want.contains(&t.dxgi_format)) {
            let mut ds = vec![];
            if let Ok(data) = s.data(t) {
                let l = t.layout().map(|l| l.mips.len() as u32).unwrap_or(0);
                for m in 0..l.saturating_sub(1) {
                    let (Ok((p0, w0, h0)), Ok((p1, _, _))) = (sixng_core::ttpf::decode_rgba(t, &data, m), sixng_core::ttpf::decode_rgba(t, &data, m + 1)) else { break };
                    if w0 < 8 || h0 < 8 {
                        break;
                    }
                    ds.push(format!("m{}-{}: {:.1}", m, m + 1, psnr(&down(&p0, w0, h0), &p1)));
                }
            }
            println!("{}\t{:016x}\t{}\t{}x{} tile {} mips {} decodable {}\t{}", s.label(), t.hash, tx::dxgi_name(t.dxgi_format), t.width, t.height, t.tile_mode(), t.mips(), t.decodable(), ds.join(", "));
        }
    }
    println!("census: {census:?}");
    println!("still unknown: {unknown:?}");
    Ok(())
}
