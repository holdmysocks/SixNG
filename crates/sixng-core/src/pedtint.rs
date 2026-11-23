//! Ped clothing colours: tint palettes and the wardrobe tint bytes.
//!
//! Most ped clothing shaders bind no colour texture. Their packed data map
//! (slot `739e3d0d`) holds R = 255, G = a grey shade of the fabric and
//! B = a flat per-region value; the colour comes from a tint palette:
//!
//! * A palette is a 64 x 256 RGBA8 sRGB texture in `textures.rpf`
//!   (`ped_tint_generic_clean`, `ped_tint_vstation_generic`,
//!   `ped_tint_hair`, `animal_tint_*` …, 23 in one dictionary). It is stored
//!   in the Gfx10 `SW_4KB_S` layout (the dictionary decode, which assumes
//!   another tiling for 4-byte texels, scrambles it). Each of the 256 rows is
//!   one colour as a dark-to-light ramp in texels 2..=55; texels 0-1 are a
//!   constant border and 56-63 one grey ramp that is the same in every row.
//! * A wardrobe item (`f795e8ba`) names a palette by the joaat of its
//!   texture name (`5ecf9331`) and gives three tint bytes (`c4d75d79`,
//!   `f4fb3dec`, `ab5d2a85`): the rows of tint 0, 1 and 2. 255 means off.
//!
//! How a garment gets its tint (measured, see `resolve`): walk the item and
//! its parents (`080eec39`); the first item whose bytes are not 255,255,255
//! (inherit) or 0,0,0 (unset) gives the rows, with its own palette, or
//! `ped_tint_vstation_generic` when it names none. Against the items' own
//! colour tags (black, white, blue, red, green; 48 K tagged items of
//! packs.rpf) row tint 0 of that palette at mid shade agrees on 61 %
//! (vstation_generic 68 %, vstation_generic_desat 67 %, vstation_special
//! 71 %, generic_clean 27 %). As the default for items that name no
//! palette, `vstation_generic_desat` gives 58 % and `generic_clean` 37 %
//! overall. Only 39 % of the tagged items resolve a tint at all. The
//! generic_clean bytes look like placeholders the game fills at run time;
//! items with none of these bytes have no stored colour.
//!
//! The export colours a part as palette row `tint 0`, sampled at the data
//! map's G (texel 2 + G * 53 / 255). How the R and B channels bring in
//! tint 1 and tint 2 is not decoded, so multi-colour garments come out in
//! their main colour only.

use crate::gfx10::{Layout, Swizzle};
use crate::meta7a::joaat;
use crate::resource::Resource;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Shader slot of the packed data map whose G channel is the shade.
pub const SHADE_PARAM: &str = "739e3d0d";
/// Palette used when the item that sets the tint bytes names none.
pub const DEFAULT_PALETTE: &str = "ped_tint_vstation_generic";
/// First and last texel of a row's colour ramp.
pub const RAMP: (usize, usize) = (2, 55);

/// Palette texture names (the dictionary holds them; listed here to name the
/// hashes the wardrobe uses).
pub const NAMES: &[&str] = &[
    "ped_tint_generic_clean",
    "ped_tint_generic_desaturated",
    "ped_tint_generic_vibrant",
    "ped_tint_garment_clean",
    "ped_tint_garment_desaturated",
    "ped_tint_garment_vibrant",
    "ped_tint_hair",
    "ped_tint_vstation_generic",
    "ped_tint_vstation_generic_desat",
    "ped_tint_vstation_special",
    "ped_tint_vstation_special_desat",
    "ped_tint_vstation_master1",
    "ped_tint_vstation_master2",
    "ped_tint_vstation_master3",
    "ped_tint_vstation_master4",
    "animal_tint_generic",
    "animal_tint_generic_1",
    "animal_tint_vibrant",
    "animal_tint_vibrant_1",
    "animal_tint_desaturated",
    "proxytint_palette",
];

pub fn palette_name(h: u32) -> Option<&'static str> {
    NAMES.iter().copied().find(|n| joaat(n) == h)
}

/// The tint of one wardrobe item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    /// joaat of the palette texture name.
    pub palette: u32,
    /// Rows of tint 0, 1, 2 (255 = off).
    pub rows: [u8; 3],
    /// The item that set the bytes.
    pub item: u32,
    /// The item named no palette: `DEFAULT_PALETTE`.
    pub default_palette: bool,
}

impl Tint {
    /// `outfit --tint` value: `<palette hex>:<r0>.<r1>.<r2>`.
    pub fn spec(&self) -> String {
        format!("{:08x}:{}.{}.{}", self.palette, self.rows[0], self.rows[1], self.rows[2])
    }
}

/// One item of a chain, item first, then its parents:
/// (id, palette hash, tint bytes).
pub type ChainItem = (u32, u32, [u8; 3]);

/// The tint of an item from its chain (see the module notes).
pub fn resolve(chain: &[ChainItem]) -> Option<Tint> {
    let &(item, pal, rows) = chain.iter().find(|c| c.2 != [255; 3] && c.2 != [0; 3])?;
    let default_palette = pal == 0;
    Some(Tint { palette: if default_palette { joaat(DEFAULT_PALETTE) } else { pal }, rows, item, default_palette })
}

/// Parse `--tint` values: `<drawable>=<palette hex>:<r0>.<r1>.<r2>`.
pub fn parse_spec(s: &str) -> Result<(String, u32, [u8; 3]), String> {
    let bad = || format!("--tint wants drawable=palette:r0.r1.r2, got {s:?}");
    let (name, v) = s.split_once('=').ok_or_else(bad)?;
    let (pal, rows) = v.split_once(':').ok_or_else(bad)?;
    let pal = u32::from_str_radix(pal.trim().trim_start_matches("0x"), 16).map_err(|_| bad())?;
    let r: Vec<u8> = rows.split('.').map(|x| x.trim().parse::<u8>()).collect::<Result<_, _>>().map_err(|_| bad())?;
    let rows: [u8; 3] = r.try_into().map_err(|_| bad())?;
    Ok((name.trim().to_string(), pal, rows))
}

/// A decoded palette: `w` x `h` sRGB RGBA8, row-major.
#[derive(Debug, Clone)]
pub struct Palette {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Palette {
    /// Row `row` at shade `v` (0..=255), linearly between ramp texels.
    pub fn sample(&self, row: u8, v: u8) -> [u8; 3] {
        let y = (row as usize).min(self.h - 1);
        let x = RAMP.0 as f32 + v as f32 * (RAMP.1 - RAMP.0) as f32 / 255.0;
        let (x0, f) = (x.floor() as usize, x.fract());
        let px = |x: usize| {
            let o = (y * self.w + x.min(self.w - 1)) * 4;
            [self.rgba[o], self.rgba[o + 1], self.rgba[o + 2]]
        };
        let (a, b) = (px(x0), px(x0 + 1));
        [0, 1, 2].map(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * f).round() as u8)
    }
}

/// Untile a palette texture (Gfx10 `SW_4KB_S`, one mip, 4-byte texels).
pub fn untile(raw: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    Some(Layout::new(Swizzle::S4K, w, h, 1, 4)?.untile(raw, 0))
}

/// Colour a shade map: RGBA8 in, RGBA8 sRGB out (alpha kept opaque). With a
/// palette row, each texel is that row at the texel's G; without one, G as
/// grey.
pub fn bake(shade: &[u8], tint: Option<(&Palette, u8)>) -> Vec<u8> {
    let mut lut = [[0u8; 3]; 256];
    for (v, c) in lut.iter_mut().enumerate() {
        *c = match tint {
            Some((p, row)) => p.sample(row, v as u8),
            None => [v as u8; 3],
        };
    }
    shade.chunks_exact(4).flat_map(|t| {
        let c = lut[t[1] as usize];
        [c[0], c[1], c[2], 255]
    }).collect()
}

/// The palettes of `textures.rpf` by name hash (read once per run).
pub fn palettes(game_root: &Path) -> Result<&'static HashMap<u32, Palette>, String> {
    static P: OnceLock<Result<HashMap<u32, Palette>, String>> = OnceLock::new();
    P.get_or_init(|| {
        let work = crate::names::work_dir().ok_or("no work directory")?;
        let ixp = crate::scan::index_path(&work.join("index"), "textures.rpf");
        let ix = crate::scan::load_index(&ixp).map_err(|e| format!("textures.rpf index ({}): {e}; index textures.rpf first", ixp.display()))?;
        let arc = game_root.join("textures.rpf");
        let want: HashMap<u32, &str> = NAMES.iter().map(|n| (joaat(n), *n)).collect();
        let mut out = HashMap::new();
        for e in ix.entries.iter().filter(|e| e.rtype.as_deref() == Some("txd")) {
            let Ok(sys) = Resource::load_sys(&arc, e) else { continue };
            let hit = |t: &crate::texture::TexInfo| want.contains_key(&joaat(&t.name)) && t.dxgi_format == 29 && t.mips == 1;
            if !crate::texture::find_textures(&sys).iter().any(hit) {
                continue;
            }
            let Ok(r) = Resource::load(&arc, e) else { continue };
            for t in crate::texture::find_textures(&r).iter().filter(|t| hit(t)) {
                let Some(raw) = r.gfx.get(t.data_off as usize..) else { continue };
                if let Some(rgba) = untile(raw, t.width, t.height) {
                    out.insert(joaat(&t.name), Palette { w: t.width as usize, h: t.height as usize, rgba });
                }
            }
        }
        if out.is_empty() {
            return Err("no ped tint palettes found in textures.rpf".into());
        }
        Ok(out)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_hash_as_the_wardrobe_does() {
        assert_eq!(joaat("ped_tint_generic_clean"), 0xa7c8_d8e0);
        assert_eq!(joaat(DEFAULT_PALETTE), 0x0531_7603);
        assert_eq!(palette_name(0x2071_bb12), Some("ped_tint_vstation_special"));
        assert_eq!(palette_name(1), None);
    }

    #[test]
    fn resolve_skips_inherit_and_unset() {
        let clean = joaat("ped_tint_generic_clean");
        // item inherits (255s), parent sets rows with its own palette
        let t = resolve(&[(1, 0, [255; 3]), (2, clean, [0, 156, 6]), (3, 0, [255; 3])]).unwrap();
        assert_eq!((t.item, t.palette, t.rows, t.default_palette), (2, clean, [0, 156, 6], false));
        // rows without a palette: the default palette
        let t = resolve(&[(1, 0, [19, 19, 19]), (2, clean, [0; 3])]).unwrap();
        assert_eq!((t.item, t.palette, t.default_palette), (1, joaat(DEFAULT_PALETTE), true));
        assert_eq!(t.spec(), "05317603:19.19.19");
        // all unset or inherited: no stored colour
        assert!(resolve(&[(1, clean, [0; 3]), (2, 0, [255; 3])]).is_none());
    }

    #[test]
    fn spec_round_trip() {
        assert_eq!(parse_spec("pants_full=05317603:19.1.255"), Ok(("pants_full".into(), 0x0531_7603, [19, 1, 255])));
        assert!(parse_spec("pants_full=05317603:19.1").is_err());
        assert!(parse_spec("pants_full").is_err());
    }

    /// A 64 x 2 palette: row 1 ramps red 0..=53 over the ramp texels.
    fn palette() -> Palette {
        let (w, h) = (64, 2);
        let mut rgba = vec![0u8; w * h * 4];
        for x in 0..w {
            let o = (w + x) * 4;
            rgba[o] = x.clamp(RAMP.0, RAMP.1) as u8 - RAMP.0 as u8;
            rgba[o + 3] = 255;
        }
        Palette { w, h, rgba }
    }

    #[test]
    fn sample_maps_shade_onto_the_ramp() {
        let p = palette();
        assert_eq!(p.sample(1, 0), [0, 0, 0]);
        assert_eq!(p.sample(1, 255), [53, 0, 0]);
        assert_eq!(p.sample(1, 128), [27, 0, 0]);
        // rows past the palette clamp to the last row
        assert_eq!(p.sample(200, 255), [53, 0, 0]);
    }

    #[test]
    fn bake_uses_green_as_shade() {
        let p = palette();
        let shade = [255u8, 255, 64, 127, 255, 0, 90, 127];
        assert_eq!(bake(&shade, Some((&p, 1))), vec![53, 0, 0, 255, 0, 0, 0, 255]);
        assert_eq!(bake(&shade, None), vec![255, 255, 255, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn untile_places_the_first_texels_of_a_4k_block() {
        // SW_4KB_S, 4-byte texels: x0, x1 are the lowest element bits, then y0.
        let (w, h) = (64u32, 64u32);
        let raw: Vec<u8> = (0..(w * h * 4) as usize).map(|i| (i / 4) as u8).collect();
        let lin = untile(&raw, w, h).unwrap();
        let at = |x: usize, y: usize| lin[(y * w as usize + x) * 4];
        assert_eq!([at(0, 0), at(1, 0), at(2, 0), at(3, 0), at(0, 1)], [0, 1, 2, 3, 4]);
    }
}
