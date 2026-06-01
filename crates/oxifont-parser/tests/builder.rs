//! Integration tests for `ParsedFaceBuilder`.
//!
//! Uses the same TTF fixture as `parse.rs` (Noto Sans Regular) included at
//! compile time. The builder tests verify both successful construction paths
//! and expected error paths when called with invalid input.

use oxifont_core::FontFace as _;
use oxifont_parser::ParsedFace;

/// Fixture bytes compiled in at test time (same fixture as parse.rs).
static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

#[test]
fn test_builder_default_face_index() {
    let face = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .build()
        .expect("builder with defaults must succeed on valid TTF");

    // Default face_index is 0.
    assert_eq!(
        face.as_face_info().face_index,
        0,
        "default face_index must be 0"
    );
    assert!(
        !face.family_name().is_empty(),
        "family name must not be empty"
    );
}

#[test]
fn test_builder_with_face_index() {
    // The fixture is a plain TTF with exactly one face (index 0).
    // Requesting index 1 must return an error (IndexOutOfBounds or ParseError).
    let result = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .face_index(1)
        .build();

    assert!(
        result.is_err(),
        "face_index=1 on a single-face TTF must return Err"
    );
}

#[test]
fn test_builder_with_variation() {
    // Build with a valid variation tag. The fixture is not a variable font so
    // the setting is stored but does not alter the underlying binary data.
    let face = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .variation("wght", 700.0)
        .build()
        .expect("builder must succeed on valid TTF fixture");

    let settings = face.variation_settings();
    assert_eq!(settings.len(), 1, "one variation setting must be stored");
    assert_eq!(settings[0].0, *b"wght", "tag must be b\"wght\"");
    assert!(
        (settings[0].1 - 700.0_f32).abs() < f32::EPSILON,
        "value must be 700.0"
    );
}

#[test]
fn test_builder_empty_data_fails() {
    // Empty data is structurally invalid — build() must return Err.
    let result = ParsedFace::builder(vec![]).build();
    assert!(result.is_err(), "empty data must produce a parse error");
}

#[test]
fn test_builder_invalid_tag_fails() {
    // Tags with non-ASCII bytes must be rejected by build().
    let result = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .variation("wégh", 700.0) // 'é' is non-ASCII
        .build();

    assert!(
        result.is_err(),
        "non-ASCII variation tag must produce a parse error from build()"
    );
}

#[test]
fn test_builder_tag_padding() {
    // Short tag "wg" should be padded to b"wg  " (two trailing spaces).
    // The fixture has no fvar table, so the setting is stored without
    // actually influencing the parsed font.
    let face = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .variation("wg", 400.0)
        .build()
        .expect("short ASCII tag must be accepted");

    let settings = face.variation_settings();
    assert_eq!(settings.len(), 1, "one variation setting expected");
    assert_eq!(
        settings[0].0, *b"wg  ",
        "tag must be padded with trailing spaces to four bytes"
    );
}

#[test]
fn test_builder_tag_truncation() {
    // Tag "wghtX" (5 chars) should be truncated to b"wght".
    let face = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .variation("wghtX", 400.0)
        .build()
        .expect("truncated ASCII tag must be accepted");

    let settings = face.variation_settings();
    assert_eq!(settings.len(), 1, "one variation setting expected");
    assert_eq!(
        settings[0].0, *b"wght",
        "tag must be truncated to four bytes"
    );
}

#[test]
fn test_builder_chained_variations() {
    // Multiple variation calls accumulate settings in order.
    let face = ParsedFace::builder(FIXTURE_BYTES.to_vec())
        .variation("wght", 700.0)
        .variation("wdth", 75.0)
        .build()
        .expect("two variation settings must succeed");

    let settings = face.variation_settings();
    assert_eq!(settings.len(), 2, "two variation settings expected");
    assert_eq!(settings[0].0, *b"wght");
    assert_eq!(settings[1].0, *b"wdth");
}

#[test]
fn test_builder_invalid_sfnt_fails() {
    // Data that looks like 12 bytes of zeros is structurally invalid SFNT —
    // build() must return Err.
    let result = ParsedFace::builder(vec![0u8; 12]).build();
    assert!(
        result.is_err(),
        "zero-filled data must produce a parse error"
    );
}
