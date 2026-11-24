//! Research: the clip objects of one clip dictionary, any clip type: map
//! hash, type (+0x10), the string at +0x28 and the first 0xa0 bytes.
//! Usage: clipkinds <archive path> <index json> <entry id> [<entry id> ...]
use sixng_core::{anim, scan};

fn ptr(b: &[u8], o: usize) -> Option<usize> {
    let v = u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?);
    let off = (v & 0x0fff_ffff) as usize;
    (v >> 28 == 5 && off < b.len()).then_some(off)
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn cstr(b: &[u8], o: usize) -> Option<String> {
    let e = b.get(o..)?.iter().position(|&c| c == 0)?;
    let s = &b[o..o + e];
    (!s.is_empty() && s.iter().all(|c| c.is_ascii_graphic() || *c == b' ')).then(|| String::from_utf8_lossy(s).into_owned())
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let archive = std::path::PathBuf::from(&a[1]);
    let ix = scan::load_index(std::path::Path::new(&a[2]))?;
    for id in &a[3..] {
        let e = ix.entries.iter().find(|e| &e.id == id).ok_or_else(|| anyhow::anyhow!("no {id}"))?;
        let b = anim::dictionary_bytes(&archive, e)?;
        println!("== {id} {}", anim::dictionary_name(&b).unwrap_or_default());
        let buckets = ptr(&b, 0x48).unwrap();
        let n = u16::from_le_bytes([b[0x50], b[0x51]]) as usize;
        for i in 0..n {
            let mut node = ptr(&b, buckets + 8 * i);
            while let Some(nd) = node {
                let h = u32_at(&b, nd);
                if let Some(co) = ptr(&b, nd + 8) {
                    let kind = u32_at(&b, co + 0x10);
                    let s28 = ptr(&b, co + 0x28).and_then(|p| cstr(&b, p));
                    // every pointer in the first 0xa0 bytes that leads to a string
                    let strs: Vec<(usize, String)> = (0..0xa0).step_by(8).filter_map(|o| ptr(&b, co + o).and_then(|p| cstr(&b, p)).map(|s| (o, s))).collect();
                    let hex: String = b[co..(co + 0xa0).min(b.len())].iter().map(|x| format!("{x:02x}")).collect();
                    println!("{h:08x} kind={kind} name={s28:?} strs={strs:?}\n  {hex}");
                }
                node = ptr(&b, nd + 16);
            }
        }
    }
    Ok(())
}
