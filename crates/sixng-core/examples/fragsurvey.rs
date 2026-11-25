//! Research: decode the physics of every fragment of an archive, one JSON line each.
//! fragsurvey <game> <work> [archive] [name regex]
use rayon::prelude::*;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(a.get(3).map(|s| s.as_str()).unwrap_or("levels"))?;
    let ix = ws.load_index(&ar)?;
    let re = regex::Regex::new(a.get(4).map(|s| s.as_str()).unwrap_or("."))?;
    let (ok, bad) = (std::sync::atomic::AtomicUsize::new(0), std::sync::atomic::AtomicUsize::new(0));
    ix.entries.par_iter().filter(|e| e.rtype.as_deref() == Some("ft") && e.name.as_deref().is_some_and(|n| re.is_match(n))).for_each(|e| {
        let Ok(r) = sixng_core::resource::Resource::load(&ar.path, e) else { return };
        match sixng_core::fragphys::try_decode(&r) {
            Ok(p) => {
                ok.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!("{}", serde_json::json!({"id": e.id, "name": e.name, "p": p}));
            }
            Err(why) => {
                bad.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!("{}", serde_json::json!({"id": e.id, "name": e.name, "fail": why, "head": (0..0x40).map(|k| r.u32(4 * k)).collect::<Vec<_>>()}));
            }
        }
    });
    eprintln!("decoded {} failed {}", ok.into_inner(), bad.into_inner());
    Ok(())
}
