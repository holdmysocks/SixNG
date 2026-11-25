//! Research: survey the joint-limit arrays behind fragment drawables (+0x90).
//! jointsurvey <game> <work> <archive>   -> jsonl on stdout
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(&a[3])?;
    let ix = ws.load_index(&ar)?;
    for e in ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("ft")) {
        let Ok(r) = sixng_core::resource::Resource::load(&ar.path, e) else { continue };
        let Some(d) = r.sys_off(r.u64(0x18)) else { continue };
        let Some(j) = r.sys_off(r.u64(d + 0x90)) else { continue };
        let hdr: Vec<u32> = (0..20).map(|k| r.u32(j + 4 * k)).collect();
        let mut arrs = Vec::new();
        for (co, po) in [(0x28usize, 0x30usize), (0x38, 0x40)] {
            let n = r.u16(j + co) as usize;
            let mut recs = Vec::new();
            if let Some(t) = r.sys_off(r.u64(j + po)) {
                for i in 0..n.min(64) {
                    let o = t + 0x30 * i;
                    if o + 0x30 > r.sys.len() { break }
                    let v: Vec<f32> = (0..8).map(|q| r.f32(o + 4 * q)).collect();
                    let tail: Vec<u32> = (8..12).map(|q| r.u32(o + 4 * q)).collect();
                    recs.push(serde_json::json!({"v": v, "tail": tail}));
                }
            }
            arrs.push(serde_json::json!({"n": n, "n2": r.u16(j + co + 4), "recs": recs}));
        }
        let groups = r.u16(0x30);
        println!("{}", serde_json::json!({"name": e.name, "root_flags": r.u32(0xB8), "hdr": hdr, "arrs": arrs, "groups": groups}));
    }
    Ok(())
}
