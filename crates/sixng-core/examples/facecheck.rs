//! Research: face networks of a head drawable and its shape keys.
//! facecheck <game dir> <archive id> <head name> [out.bin]
//! out.bin: the LOD0 basis, f32 inputs x outputs.
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let game = std::path::PathBuf::from(&a[1]);
    let ws = sixng_core::export::Workspace::new(&game, std::env::var("SIXNG_WORK").expect("SIXNG_WORK"));
    let arc = ws.resolve(&a[2])?;
    let t = std::time::Instant::now();
    let nets = sixng_core::facegraph::nets_for_head(&arc, &a[3]);
    for n in &nets {
        let max = n.basis.iter().fold(0f32, |m, x| m.max(x.abs()));
        println!("graph {:x}: {} controls, {} vertices, max {:.4} m", n.graph, n.inputs, n.vertices(), max);
    }
    println!("{:.2} s", t.elapsed().as_secs_f32());
    if let (Some(p), Some(n)) = (a.get(4), nets.last()) {
        std::fs::write(p, n.basis.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
    }
    Ok(())
}
