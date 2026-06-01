//! Feature-matrix and compile-time API surface tests for the `oxifont` facade
//! crate.
//!
//! These tests verify that:
//! - Core types from `oxifont-core` are always accessible through the facade.
//! - The `version()` function returns a well-formed semver-like string.
//! - Feature-gated modules expose the expected encode/decode/subset API when
//!   the corresponding Cargo feature is active.
//! - The `detect_format` API identifies SFNT magic bytes correctly at all times.
//!
//! All tests are compile-time or trivially fast; none perform I/O or require
//! system fonts to be present.

// ---------------------------------------------------------------------------
// Core types — no feature gate (always present)
// ---------------------------------------------------------------------------

/// Verify that the canonical core types are accessible as zero-sized type
/// checks. This test effectively compiles the entire public type surface of
/// `oxifont-core` through the `oxifont` facade and will fail at compile time
/// if any re-export is removed or renamed.
#[test]
fn test_core_types_accessible() {
    let _ = std::mem::size_of::<oxifont::FontStyle>();
    let _ = std::mem::size_of::<oxifont::FontStretch>();
    let _ = std::mem::size_of::<oxifont::FontMetrics>();
    let _ = std::mem::size_of::<oxifont::GlyphOutline>();
    let _ = std::mem::size_of::<oxifont::FontFormat>();
    let _ = std::mem::size_of::<oxifont::KerningPair>();
    let _ = std::mem::size_of::<oxifont::VariationAxis>();
    let _ = std::mem::size_of::<oxifont::ColorGlyphFormat>();
    let _ = std::mem::size_of::<oxifont::FontError>();
    let _ = std::mem::size_of::<oxifont::FaceInfo>();
    let _ = std::mem::size_of::<oxifont::FontQuery>();
}

// ---------------------------------------------------------------------------
// version()
// ---------------------------------------------------------------------------

/// `version()` must return a non-empty string that contains at least one dot,
/// consistent with a semver-like `"MAJOR.MINOR.PATCH"` format.
#[test]
fn test_version_returns_nonempty_string() {
    let v = oxifont::version();
    assert!(!v.is_empty(), "version() must return a non-empty string");
    assert!(
        v.contains('.'),
        "version should contain at least one dot: {v}"
    );
}

// ---------------------------------------------------------------------------
// detect_format — always present
// ---------------------------------------------------------------------------

/// `detect_format` must correctly classify the four standard SFNT magic
/// sequences and report `Unknown` for unrecognised bytes.
#[test]
fn test_detect_format_api() {
    use oxifont::FontFormat;

    // TrueType (0x00010000)
    let tt = [0x00u8, 0x01, 0x00, 0x00];
    assert_eq!(oxifont::detect_format(&tt), FontFormat::TrueType);

    // OpenType/CFF (OTTO)
    let cff = b"OTTO";
    assert_eq!(oxifont::detect_format(cff), FontFormat::OpenType);

    // TrueType Collection (ttcf)
    let ttc = b"ttcf";
    assert_eq!(oxifont::detect_format(ttc), FontFormat::TrueTypeCollection);

    // WOFF1 (wOFF)
    let w1 = b"wOFF";
    assert_eq!(oxifont::detect_format(w1), FontFormat::Woff1);

    // WOFF2 (wOF2)
    let w2 = b"wOF2";
    assert_eq!(oxifont::detect_format(w2), FontFormat::Woff2);

    // Unknown / too short
    assert_eq!(oxifont::detect_format(&[]), FontFormat::Unknown);
    assert_eq!(oxifont::detect_format(&[0xDE, 0xAD]), FontFormat::Unknown);
    assert_eq!(oxifont::detect_format(&[0xFF; 4]), FontFormat::Unknown);
}

// ---------------------------------------------------------------------------
// WOFF1 feature gate
// ---------------------------------------------------------------------------

/// When the `woff1` feature is active the `oxifont::webfont` module must
/// expose `encode_woff1` and `decode_woff1`. Feeding them invalid data is
/// fine — the test only checks that the functions are callable through the
/// facade's feature-gated re-export path.
#[cfg(feature = "woff1")]
#[test]
fn test_woff1_feature_exposes_encode_decode() {
    // 12-byte payload — not a real SFNT but enough to exercise the call path.
    let data: Vec<u8> = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    // Both encode and decode may fail on this stub data — that is acceptable.
    let _encode_result = oxifont::webfont::encode_woff1(&data);
    let _decode_result = oxifont::webfont::decode_woff1(&data);
}

// ---------------------------------------------------------------------------
// WOFF2 feature gate
// ---------------------------------------------------------------------------

/// When the `woff2` feature is active the `oxifont::webfont` module must
/// expose `encode_woff2` and `decode_woff2`. Both may fail on stub data.
#[cfg(feature = "woff2")]
#[test]
fn test_woff2_feature_exposes_encode_decode() {
    let data: Vec<u8> = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let _encode_result = oxifont::webfont::encode_woff2(&data);
    let _decode_result = oxifont::webfont::decode_woff2(&data);
}

// ---------------------------------------------------------------------------
// subset feature gate
// ---------------------------------------------------------------------------

/// When the `subset` feature is active `oxifont::subset::subset_font` must be
/// callable. Feeding it invalid data is fine — the call path is what matters.
#[cfg(feature = "subset")]
#[test]
fn test_subset_feature_exposes_subset_font() {
    use std::collections::BTreeSet;

    let data: Vec<u8> = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let codepoints: BTreeSet<char> = "A".chars().collect();
    // Failure on invalid data is expected and acceptable.
    let _result = oxifont::subset::subset_font(&data, &codepoints);
}

// ---------------------------------------------------------------------------
// discovery feature gate
// ---------------------------------------------------------------------------

/// When the `discovery` feature is active the `oxifont::discovery` module must
/// expose `system_font_dirs` and `scan_dirs`. The function is only called with
/// an empty slice to avoid any I/O, so the test always passes in CI.
#[cfg(feature = "discovery")]
#[test]
fn test_discovery_feature_exposes_scan_api() {
    // system_font_dirs is a pure function (no I/O) — call it to verify the
    // symbol is reachable through the facade.
    let _dirs = oxifont::discovery::system_font_dirs();

    // scan_dirs on an empty slice returns an empty vec — no I/O.
    let empty: &[std::path::PathBuf] = &[];
    let faces = oxifont::discovery::scan_dirs(empty);
    assert!(
        faces.is_empty(),
        "scan_dirs on empty slice must return empty Vec"
    );
}

// ---------------------------------------------------------------------------
// db feature gate — FontDatabase::new()
// ---------------------------------------------------------------------------

/// When the `db` feature is active the `oxifont::db` module must expose
/// `FontDatabase`. Constructing an empty database is a compile-time check.
#[cfg(feature = "db")]
#[test]
fn test_db_feature_exposes_font_database() {
    let db = oxifont::db::FontDatabase::new();
    // An empty database has no faces.
    drop(db);
}

// ---------------------------------------------------------------------------
// Prelude re-exports
// ---------------------------------------------------------------------------

/// Verify that `use oxifont::prelude::*` brings in `FontQuery`, `FontStyle`,
/// and `FontStretch` without name conflicts.
#[test]
fn test_prelude_imports_work() {
    use oxifont::prelude::*;
    let _q = FontQuery::new().family("test-family");
    let _s = FontStyle::Italic;
    let _st = FontStretch::Condensed;
    let _e = FontError::UnsupportedFormat;
}

// ---------------------------------------------------------------------------
// parser module re-export
// ---------------------------------------------------------------------------

/// The `oxifont::parser` module must re-export `ParsedFace` unconditionally.
#[test]
fn test_parser_module_reexport() {
    // This is a compile-time check: if the import fails the test fails.
    use oxifont::parser::ParsedFace;
    let _ = std::mem::size_of::<ParsedFace>();
}
