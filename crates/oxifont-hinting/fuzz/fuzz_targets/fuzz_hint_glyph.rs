//! Fuzz target: drive the full hinting pipeline over arbitrary font bytes.
//!
//! The VM executes attacker-controlled bytecode from the `fpgm`/`prep`
//! programs (during `HintingEngine::new`) and from a glyph's own instruction
//! stream (during `hint_glyph`). The crate documents the strong invariant that
//! it *never panics on malformed or hostile bytecode* (bounded instruction
//! count, call depth and loop counts); this target exercises that claim.
//!
//! Invariants:
//!   - `SfntTableMap::parse`, `HintingEngine::new`, `set_ppem` and `hint_glyph`
//!     never panic on arbitrary input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oxifont_core::sfnt::SfntTableMap;
use oxifont_hinting::HintingEngine;

fuzz_target!(|data: &[u8]| {
    // The first two bytes (if present) pick a ppem; the rest is the font.
    let (ppem, font) = match data.split_first_chunk::<2>() {
        Some((head, rest)) => (u16::from_le_bytes(*head), rest),
        None => (16u16, data),
    };

    let map = match SfntTableMap::parse(font) {
        Ok(m) => m,
        Err(_) => return,
    };

    let mut engine = match HintingEngine::new(&map) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Clamp ppem into a sane, non-zero range to keep the harness productive
    // while still spanning small and large sizes.
    let ppem = (ppem % 2048).max(1);
    if engine.set_ppem(ppem).is_err() {
        return;
    }

    // Hint a spread of glyph ids, including out-of-range ones (must Err, not
    // panic) and composite glyphs (which recurse and resolve point matches).
    for gid in [0u16, 1, 2, 3, 7, 42, 255, 1024, u16::MAX] {
        let _ = engine.hint_glyph(gid);
    }
});
