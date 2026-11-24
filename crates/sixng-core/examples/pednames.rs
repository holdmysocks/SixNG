//! Research: how many unnamed ped models of the outfit list a wider name
//! search finds, per word source and name form, beside a control (the same
//! candidates behind prefixes no ped model has) that measures chance hits.
//! Usage: pednames <game folder> <work folder>
use rayon::prelude::*;
use sixng_core::{game::ArchiveInfo, meta7a::joaat, names, pedlook, scan, text};
use std::collections::{BTreeMap, BTreeSet, HashSet};
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (game, work) = (std::path::Path::new(&a[1]), std::path::Path::new(&a[2]));
    names::set_work_dir(work);
    let t = pedlook::models(game).map_err(|e| anyhow::anyhow!(e))?;
    let known = pedlook::model_names(game).map_err(|e| anyhow::anyhow!(e))?;
    let keys: HashSet<u32> = t.by_model.keys().copied().filter(|h| !known.names.contains_key(h)).collect();
    eprintln!("{} unnamed models", keys.len());
    // word sources
    let mut index_words: BTreeSet<String> = BTreeSet::new();
    for f in std::fs::read_dir(work.join("index"))? {
        let p = f?.path();
        if p.extension().is_some_and(|e| e == "json") {
            if let Ok(ix) = scan::load_index(&p) {
                for e in &ix.entries {
                    for n in e.name.iter().chain(e.members.iter()) {
                        for w in n.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()) {
                            if w.len() >= 2 && w.len() <= 20 {
                                index_words.insert(w.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    let path = game.join("data.rpf");
    let ai = ArchiveInfo { id: "data.rpf".into(), size: 0, container: "rpf9".into(), path };
    let dix = scan::load_index(&scan::index_path(&work.join("index"), "data.rpf"))?;
    let (tables, _) = text::read_all(&ai, &dix.entries, &[], sixng_core::export::no_progress())?;
    let text_words: HashSet<String> = tables
        .par_iter()
        .map(|t| {
            let mut s = HashSet::new();
            for x in t.strings.values() {
                for w in text::plain(x).split(|c: char| !c.is_ascii_alphabetic()) {
                    if w.len() >= 3 && w.len() <= 16 {
                        s.insert(w.to_ascii_lowercase());
                    }
                }
            }
            s
        })
        .reduce(HashSet::new, |mut a, b| {
            a.extend(b);
            a
        });
    eprintln!("{} index words, {} text words", index_words.len(), text_words.len());
    let real: Vec<String> = "agsu".chars().flat_map(|k| "fm".chars().flat_map(move |s| "ymo".chars().map(move |g| format!("{k}_{s}_{g}_")))).chain(["mp_f_".to_string(), "mp_m_".to_string(), "cs_".into(), "ig_".into(), "csb_".into()]).collect();
    // control prefixes: same shapes, letters no ped model prefix uses
    let fake: Vec<String> = "qxzj".chars().flat_map(|k| "kw".chars().flat_map(move |s| "vbd".chars().map(move |g| format!("{k}_{s}_{g}_")))).chain(["qp_k_".to_string(), "qp_w_".to_string(), "xq_".into(), "zj_".into(), "xqb_".into()]).collect();
    let suffixes: Vec<String> = std::iter::once(String::new()).chain((0..10).map(|n| format!("_{n:02}"))).collect();
    let run = |label: &str, words: &Vec<String>| {
        for (kind, prefixes) in [("real", &real), ("control", &fake)] {
            let hits: Vec<String> = words
                .par_iter()
                .flat_map_iter(|w| {
                    let mut v = Vec::new();
                    for p in prefixes.iter() {
                        for s in &suffixes {
                            let c = format!("{p}{w}{s}");
                            if keys.contains(&joaat(&c)) {
                                v.push(c);
                            }
                        }
                    }
                    v
                })
                .collect();
            let mut by: BTreeMap<String, usize> = BTreeMap::new();
            for h in &hits {
                *by.entry(h.split('_').next().unwrap_or("").to_string()).or_insert(0) += 1;
            }
            println!("{label}\t{kind}\t{} candidates\t{} hits\t{by:?}", words.len() * prefixes.len() * suffixes.len(), hits.len());
            if kind == "real" {
                let mut h = hits.clone();
                h.sort();
                for x in h.iter().take(400) {
                    println!("    {x}");
                }
            }
        }
    };
    let iw: Vec<String> = index_words.iter().cloned().collect();
    run("index words", &iw);
    let tw: Vec<String> = text_words.iter().filter(|w| !index_words.contains(*w)).cloned().collect();
    run("text words", &tw);
    // two-word joins of index words that occur next to each other in a name are covered by prefixes;
    // here: index word + index word, short lists only
    let short: Vec<&String> = iw.iter().filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphabetic())).collect();
    eprintln!("{} alphabetic index words for pairs", short.len());
    if short.len() <= 12000 {
        let pairs: Vec<String> = short.par_iter().flat_map_iter(|a| short.iter().map(move |b| format!("{a}_{b}"))).collect();
        run("index word pairs", &pairs);
    }
    Ok(())
}
