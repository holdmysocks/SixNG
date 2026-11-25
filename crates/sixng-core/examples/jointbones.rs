//! Research: joint limits with their bones' orientation, and each group's
//! flag words, bone flags and limits, as jsonl.
//! jointbones <game> <work> <archive>
use rayon::prelude::*;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(&a[3])?;
    let ix = ws.load_index(&ar)?;
    ix.entries.par_iter().filter(|e| e.rtype.as_deref() == Some("ft")).for_each(|e| {
        let Ok(r) = sixng_core::resource::Resource::load(&ar.path, e) else { return };
        let Ok(p) = sixng_core::fragphys::try_decode(&r) else { return };
        let skel = sixng_core::drawable::find_skeleton(&r);
        let by_hash = |h: u32| skel.iter().find(|b| sixng_core::meta7a::joaat(&b.name) == h);
        let lim = |v: &[sixng_core::fragphys::JointLimit]| -> Vec<serde_json::Value> {
            v.iter().map(|l| {
                let b = by_hash(l.bone_hash);
                serde_json::json!({"bone": b.map(|b| &b.name), "bi": b.map(|b| b.index), "flags": b.map(|b| b.flags), "rot": b.map(|b| b.rotation), "bind": b.and_then(|b| b.bind_matrix), "min": l.min, "max": l.max})
            }).collect()
        };
        let groups: Vec<_> = p.groups.iter().map(|g| {
            let b = skel.get(g.bone as usize);
            serde_json::json!({"n": b.map(|b| &b.name), "f": g.flags, "bf": b.map(|b| b.flags), "bone": g.bone, "parent": g.parent, "t": g.tuning_raw, "nc": g.child_end - g.first_child})
        }).collect();
        println!("{}", serde_json::json!({"name": e.name, "root": p.root_tail[0].to_bits(), "rot": lim(&p.rotation_limits), "tra": lim(&p.translation_limits), "g": groups, "nb": skel.len()}));
    });
    Ok(())
}
