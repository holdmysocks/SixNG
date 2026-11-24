//! Research: how a scene's base clip dictionary (`cin@<scene>`) points at
//! the animations streamed in its `cin@<scene>-<N>` section dictionaries.
//! Usage: sectprobe <archive path> <index json> <scene> [<clip filter>]
//! Prints the clips of the base dictionary with the raw record behind +0x68,
//! then every section dictionary with its animations.
use rayon::prelude::*;
use sixng_core::{anim, resource::Resource, scan};

fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn ptr(b: &[u8], o: usize) -> Option<usize> {
    let v = u64_at(b, o);
    let off = (v & 0x0fff_ffff) as usize;
    (v >> 28 == 5 && off < b.len()).then_some(off)
}
fn cstr(b: &[u8], o: usize) -> Option<String> {
    let e = b.get(o..)?.iter().position(|&c| c == 0)?;
    let s = &b[o..o + e];
    (!s.is_empty() && s.iter().all(|c| c.is_ascii_graphic() || *c == b' ')).then(|| String::from_utf8_lossy(s).into_owned())
}
fn hex(b: &[u8], o: usize, n: usize) -> String {
    b.get(o..(o + n).min(b.len())).unwrap_or(&[]).chunks(8).map(|c| c.iter().map(|x| format!("{x:02x}")).collect::<String>()).collect::<Vec<_>>().join(" ")
}
/// Pointers in [o, o+n) that lead to strings.
fn strs(b: &[u8], o: usize, n: usize) -> Vec<(usize, String)> {
    (0..n).step_by(8).filter_map(|k| ptr(b, o + k).and_then(|p| cstr(b, p)).map(|s| (k, s))).collect()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let archive = std::path::PathBuf::from(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    let scene = a[3].to_ascii_lowercase();
    let filt = a.get(4).map(|s| s.to_ascii_lowercase());
    let base_name = format!("cin@{scene}");
    let sect_prefix = format!("cin@{scene}-");
    let found: Vec<(usize, String)> = ix
        .entries
        .par_iter()
        .enumerate()
        .filter(|(_, e)| e.kind == sixng_core::classify::Kind::RageObj && e.rtype.is_none() && e.name.is_none() && !e.streams.is_empty())
        .filter_map(|(i, e)| {
            let r = Resource::load_sys(&archive, e).ok()?;
            let n = anim::dictionary_name(&r.sys)?;
            (n.eq_ignore_ascii_case(&base_name) || n.to_ascii_lowercase().starts_with(&sect_prefix)).then_some((i, n))
        })
        .collect();
    let mut found = found;
    found.sort_by(|x, y| x.1.cmp(&y.1));
    for (i, n) in &found {
        println!("dict {n} entry {} (index {i})", ix.entries[*i].id);
    }
    let mode_raw = std::env::var("RAW").is_ok();
    if !mode_raw {
        let parse = |i: usize| anim::ClipDictionary::parse(&anim::dictionary_bytes(&archive, &ix.entries[i]).unwrap()).unwrap();
        let (bi, _) = found.iter().find(|(_, n)| n.eq_ignore_ascii_case(&base_name)).unwrap();
        let mut base = parse(*bi);
        let secs: Vec<anim::ClipDictionary> = found.iter().filter(|(i, _)| i != bi).map(|(i, _)| parse(*i)).collect();
        for s in &secs {
            for a in &s.animations {
                println!("section {} anim {:08x} {} frames={} dur={:.3} tracks={}", s.name, a.hash, a.name, a.frames, a.duration, a.tracks.len());
            }
        }
        let before: usize = base.clips.iter().map(|c| c.pending.len()).sum();
        let joined = base.join(secs);
        println!("pending before {before}, joined {joined}");
        for c in &base.clips {
            let tr = anim::clip_tracks(&base, c);
            let last = tr.iter().filter_map(|t| t.times.last()).fold(0f32, |a, &b| a.max(b));
            let keys = tr.iter().map(|t| t.times.len()).max().unwrap_or(0);
            let mv = anim::clip_mover(&base, c);
            println!("clip {} dur={:.3} sections={} bindings={} pending={} tracks={} keys={} last={:.3} mover_keys={}", c.short_name(), c.duration, c.sections, c.bindings.len(), c.pending.len(), tr.len(), keys, last, mv.first().map_or(0, |m| m.times.len()));
            if filt.as_ref().is_some_and(|f| c.short_name().contains(f.as_str())) {
                for b in &c.bindings {
                    println!("   bind off={:.3} [{:.3},{:.3}] rate={} anim={}", b.offset, b.start, b.end, b.rate, base.animations[b.animation].name);
                }
                // continuity: the largest per-key change of every track, inside sections and across section starts
                let starts: Vec<f32> = c.bindings.iter().map(|b| b.offset).filter(|&o| o > 0.0).collect();
                let (mut inside, mut across) = (Vec::new(), Vec::new());
                for t in tr.iter().chain(&mv) {
                    for k in 1..t.times.len() {
                        let d: f32 = (0..4).map(|i| (t.values[k][i] - t.values[k - 1][i]).abs()).fold(0.0, f32::max);
                        if starts.iter().any(|&o| (t.times[k] - o).abs() < 0.02) { across.push(d) } else { inside.push(d) }
                    }
                }
                let pct = |v: &mut Vec<f32>, p: f32| { v.sort_by(f32::total_cmp); v.get(((v.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0) };
                // per section start: mover translation jump (m) and the largest bone rotation change (sign-aware), vs the step just before
                for &o in &starts {
                    let jump = |t: &anim::ClipTrack, quat: bool| -> (f32, f32) {
                        let k = t.times.iter().position(|&x| (x - o).abs() < 0.02 && x >= o - 0.001).unwrap_or(0);
                        if k < 2 { return (0.0, 0.0) }
                        let d = |a: [f32; 4], b: [f32; 4]| if quat { 1.0 - (a[0]*b[0]+a[1]*b[1]+a[2]*b[2]+a[3]*b[3]).abs() } else { ((a[0]-b[0]).powi(2)+(a[1]-b[1]).powi(2)+(a[2]-b[2]).powi(2)).sqrt() };
                        (d(t.values[k], t.values[k-1]), d(t.values[k-1], t.values[k-2]))
                    };
                    let mvj = mv.iter().find(|m| m.kind == anim::track::MOVER_TRANSLATION).map(|m| jump(m, false)).unwrap_or_default();
                    let rj = tr.iter().filter(|t| t.kind == anim::track::ROTATION).map(|t| jump(t, true)).fold((0f32, 0f32), |a, b| (a.0.max(b.0), a.1.max(b.1)));
                    println!("   start {o:7.3}: mover {:.4} m (before {:.4}); max bone rot 1-|dot| {:.5} (before {:.5})", mvj.0, mvj.1, rj.0, rj.1);
                }
                println!("   step inside p50={:.4} p99={:.4} max={:.4} | across starts p50={:.4} p99={:.4} max={:.4} (n={})", pct(&mut inside, 0.5), pct(&mut inside, 0.99), pct(&mut inside, 1.0), pct(&mut across, 0.5), pct(&mut across, 0.99), pct(&mut across, 1.0), across.len());
            }
        }
        return Ok(());
    }
    for (i, n) in &found {
        let e = &ix.entries[*i];
        let b = anim::dictionary_bytes(&archive, e)?;
        let d = anim::ClipDictionary::parse(&b)?;
        println!("\n== {n}: {} clips, {} animations, {} bytes", d.clips.len(), d.animations.len(), b.len());
        for an in &d.animations {
            let bones: std::collections::BTreeSet<u32> = an.tracks.iter().map(|t| t.bone).collect();
            println!("  anim {:08x} {} frames={} dur={:.3} tracks={} bones={}", an.hash, an.name, an.frames, an.duration, an.tracks.len(), bones.len());
        }
        if !n.eq_ignore_ascii_case(&base_name) {
            println!("  head {}", hex(&b, 0, 0x90));
            for (o, nm) in [(0x48usize, "clipmap"), (0x70, "animdicts")] {
                println!("  {nm} @{o:#x}: {}", hex(&b, o, 0x10));
            }
            let buckets = ptr(&b, 0x48).unwrap();
            let nb = u16::from_le_bytes([b[0x50], b[0x51]]) as usize;
            for k in 0..nb {
                let mut node = ptr(&b, buckets + 8 * k);
                while let Some(nd) = node {
                    println!("  node {}", hex(&b, nd, 24));
                    if let Some(v) = ptr(&b, nd + 8) {
                        println!("   val@{v:#x} {}  strs {:?}", hex(&b, v, 0x70), strs(&b, v, 0x70));
                    }
                    node = ptr(&b, nd + 16);
                }
            }
            continue;
        }
        // raw clip records of the base dictionary
        let buckets = ptr(&b, 0x48).unwrap();
        let nb = u16::from_le_bytes([b[0x50], b[0x51]]) as usize;
        for k in 0..nb {
            let mut node = ptr(&b, buckets + 8 * k);
            while let Some(nd) = node {
                if let Some(co) = ptr(&b, nd + 8) {
                    let name = ptr(&b, co + 0x28).and_then(|p| cstr(&b, p)).unwrap_or_default();
                    if filt.as_ref().is_none_or(|f| name.to_ascii_lowercase().contains(f)) {
                        println!("\n clip {name} kind={} raw68={:#018x}", u32::from_le_bytes(b[co + 0x10..co + 0x14].try_into().unwrap()), u64_at(&b, co + 0x68));
                        println!("  rec  {}", hex(&b, co, 0xa0));
                        println!("  strs {:?}", strs(&b, co, 0xa0));
                        if let Some(h) = ptr(&b, co + 0x68) {
                            println!("  @68  {}", hex(&b, h, 0x40));
                            if let Some(arr) = ptr(&b, h) {
                                let cnt = u16::from_le_bytes([b[h + 8], b[h + 9]]) as usize;
                                for j in 0..cnt.min(8) {
                                    let e = arr + 0x38 * j;
                                    println!("  bind[{j}] {}  strs {:?}", hex(&b, e, 0x38), strs(&b, e, 0x38));
                                    if let Some(ao) = ptr(&b, e) {
                                        println!("    anim@{ao:#x} {}  strs {:?}", hex(&b, ao, 0x70), strs(&b, ao, 0x70));
                                    }
                                }
                            }
                        }
                    }
                }
                node = ptr(&b, nd + 16);
            }
        }
    }
    Ok(())
}
