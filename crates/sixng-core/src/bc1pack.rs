//! Preset window for BCPack BC1 chunks.
//!
//! BC1 chunks are decoded by the `bcpack` crate. A block whose back distance
//! reaches before the start of the chunk takes its prediction from a 256-slot
//! preset window, so a wrong slot value shifts the colour endpoints of every
//! block chained to it. The crate's default window is used, with the slots
//! listed in [`OVERRIDES`] replaced.
//!
//! The replaced values come from a rule that needs no reference image: a
//! block in three-colour mode (c0 <= c1 as 16-bit values) does not use index
//! 3, the transparent one. Textures with real cut-out alpha break it, so it
//! is not exact: on chains that start at the default prediction 0.018 % of
//! blocks do. On chains that start at some preset slots up to a quarter of
//! the blocks broke it and decoded with black holes. A slot is replaced only
//! where a refit lowered the violations on held-out chunks and did not raise
//! the error against plain child mips.
//!
//! A second set of slots is fitted on mip consistency: per 2x2 quadrant, a
//! decoded block of mip m should match the pixels of mip m+1 it covers, with
//! mip m+1 decoded from either plain or BCPack data. Blocks whose reference
//! starts at the same slot are skipped, since both sides move together there.
//! A slot is kept only where the fit gains on both even and odd chunks and a
//! check over all textures shows no regression traced to it.

use bcpack::bc1::Window;

include!("bc1pack_tables.rs");

/// The preset window used for BC1 chunks.
pub fn window() -> Window {
    let mut w = Window::default_table();
    for &(slot, fields) in OVERRIDES.iter() {
        w.slots[slot as usize] = fields;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_are_valid_fields() {
        let mut seen = [false; 256];
        for &(slot, f) in OVERRIDES.iter() {
            assert!(!seen[slot as usize], "slot {slot} listed twice");
            seen[slot as usize] = true;
            // R5 G6 B5 for both endpoints
            for (k, &v) in f.iter().enumerate() {
                assert!(v < if k % 3 == 1 { 64 } else { 32 }, "slot {slot} field {k}");
            }
        }
    }

    #[test]
    fn window_differs_only_in_overridden_slots() {
        let (d, w) = (Window::default_table(), window());
        for s in 0..256 {
            let listed = OVERRIDES.iter().find(|o| o.0 as usize == s);
            match listed {
                Some(o) => assert_eq!(w.slots[s], o.1),
                None => assert_eq!(w.slots[s], d.slots[s]),
            }
        }
    }
}
