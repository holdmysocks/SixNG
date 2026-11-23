//! sixng-core: GTA VI (RPF9, Xbox Series) asset extraction core.
pub mod anim;
pub mod audio;
pub mod audiometa;
pub mod scriptdoc;
pub mod wardrobe;
pub mod bounds;
pub mod classify;
pub mod navmesh;
pub mod lights;
pub mod partskin;
pub mod paths;
pub mod outfit;
pub mod pedskel;
pub mod bodyshape;
pub mod headskin;
pub mod batches;
pub mod drawable;
pub mod export;
pub mod furlink;
pub mod game;
pub mod gltf;
pub mod groom;
pub mod cloth;
pub mod text;
pub mod gamenames;
pub mod texusers;
pub mod headdelta;
pub mod meta7a;
pub mod names;
pub mod material;
pub mod gfx10;
pub mod resource;
pub mod texture;
pub mod ttpf;
pub mod bc3pack;
pub mod bc45pack;
pub mod bc6h;
pub mod bc6pack;
pub mod bc7pack;
pub mod gaps;
pub mod lodlinks;
pub mod bink;
pub mod interiorents;
pub mod pso;
pub mod gfxfont;
pub mod ptfx;
pub mod vehicles;
pub mod rpf9;
pub mod scan;
pub mod surface;
pub mod timecycle;
pub mod world;

pub const INDEX_VERSION: &str = "5";

/// Per-user settings folder for SixNG: `%APPDATA%\SixNG` on Windows,
/// `~/Library/Application Support/SixNG` on macOS, `$XDG_CONFIG_HOME/SixNG`
/// (or `~/.config/SixNG`) elsewhere.
pub fn user_settings_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |k| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(base.join("SixNG"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn user_settings_dir_ends_in_sixng() {
        let d = super::user_settings_dir().expect("settings dir");
        assert!(d.ends_with("SixNG"), "{}", d.display());
        #[cfg(target_os = "macos")]
        assert!(d.ends_with("Library/Application Support/SixNG"), "{}", d.display());
    }
}
