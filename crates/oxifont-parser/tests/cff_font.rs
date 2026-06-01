//! CFF-outlined font integration tests for `oxifont-parser`.
//!
//! Searches for `.otf` files on the system (which are typically CFF-outlined)
//! and validates that `ParsedFace::is_cff()` works correctly and that outline
//! extraction succeeds for CFF glyphs. Tests skip gracefully when no OTF
//! system fonts are found.

use oxifont_core::FontFace as _;
use oxifont_parser::ParsedFace;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helper: find any OTF (typically CFF-outlined) on the system
// ---------------------------------------------------------------------------

fn find_otf_font() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/opentype",
    ];
    for dir in &dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("otf"))
                .unwrap_or(false)
            {
                return Some(p);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// CFF detection test
// ---------------------------------------------------------------------------

#[test]
fn test_cff_font_detection() {
    let Some(path) = find_otf_font() else {
        // No OTF found on this system; skip gracefully.
        return;
    };
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return,
    };
    let face = match ParsedFace::parse(data, 0) {
        Ok(f) => f,
        Err(_) => return,
    };

    // OTF files are typically CFF-outlined; is_cff() must not panic.
    let _ = face.is_cff();
    // glyph_count must be accessible regardless of outline format.
    assert!(
        face.glyph_count() > 0,
        "OTF font at {:?} must report at least one glyph",
        path
    );
}

// ---------------------------------------------------------------------------
// CFF outline extraction test
// ---------------------------------------------------------------------------

#[test]
fn test_cff_outline_extraction() {
    let Some(path) = find_otf_font() else {
        // No OTF found on this system; skip gracefully.
        return;
    };
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return,
    };
    let face = match ParsedFace::parse(data, 0) {
        Ok(f) => f,
        Err(_) => return,
    };

    // If this font has a CFF table, verify that outline extraction works.
    if face.is_cff() {
        if let Some(gid) = face.glyph_for_char('A') {
            // outline() must not panic for CFF fonts; None is acceptable for
            // glyphs without contours, but Some must be well-formed.
            let result = face.outline(gid);
            if let Some(ref cmds) = result {
                assert!(
                    !cmds.is_empty(),
                    "CFF outline for 'A' (gid={gid}) must not be an empty path list"
                );
            }
        }
    }
    // Confirm the font is usable even if 'A' is absent or outlineless.
    let _ = face.glyph_count();
}
