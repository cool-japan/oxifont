//! Integration tests for `oxifont-subset`.
//!
//! Uses the Noto Sans Regular TTF fixture from `oxifont-parser`.

use std::collections::BTreeSet;

/// Load the test font fixture shared with `oxifont-parser`.
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn load_test_font() -> &'static [u8] {
    FIXTURE_BYTES
}

// ---------------------------------------------------------------------------
// Test 1: subset produces a parseable font
// ---------------------------------------------------------------------------

#[test]
fn subset_produces_parseable_font() {
    let font_data = load_test_font();
    let codepoints: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();
    let result =
        oxifont_subset::subset_font(font_data, &codepoints).expect("subset should succeed");

    let face = ttf_parser::Face::parse(&result, 0).expect("re-parse of subset font should succeed");

    // Should have exactly 4 glyphs: .notdef + A + B + C
    assert_eq!(
        face.number_of_glyphs(),
        4,
        "Expected .notdef + 3 glyphs, got {}",
        face.number_of_glyphs()
    );
}

// ---------------------------------------------------------------------------
// Test 2: cmap resolves requested codepoints
// ---------------------------------------------------------------------------

#[test]
fn subset_cmap_resolves_codepoints() {
    let font_data = load_test_font();
    let codepoints: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();
    let result =
        oxifont_subset::subset_font(font_data, &codepoints).expect("subset should succeed");

    let face = ttf_parser::Face::parse(&result, 0).expect("re-parse of subset font should succeed");

    for cp in ['A', 'B', 'C'] {
        let gid = face.glyph_index(cp);
        assert!(gid.is_some(), "cmap should resolve '{}' in subset font", cp);
    }
}

// ---------------------------------------------------------------------------
// Test 3: Unrequested codepoints are not present
// ---------------------------------------------------------------------------

#[test]
fn subset_excludes_unrequested_glyphs() {
    let font_data = load_test_font();
    let codepoints: BTreeSet<char> = ['A'].iter().copied().collect();
    let result =
        oxifont_subset::subset_font(font_data, &codepoints).expect("subset should succeed");

    let face = ttf_parser::Face::parse(&result, 0).expect("re-parse of subset font should succeed");

    // Total glyph count should be very small (.notdef + 'A' + any composites)
    assert!(
        face.number_of_glyphs() <= 5,
        "Expected very few glyphs after subsetting to just 'A', got {}",
        face.number_of_glyphs()
    );
}

// ---------------------------------------------------------------------------
// Test 4: Empty codepoint set produces a font with only .notdef
// ---------------------------------------------------------------------------

#[test]
fn subset_empty_codepoints_produces_notdef_only() {
    let font_data = load_test_font();
    let codepoints: BTreeSet<char> = BTreeSet::new();
    let result = oxifont_subset::subset_font(font_data, &codepoints)
        .expect("subset with empty codepoints should succeed");

    let face =
        ttf_parser::Face::parse(&result, 0).expect("re-parse of notdef-only subset should succeed");

    assert_eq!(
        face.number_of_glyphs(),
        1,
        "Empty codepoints should produce only .notdef, got {}",
        face.number_of_glyphs()
    );
}

// ---------------------------------------------------------------------------
// Test 5: Subset output size is smaller than the original
// ---------------------------------------------------------------------------

#[test]
fn subset_output_is_smaller_than_original() {
    let font_data = load_test_font();
    let codepoints: BTreeSet<char> = ['H', 'e', 'l', 'o'].iter().copied().collect();
    let result =
        oxifont_subset::subset_font(font_data, &codepoints).expect("subset should succeed");

    assert!(
        result.len() < font_data.len(),
        "Subset font ({} bytes) should be smaller than original ({} bytes)",
        result.len(),
        font_data.len()
    );
}
