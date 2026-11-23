//! Research: prop-batch records with implausible transforms, with the stream layout of their file.
//! propbad <game> [archive]   (DUMP=<entry id> prints the flagged records of one file)
use rayon::prelude::*;
fn bad(i: &sixng_core::batches::PropInstance) -> bool {
    (0..3).any(|k| !i.p[k].is_finite() || i.p[k].abs() > 1e6 || !i.s[k].is_finite() || i.s[k] > 1e3 || i.s[k] < 1e-4)
}
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], "work");
    let ar = ws.resolve(a.get(2).map(|s| s.as_str()).unwrap_or("levels"))?;
    let ix = ws.load_index(&ar)?;
    let r: Vec<String> = ix.entries.par_iter().filter(|e| e.sub == "7a" && std::env::var("DUMP").map_or(true, |d| d == e.id)).filter_map(|e| {
        let rs = sixng_core::resource::Resource::load_sys(&ar.path, e).ok()?;
        let m = sixng_core::meta7a::Meta::parse(&rs)?;
        if m.objects(sixng_core::batches::PROP_BATCH).is_empty() { return None; }
        let rm = sixng_core::resource::Resource::load_meta(&ar.path, e).ok()?;
        let mm = sixng_core::meta7a::Meta::parse(&rm)?;
        let bs = sixng_core::batches::prop_batches(&rm, &mm);
        let mut out = Vec::new();
        let (mut tot, mut nb) = (0, 0);
        for (bi, b) in bs.iter().enumerate() {
            tot += b.instances.len();
            let idx: Vec<usize> = b.instances.iter().enumerate().filter(|(_, i)| bad(i)).map(|(k, _)| k).collect();
            nb += idx.len();
            if std::env::var("DUMP").ok().as_deref() == Some(e.id.as_str()) {
                for &k in &idx { let i = &b.instances[k]; out.push(format!("    #{k} p {:?} s {:?} q {:?} bbox {:?}", i.p, i.s, i.q, b.bbox)); }
            }
            if !idx.is_empty() {
                let o = mm.objects(sixng_core::batches::PROP_BATCH)[bi];
                let p = rm.u64(o + 0x90);
                let off = mm.ptr(p).map(|x| x.1);
                out.push(format!("  batch {bi} arch {:08x} n {} ptr {p:#x} off {:?} gfx_base {:#x} bad {:?}", b.archetype, b.instances.len(), off, rm.gfx_base, idx));
            }
        }
        Some(format!("{} streams {:?} sys {:#x} tot {tot} bad {nb}\n{}", e.id, e.streams, rm.sys.len(), out.join("\n")))
    }).collect();
    for x in &r { if !x.ends_with(" bad 0\n") { println!("{x}"); } }
    Ok(())
}
