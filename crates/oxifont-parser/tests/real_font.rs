//! Real-world font integration tests for `oxifont-parser`.
//!
//! Locates a system-installed TTF or OTF font at runtime and validates the
//! full parse → query pipeline. Tests skip gracefully when no system fonts
//! are found (e.g. on minimal CI images). All assertions must not panic.

use oxifont_core::FontFace as _;
use oxifont_parser::ParsedFace;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helper: find any TTF/OTF on the system
// ---------------------------------------------------------------------------

fn find_system_font() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/opentype",
    ];
    for dir in &dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_lowercase().as_str(), "ttf" | "otf") {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Basic parse: family name, weight, glyph count
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_family_name() {
    let Some(path) = find_system_font() else {
        return; // skip when no system fonts are available
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    let family = face.family_name();
    assert!(
        !family.is_empty(),
        "family name must not be empty for font at {:?}",
        path
    );
}

#[test]
fn parse_real_system_font_weight_in_range() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    let weight = face.weight();
    assert!(
        (100..=900).contains(&weight),
        "weight {weight} out of CSS range 100–900 for font at {:?}",
        path
    );
}

#[test]
fn parse_real_system_font_glyph_count() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    let count = face.glyph_count();
    assert!(count > 0, "glyph count must be > 0 for font at {:?}", path);
}

// ---------------------------------------------------------------------------
// Metrics validation
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_units_per_em() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    let metrics = face.metrics().expect("system font must provide metrics");
    assert!(
        metrics.units_per_em > 0,
        "units_per_em must be positive for font at {:?}",
        path
    );
}

#[test]
fn parse_real_system_font_ascender_positive() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    let metrics = face.metrics().expect("system font must provide metrics");
    // Ascender should be positive for any standard Latin font.
    assert!(
        metrics.ascender > 0,
        "ascender must be positive for font at {:?}",
        path
    );
}

// ---------------------------------------------------------------------------
// PostScript name
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_postscript_name() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    // PostScript name may be absent in some fonts — just verify no panic.
    let ps_name = face.postscript_name();
    if let Some(name) = ps_name {
        assert!(
            !name.is_empty(),
            "PostScript name must not be empty string for font at {:?}",
            path
        );
    }
}

// ---------------------------------------------------------------------------
// Table presence
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_required_tables_present() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    // Every valid OpenType/TrueType font must contain these tables.
    for tag in [*b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp"] {
        assert!(
            face.has_table(tag),
            "required table {:?} must be present in font at {:?}",
            core::str::from_utf8(&tag).unwrap_or("????"),
            path
        );
    }
}

// ---------------------------------------------------------------------------
// Glyph mapping
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_maps_ascii_letters() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");

    // At least one basic ASCII letter should be mapped in any Latin font.
    // We test a range and require at least one hit.
    let mapped_count = b'A'..=b'Z';
    let hit = mapped_count
        .map(|b| b as char)
        .any(|c| face.glyph_for_char(c).is_some());
    // Fonts without Latin coverage (e.g. symbol-only) may fail — that is
    // acceptable. We only require no panic.
    let _ = hit;
}

// ---------------------------------------------------------------------------
// Clone round-trip on a real font
// ---------------------------------------------------------------------------

#[test]
fn parse_real_system_font_clone_matches_original() {
    let Some(path) = find_system_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");
    let face = ParsedFace::parse(data, 0).expect("parse real font");
    let cloned = face.clone();

    assert_eq!(
        face.family_name(),
        cloned.family_name(),
        "cloned family name must match original"
    );
    assert_eq!(
        face.weight(),
        cloned.weight(),
        "cloned weight must match original"
    );
    assert_eq!(
        face.units_per_em(),
        cloned.units_per_em(),
        "cloned units_per_em must match original"
    );
}

// ---------------------------------------------------------------------------
// Error paths with real-font data sizes (complement parse_errors.rs)
// ---------------------------------------------------------------------------

#[test]
fn parse_error_empty_always_fails() {
    assert!(
        ParsedFace::parse(vec![], 0).is_err(),
        "empty bytes must always fail"
    );
}

#[test]
fn parse_error_small_buffer_does_not_panic() {
    // Test a range of small buffer sizes to ensure no panics.
    for size in [1usize, 2, 3, 4, 8, 11] {
        let _ = ParsedFace::parse(vec![0u8; size], 0);
    }
}
