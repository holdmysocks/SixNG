//! World-data export steps with the CLI's default selection, for the GUI
//! (and anything else that wants "run step X on the whole archive").
use crate::classify::Kind;
use crate::export::{self, Progress, Workspace};
use crate::scan::Entry;
use anyhow::Result;
use std::path::Path;

/// (step id, button label, help) in pipeline order.
pub const STEPS: &[(&str, &str, &str)] = &[
    ("placements", "Placements", "map entities with transforms (levels)"),
    ("batches", "Batches", "instanced grass / prop batches (~5.8 GB)"),
    ("collision", "Collision", "collision GLBs split by surface id (~15 GB)"),
    ("navmesh", "Road navmesh", "navmesh road polygons (navmeshes)"),
    ("paths", "Lane graph", "path nodes, links, junctions, traffic lights"),
    ("lights", "Lights", "light emitters in world space"),
    ("interiors", "Interiors", "interior instances + collision links (after Collision)"),
    ("surfaces", "Surfaces", "inferred surface categories (after Collision, Lane graph, Batches)"),
    ("names", "Names", "surface material and street names (after Collision, Lane graph, Surfaces)"),
];

fn sel_meta(ix: &[Entry]) -> Vec<&Entry> {
    ix.iter().filter(|e| e.kind == Kind::Resource && e.sub == "7a").collect()
}

/// Run one step on the default archive (`levels`, or `navmeshes` for the
/// road navmesh). Returns a one-line summary.
pub fn run_step(ws: &Workspace, step: &str, out: &Path, pr: Progress) -> Result<String> {
    let levels = ws.resolve("levels")?;
    match step {
        "navmesh" => {
            let a = ws.resolve("navmeshes")?;
            let ix = ws.load_index(&a)?;
            let sel: Vec<&Entry> = ix.entries.iter().filter(|e| e.kind == Kind::RageObj).collect();
            let (n, roads) = export::export_navmesh(&a, &sel, out, true, pr)?;
            Ok(format!("navmesh: {n} tiles, {roads} with roads"))
        }
        _ => {
            let ix = ws.load_index(&levels)?;
            match step {
                "placements" => {
                    let v = export::export_placements(ws, &levels, &sel_meta(&ix.entries), out, pr)?;
                    Ok(format!("placements: {} map files, {} entities", v.len(), v.iter().map(|x| x.entities).sum::<usize>()))
                }
                "batches" => {
                    let b = export::export_batches(ws, &levels, &sel_meta(&ix.entries), out, pr)?;
                    Ok(format!("batches: {}", serde_json::to_string(&b)?))
                }
                "collision" => {
                    let sel: Vec<&Entry> = ix.entries.iter().filter(|e| e.kind == Kind::RageObj && e.name.is_none()).collect();
                    let (n, census) = export::export_collision(&levels, &sel, out, pr)?;
                    Ok(format!("collision: {n} bounds files, {} surface ids", census.len()))
                }
                "paths" => {
                    let (n, l, bad) = export::export_paths(&levels, &sel_meta(&ix.entries), out, pr)?;
                    Ok(format!("lane graph: {n} nodes, {l} links ({bad} unresolved)"))
                }
                "lights" => {
                    let (sets, lights, placed, (tc, distant, lod)) = export::export_lights(ws, &levels, &sel_meta(&ix.entries), out, pr)?;
                    Ok(format!("lights: {sets} sets, {lights} lights, {placed} placed, {distant} distant, {lod} unpaired LOD, {tc} timecycle modifier regions"))
                }
                "interiors" => {
                    let (n, ok, m) = export::export_interiors(&levels, &sel_meta(&ix.entries), &ix.entries, out, pr)?;
                    Ok(format!("interiors: {n} instances ({ok} resolved), {m} interior archetypes"))
                }
                "surfaces" => {
                    let nav = ws.resolve("navmeshes").ok();
                    let nix = nav.as_ref().and_then(|n| ws.load_index(n).ok());
                    let nsel: Vec<Entry> = nix.as_ref().map(|x| x.entries.iter().filter(|e| e.kind == Kind::RageObj).cloned().collect()).unwrap_or_default();
                    let inp = crate::surface::SurfaceInputs {
                        levels: &levels,
                        level_entries: &ix.entries,
                        navmeshes: match (&nav, nix.is_some()) {
                            (Some(n), true) => Some((n, nsel.as_slice())),
                            _ => None,
                        },
                    };
                    let s = crate::surface::surface_categories(&inp, out, &*pr)?;
                    Ok(format!("surfaces: {} ids, {:.1} % of collision area categorised", s.ids, 100.0 * s.area_covered))
                }
                "names" => {
                    let s = crate::gamenames::annotate(ws, &levels.id, out, pr)?;
                    Ok(format!("names: {s}"))
                }
                _ => Err(anyhow::anyhow!("unknown step {step}")),
            }
        }
    }
}
