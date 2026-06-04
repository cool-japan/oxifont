//! Fuzz target: feed arbitrary bytes into `ParsedFace::parse`.
//!
//! Invariants verified:
//!   - `ParsedFace::parse` never panics on arbitrary input.
//!   - If parsing succeeds, trait methods do not panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oxifont_core::FontFace as _;

fuzz_target!(|data: &[u8]| {
    // Try face index 0 (most common).
    if let Ok(face) = oxifont_parser::ParsedFace::parse(data.to_vec(), 0) {
        // Verify core trait methods do not panic.
        let _ = face.family_name();
        let gc = face.glyph_count();
        let _ = face.units_per_em();
        let _ = face.metrics();
        let _ = face.style();
        let _ = face.weight();
        let _ = face.is_monospace();
        let _ = face.axes();
        // Outline extraction for a few GIDs.
        for gid in [0u16, 1, 2, 100] {
            if gid < gc {
                let _ = face.outline(gid);
                let _ = face.advance_width(gid);
            }
        }
        // Color glyph query.
        let _ = face.color_glyph_format();
        // PostScript name.
        let _ = face.postscript_name();
    }
    // Also try face index 1 for potential TTC fixtures.
    if data.len() >= 12 {
        let _ = oxifont_parser::ParsedFace::parse(data.to_vec(), 1);
    }
});
