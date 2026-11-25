//! Research: per fragment, its groups' names, flag words and child bound
//! types, one JSON line each (for decoding the group flag bits).
//! fraggroups <game> <work> [archive] [name regex]
use rayon::prelude::*;
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ws = sixng_core::export::Workspace::new(&a[1], &a[2]);
    let ar = ws.resolve(a.get(3).map(|s| s.as_str()).unwrap_or("levels"))?;
    let ix = ws.load_index(&ar)?;
    let re = regex::Regex::new(a.get(4).map(|s| s.as_str()).unwrap_or("."))?;
    ix.entries.par_iter().filter(|e| e.rtype.as_deref() == Some("ft") && e.name.as_deref().is_some_and(|n| re.is_match(n))).for_each(|e| {
        let Ok(r) = sixng_core::resource::Resource::load(&ar.path, e) else { return };
        let Ok(p) = sixng_core::fragphys::try_decode(&r) else { return };
        let skel = sixng_core::drawable::find_skeleton(&r);
        let bones: Vec<String> = skel.iter().map(|b| b.name.clone()).collect();
        let j = sixng_core::fragphys::fragment_json(&p, &bones);
        let groups: Vec<_> = j["groups"].as_array().unwrap().iter().map(|g| {
            let kinds: Vec<_> = g["children"].as_array().unwrap().iter().map(|i| j["children"][i.as_u64().unwrap() as usize]["bound"]["type"].clone()).collect();
            serde_json::json!([g["name"], g["flags"], g["flags2"], kinds, g["parent"], g["tuning"]["strength"], g["bone"].as_u64().and_then(|b| skel.get(b as usize)).map(|b| b.flags), g["tuning"]["joint_stiffness"]])
        }).collect();
        println!("{}", serde_json::json!({"name": e.name, "root": j["root_flags"], "g": groups}));
    });
    Ok(())
}
