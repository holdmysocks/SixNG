//! Research: print surface id -> physics material name, from the XML / text
//! files under the given folders (extracted from common).
//! surfnames <dir>...
fn main() -> anyhow::Result<()> {
    let mut texts = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = std::env::args().skip(1).map(Into::into).collect();
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            stack.extend(std::fs::read_dir(&p)?.filter_map(|e| e.ok().map(|e| e.path())));
        } else if let Ok(s) = std::fs::read_to_string(&p) {
            texts.push(s);
        }
    }
    for (k, v) in sixng_core::gamenames::surface_names_from(&texts) {
        println!("{k:04x} {v}");
    }
    Ok(())
}
