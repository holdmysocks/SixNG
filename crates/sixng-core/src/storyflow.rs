//! Story flow tags and the mission names they give.
//!
//! The story pack's protected ambient-rule files (RBF, read through the
//! table of contents) name the content tags the story flow sets, as text:
//! one per finished mission, `Flow<Arc words><Mission words>Complete`. A
//! scene name starts with the mission id, `<arc>_<mission>`, whose parts are
//! short forms of those words (letters left out, none added or reordered).
//! [`match_names`] pairs the two by that rule, so the names come from the
//! game's files at run time and none are written here.

use crate::export::Workspace;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Protected files larger than this are not ambient-rule files.
const MAX_RULE_FILE: u64 = 1 << 20;
const CACHE_VERSION: u32 = 1;

/// Names for missions and arcs, keyed by their scene-name ids
/// (`mny_kidnp`, and its arc `mny`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Names {
    pub arcs: BTreeMap<String, String>,
    pub missions: BTreeMap<String, String>,
}

impl Names {
    /// "Arc: Mission", "Arc" or nothing, for a mission id.
    pub fn label(&self, mission: &str) -> Option<String> {
        let arc = mission.rsplit_once('_').map(|(a, _)| a).unwrap_or(mission);
        match (self.arcs.get(arc), self.missions.get(mission)) {
            (Some(a), Some(m)) => Some(format!("{a}: {m}")),
            (Some(a), None) => Some(a.clone()),
            (None, Some(m)) => Some(m.clone()),
            (None, None) => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    tags: Vec<String>,
}

/// The flow tags of the story pack: from `<work>/index/story_flow_tags.json`,
/// or read from the pack (a few seconds; needs the game executable for the
/// table of contents) and then cached.
pub fn tags(ws: &Workspace) -> Result<Vec<String>> {
    let path = ws.index_dir().join("story_flow_tags.json");
    if let Ok(b) = std::fs::read(&path) {
        if let Ok(c) = serde_json::from_slice::<Cache>(&b) {
            if c.version == CACHE_VERSION {
                return Ok(c.tags);
            }
        }
    }
    let a = ws.resolve(crate::cutscene::STORY_ARCHIVE)?;
    let tags = read_tags(&ws.game, &a.path)?;
    std::fs::create_dir_all(ws.index_dir())?;
    std::fs::write(&path, serde_json::to_vec(&Cache { version: CACHE_VERSION, tags: tags.clone() })?)?;
    Ok(tags)
}

/// Mission and arc names for the given mission ids, from the story pack.
pub fn names(ws: &Workspace, missions: &[String]) -> Result<Names> {
    Ok(match_names(&tags(ws)?, missions))
}

/// Every `Flow…` tag named in the protected RBF files of an archive and
/// the archives stored in it.
pub fn read_tags(game: &Path, archive: &Path) -> Result<Vec<String>> {
    use crate::rpf9toc;
    let mut cipher = crate::gamecipher::open(game)?;
    let mut f = std::fs::File::open(archive).with_context(|| format!("open {}", archive.display()))?;
    let mut out = BTreeSet::new();
    let mut todo = vec![0u64];
    while let Some(base) = todo.pop() {
        let toc = rpf9toc::open_toc(cipher.as_mut(), &mut f, base).with_context(|| format!("table of contents at {base:#x}"))?;
        if !toc.has_entries() {
            continue;
        }
        let files = toc.files()?;
        for e in &files {
            if e.crypto.is_none() || e.resource.is_some() || e.unpacked > MAX_RULE_FILE {
                continue;
            }
            let Ok((d, _)) = rpf9toc::read_file(cipher.as_mut(), &mut f, base, e) else { continue };
            if let Ok((xml, _)) = crate::rbf::to_xml(&d) {
                out.extend(flow_tags_in(&xml));
            }
        }
        todo.extend(rpf9toc::nested_archives(&mut f, base, &files).into_iter().map(|o| base + o));
    }
    Ok(out.into_iter().collect())
}

/// The `Flow…` names in a text: tag ids and rule keys.
pub fn flow_tags_in(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(k) = text[i..].find("Flow") {
        let s = i + k;
        let e = s + text[s..].bytes().take_while(|c| c.is_ascii_alphanumeric() || *c == b'_').count();
        let boundary = s == 0 || !(b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_');
        if boundary && e > s + 4 && b[s + 4].is_ascii_uppercase() {
            out.insert(text[s..e].to_string());
        }
        i = e.max(s + 4);
    }
    out
}

/// Words of a CamelCase name; a run of capitals is one word, except for
/// its last letter when a lower-case letter follows (`CSCStunt` → CSC,
/// Stunt). Digits stay with the word before them.
pub fn words(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    for i in 0..c.len() {
        let up = c[i].is_ascii_uppercase();
        let starts = i == 0
            || (up && (c[i - 1].is_ascii_lowercase() || c[i - 1].is_ascii_digit()))
            || (up && c[i - 1].is_ascii_uppercase() && c.get(i + 1).is_some_and(|n| n.is_ascii_lowercase()));
        if starts || out.is_empty() {
            out.push(String::new());
        }
        out.last_mut().unwrap().push(c[i]);
    }
    out
}

/// Whether `short` is `long` with letters left out, starting with its
/// first letter.
fn short_form(short: &str, long: &str) -> bool {
    let (s, l) = (short.to_ascii_lowercase(), long.to_ascii_lowercase());
    if s.is_empty() || s.as_bytes()[0] != l.as_bytes().first().copied().unwrap_or(0) {
        return false;
    }
    let mut it = l.chars();
    s.chars().all(|ch| it.any(|x| x == ch))
}

/// How many words, from the first, `short` spends its letters on when it
/// is a short form of the words joined (letters matched as early as they
/// can be); `None` when it is not.
fn words_spanned(short: &str, w: &[String]) -> Option<usize> {
    if !short_form(short, &w.concat()) {
        return None;
    }
    let mut it = w.iter().enumerate().flat_map(|(i, x)| x.chars().map(move |c| (i, c.to_ascii_lowercase())));
    let mut last = 0;
    for ch in short.to_ascii_lowercase().chars() {
        last = it.by_ref().find(|&(_, x)| x == ch)?.0;
    }
    Some(last + 1)
}

/// The words of a finished-mission tag (`FlowAbcDefComplete` → Abc, Def).
fn mission_words(tag: &str) -> Option<Vec<String>> {
    let body = tag.strip_prefix("Flow")?;
    let body = body.strip_suffix("AICondition").unwrap_or(body);
    let body = ["Completed", "Complete", "Completet"].iter().find_map(|s| body.strip_suffix(s))?;
    let w = words(body);
    (w.len() >= 2).then_some(w)
}

/// Pair finished-mission tags with mission ids (`<arc>_<mission>`, the arc
/// one or more parts). The arc parts, joined, are a short form of the tag's
/// first words, and the mission part a short form of a later run of words.
/// When several ids fit a tag, the one whose mission letters fall in the
/// fewest words wins; a tie names none. Only pairs where the id then has
/// one tag are kept. An id that starts another id (`abc` of `abc_def`) is
/// an arc, not a mission.
///
/// Where an arc's words end: for an arc with two or more matched tags, at
/// the end of the words the tags share (`Harbor` in HarborBoatRace and
/// HarborNightSwim); for one tag, where its mission part starts.
pub fn match_names(tags: &[String], missions: &[String]) -> Names {
    struct Fit<'a> {
        id: &'a str,
        w: Vec<String>,
        /// Fewest words the arc can take.
        arc_min: usize,
        /// The word where the mission part starts.
        start: usize,
        /// Words the mission letters fall in.
        span: usize,
    }
    let all: BTreeSet<&str> = missions.iter().map(|s| s.as_str()).collect();
    let is_arc = |m: &str| all.range::<str, _>((std::ops::Bound::Excluded(m), std::ops::Bound::Unbounded)).next().is_some_and(|n| n.strip_prefix(m).is_some_and(|r| r.starts_with('_')));
    let ids: BTreeSet<&str> = all.iter().copied().filter(|m| m.contains('_') && !is_arc(m)).collect();
    let mut seen = BTreeSet::new();
    let mut fits: Vec<Fit> = Vec::new();
    for t in tags {
        let Some(w) = mission_words(t) else { continue };
        if !seen.insert(w.clone()) {
            continue;
        }
        let mut found = Vec::new();
        for &id in &ids {
            let (arc, short) = id.rsplit_once('_').unwrap();
            let arc_letters: String = arc.split('_').collect();
            let Some(j) = (1..w.len()).find(|&j| short_form(&arc_letters, &w[..j].concat())) else { continue };
            // The start whose letters fall in the fewest words; the later one on a tie.
            if let Some((k, span)) = (j..w.len()).filter_map(|k| words_spanned(short, &w[k..]).map(|n| (k, n))).min_by_key(|&(k, n)| (n, std::cmp::Reverse(k))) {
                found.push(Fit { id, w: w.clone(), arc_min: j, start: k, span });
            }
        }
        let Some(best) = found.iter().map(|f| f.span).min() else { continue };
        found.retain(|f| f.span == best);
        if found.len() == 1 {
            fits.extend(found);
        }
    }
    let mut per_id: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &fits {
        *per_id.entry(f.id).or_default() += 1;
    }
    fits.retain(|f| per_id[f.id] == 1);
    let mut by_arc: BTreeMap<&str, Vec<&Fit>> = BTreeMap::new();
    for f in &fits {
        by_arc.entry(f.id.rsplit_once('_').unwrap().0).or_default().push(f);
    }
    let mut names = Names::default();
    for (arc, fs) in by_arc {
        let lo = fs.iter().map(|f| f.arc_min).max().unwrap();
        let hi = fs.iter().map(|f| f.start).min().unwrap();
        let split: Vec<usize> = if fs.len() == 1 {
            vec![fs[0].start]
        } else {
            let shared = (0..fs[0].w.len()).take_while(|&i| fs.iter().all(|f| f.w.get(i) == fs[0].w.get(i))).count();
            let j = shared.min(hi);
            if j >= lo { vec![j; fs.len()] } else { fs.iter().map(|f| f.arc_min).collect() }
        };
        let mut votes: BTreeMap<String, usize> = BTreeMap::new();
        for (f, &j) in fs.iter().zip(&split) {
            names.missions.insert(f.id.to_string(), f.w[j..].join(" "));
            *votes.entry(f.w[..j].join(" ")).or_default() += 1;
        }
        if let Some((a, _)) = votes.into_iter().max_by(|x, y| x.1.cmp(&y.1).then(y.0.cmp(&x.0))) {
            names.arcs.insert(arc.to_string(), a);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn camel_case_words() {
        assert_eq!(words("HarborBoatRace"), s(&["Harbor", "Boat", "Race"]));
        assert_eq!(words("XYZRadioTower2"), s(&["XYZ", "Radio", "Tower2"]));
        assert_eq!(words("Ch5Full"), s(&["Ch5", "Full"]));
    }

    #[test]
    fn tags_in_text() {
        let t = "<TagId>FlowHarborBoatRaceComplete</TagId><Item key=\"FlowHarborBoatRaceCompleteAICondition\"> Flowers NoFlowX";
        assert_eq!(flow_tags_in(t).into_iter().collect::<Vec<_>>(), s(&["FlowHarborBoatRaceComplete", "FlowHarborBoatRaceCompleteAICondition"]));
    }

    #[test]
    fn mission_ids_take_the_tag_they_shorten() {
        let tags = s(&[
            "FlowHarborBoatRaceComplete",
            "FlowHarborBoatRaceCompleteAICondition",
            "FlowHarborNightSwimCompleted",
            "FlowXYZRadioTowerComplete",
            "FlowChapter2Complete",
            "FlowShopSoda",
        ]);
        let ids = s(&["hbr_race", "hbr_swim", "hbr_dock", "x_yz_tower", "ch_two"]);
        let n = match_names(&tags, &ids);
        assert_eq!(n.missions.get("hbr_race").map(String::as_str), Some("Boat Race"));
        assert_eq!(n.missions.get("hbr_swim").map(String::as_str), Some("Night Swim"));
        assert_eq!(n.missions.get("x_yz_tower").map(String::as_str), Some("Tower"));
        assert!(!n.missions.contains_key("hbr_dock"));
        assert!(!n.missions.contains_key("ch_two"));
        assert_eq!(n.arcs.get("hbr").map(String::as_str), Some("Harbor"));
        assert_eq!(n.arcs.get("x_yz").map(String::as_str), Some("XYZ Radio"));
        assert_eq!(n.label("hbr_race").as_deref(), Some("Harbor: Boat Race"));
        assert_eq!(n.label("hbr_dock").as_deref(), Some("Harbor"));
        assert_eq!(n.label("zz_q"), None);
    }

    #[test]
    fn a_tag_that_fits_two_ids_names_neither() {
        let tags = s(&["FlowHarborRaceComplete"]);
        let n = match_names(&tags, &s(&["hbr_race", "har_race"]));
        assert!(n.missions.is_empty());
    }

    #[test]
    fn the_tighter_fit_wins_and_arc_ids_are_not_missions() {
        // `hbr_hrt` spreads over Heavy Rescue Truck; `hbr_truck` is one word.
        let tags = s(&["FlowHarborHeavyRescueTruckComplete", "FlowHarborOtherJobComplete"]);
        let n = match_names(&tags, &s(&["hbr_hrt", "hbr_truck", "hbr", "hbr_oth", "hbr_oth_job"]));
        assert_eq!(n.missions.get("hbr_truck").map(String::as_str), Some("Truck"));
        assert_eq!(n.missions.get("hbr_oth_job").map(String::as_str), Some("Job"));
        assert!(!n.missions.contains_key("hbr_hrt"));
        // `hbr_oth` is the arc of `hbr_oth_job`.
        assert!(!n.missions.contains_key("hbr_oth"));
    }

    #[test]
    fn an_arc_ends_where_its_tags_part() {
        let tags = s(&["FlowXYZTowerRadioPitchComplete", "FlowXYZTowerRadioLaunchPartyComplete", "FlowSoloArcPrepBigJobComplete"]);
        let n = match_names(&tags, &s(&["x_yzt_pitch", "x_yzt_party", "sol_job"]));
        assert_eq!(n.arcs.get("x_yzt").map(String::as_str), Some("XYZ Tower Radio"));
        assert_eq!(n.missions.get("x_yzt_party").map(String::as_str), Some("Launch Party"));
        assert_eq!(n.missions.get("x_yzt_pitch").map(String::as_str), Some("Pitch"));
        // One tag: the arc ends where the mission part starts.
        assert_eq!(n.arcs.get("sol").map(String::as_str), Some("Solo Arc Prep Big"));
        assert_eq!(n.missions.get("sol_job").map(String::as_str), Some("Job"));
    }
}
