//! Fuzz target: drive glyph outline decoding over arbitrary font bytes.
//!
//! `FontProgram::glyph_points` parses attacker-controlled `glyf`/`loca` data,
//! resolving composite components recursively (bounded depth) and point-matched
//! offsets. This target checks it never panics or infinitely recurses on
//! hostile input.
//!
//! Invariants:
//!   - `FontProgram::load` and `glyph_points` never panic on arbitrary input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oxifont_core::sfnt::SfntTableMap;
use oxifont_hinting::FontProgram;

fuzz_target!(|data: &[u8]| {
    let map = match SfntTableMap::parse(data) {
        Ok(m) => m,
        Err(_) => return,
    };

    let program = match FontProgram::load(&map) {
        Ok(p) => p,
        Err(_) => return,
    };

    for gid in [0u16, 1, 2, 5, 17, 128, 4096, u16::MAX] {
        let _ = program.glyph_points(gid);
    }
});
