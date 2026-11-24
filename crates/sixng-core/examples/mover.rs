//! Research: the mover (bone 0) tracks of the clips of one dictionary, and
//! each clip's movement over the ground from its start (`anim::mover_at`).
//! Usage: mover <game folder> <work folder> <dictionary> [clip]
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (game, work) = (std::path::Path::new(&a[1]), std::path::Path::new(&a[2]));
    let (_, d) = sixng_core::anim::load_dictionary(game, work, &a[3])?;
    for c in &d.clips {
        if a.get(4).is_some_and(|n| !c.short_name().eq_ignore_ascii_case(n)) {
            continue;
        }
        let m = sixng_core::anim::clip_mover(&d, c);
        let kinds: Vec<String> = c.bindings.iter().flat_map(|b| d.animations[b.animation].tracks.iter().filter(|t| t.bone == 0).map(|t| format!("{:08x}", t.kind))).collect();
        println!("clip {} duration {:.3} bindings {} mover kinds {:?}", c.short_name(), c.duration, c.bindings.len(), kinds);
        for k in 0..=4 {
            let t = c.duration * k as f32 / 4.0;
            let (p, q) = sixng_core::anim::mover_at(&m, t);
            println!("  t {t:6.3}  moved [{:7.3} {:7.3} {:7.3}]  turned {:7.1} deg", p[0], p[1], p[2], 2.0 * q[2].atan2(q[3]).to_degrees());
        }
    }
    Ok(())
}
