//! Variable font axis extraction tests for `oxifont-parser`.
//!
//! Searches for a system-installed variable font at runtime and validates the
//! `is_variable()` flag and `axes()` output. Tests skip gracefully when no
//! variable font is found (e.g. on minimal CI images without TTF fonts).

use oxifont_core::FontFace as _;
use oxifont_parser::ParsedFace;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helper: find a variable font on the system
// ---------------------------------------------------------------------------

/// Returns the path to a variable TTF/OTF font found on the local system, or
/// `None` when none can be located.
///
/// The search order is:
/// 1. Any TTF/OTF whose filename contains a variable-font hint (fast path).
/// 2. Any TTF/OTF that parses successfully and whose `is_variable()` flag is
///    `true` (slower, but catches fonts like `SFNS.ttf` that are variable
///    despite having no hint in their filename).
fn find_variable_font() -> Option<PathBuf> {
    let search_dirs = [
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/opentype",
    ];

    let is_ttf_otf = |p: &std::path::Path| {
        p.extension()
            .and_then(|x| x.to_str())
            .map(|x| matches!(x.to_lowercase().as_str(), "ttf" | "otf"))
            .unwrap_or(false)
    };

    let has_variable_hint = |p: &std::path::Path| {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        name.contains("variable")
            || name.ends_with("-vf.ttf")
            || name.ends_with("-vf.otf")
            || name.contains("-var.")
    };

    // Pass 1: name-hinted candidates (fast).
    for dir in &search_dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if is_ttf_otf(&p) && has_variable_hint(&p) {
                return Some(p);
            }
        }
    }

    // Pass 2: parse every TTF/OTF and check is_variable() (handles fonts whose
    // names carry no hint, such as Apple's SFNS.ttf / NewYork.ttf).
    for dir in &search_dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !is_ttf_otf(&p) {
                continue;
            }
            if let Ok(data) = std::fs::read(&p) {
                if let Ok(face) = ParsedFace::parse(data, 0) {
                    if face.is_variable() {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

/// Returns the path to a static (non-variable) TTF/OTF font, or `None`.
fn find_static_font() -> Option<PathBuf> {
    let search_dirs = [
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts",
    ];

    let is_ttf_otf = |p: &std::path::Path| {
        p.extension()
            .and_then(|x| x.to_str())
            .map(|x| matches!(x.to_lowercase().as_str(), "ttf" | "otf"))
            .unwrap_or(false)
    };

    for dir in &search_dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !is_ttf_otf(&p) {
                continue;
            }
            if let Ok(data) = std::fs::read(&p) {
                if let Ok(face) = ParsedFace::parse(data, 0) {
                    if !face.is_variable() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_variable_font_axis_extraction() {
    let Some(path) = find_variable_font() else {
        // No variable font found on this system — skip gracefully.
        return;
    };

    let data = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("failed to read variable font at {path:?}: {e}"));
    let face = ParsedFace::parse(data, 0)
        .unwrap_or_else(|e| panic!("failed to parse variable font at {path:?}: {e}"));

    assert!(
        face.is_variable(),
        "font at {path:?} should be detected as variable"
    );

    let axes = face.axes();
    assert!(
        !axes.is_empty(),
        "variable font at {path:?} should expose at least one variation axis"
    );

    // Validate each axis has a sane min ≤ default ≤ max range.
    for axis in axes {
        let tag = core::str::from_utf8(&axis.tag).unwrap_or("????");
        assert!(
            axis.min_value <= axis.default_value,
            "axis '{tag}' in {path:?}: min ({}) must be ≤ default ({})",
            axis.min_value,
            axis.default_value,
        );
        assert!(
            axis.default_value <= axis.max_value,
            "axis '{tag}' in {path:?}: default ({}) must be ≤ max ({})",
            axis.default_value,
            axis.max_value,
        );
    }

    // For the weight axis (wght), validate the CSS range if present.
    for axis in axes {
        if &axis.tag == b"wght" {
            assert!(
                axis.min_value >= 1.0,
                "wght min ({}) must be ≥ 1 in {path:?}",
                axis.min_value
            );
            assert!(
                axis.max_value <= 1000.0,
                "wght max ({}) must be ≤ 1000 in {path:?}",
                axis.max_value
            );
        }
    }
}

#[test]
fn test_non_variable_font_has_no_axes() {
    let Some(path) = find_static_font() else {
        // No static font found — skip gracefully.
        return;
    };

    let data = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("failed to read static font at {path:?}: {e}"));
    let face = ParsedFace::parse(data, 0)
        .unwrap_or_else(|e| panic!("failed to parse static font at {path:?}: {e}"));

    assert!(
        !face.is_variable(),
        "static font at {path:?} must not be detected as variable"
    );

    let axes = face.axes();
    assert!(
        axes.is_empty(),
        "static font at {path:?} must expose zero variation axes, got {}",
        axes.len()
    );
}

#[test]
fn test_is_variable_fixture_test_ttf() {
    // The bundled test.ttf fixture is a minimal static font — must not be variable.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test.ttf");
    let data = std::fs::read(path).expect("bundled test.ttf must be readable");
    let face = ParsedFace::parse(data, 0).expect("bundled test.ttf must parse");

    assert!(
        !face.is_variable(),
        "bundled test.ttf fixture must not be a variable font"
    );
    assert!(
        face.axes().is_empty(),
        "bundled test.ttf must have no variation axes"
    );
}

#[test]
fn test_variation_coordinates_returns_none_for_static_font() {
    // ParsedFace::variation_coordinates() must return None for non-variable fonts.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test.ttf");
    let data = std::fs::read(path).expect("bundled test.ttf must be readable");
    let face = ParsedFace::parse(data, 0).expect("bundled test.ttf must parse");

    let result = face.variation_coordinates(&[(*b"wght", 700.0)]);
    assert!(
        result.is_none(),
        "variation_coordinates must return None for a static font"
    );
}
