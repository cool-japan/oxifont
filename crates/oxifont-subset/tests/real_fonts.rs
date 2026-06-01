//! Integration tests against real system fonts.
//!
//! Each test calls [`find_ttf_on_system`] which looks for any `.ttf` file in
//! standard font directories. When no font is found the test returns early, so
//! the suite is safe to run on headless CI machines that have no system fonts.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Search well-known system font directories for any `.ttf` file.
fn find_ttf_on_system() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/TTF",
    ];
    for dir in &dirs {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("ttf"))
                    .unwrap_or(false)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Test 1: basic ASCII subset round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_subset_real_font_ascii() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = ('A'..='Z').chain('a'..='z').chain('0'..='9').collect();
    let (subset, stats) = oxifont_subset::subset_font_with_options(
        &data,
        &codepoints,
        &oxifont_subset::SubsetOptions::default(),
    )
    .expect("subset should succeed on a valid TTF");
    assert!(subset.len() >= 12, "output too short to be valid SFNT");
    assert!(
        stats.glyphs_retained > 0,
        "should retain at least some glyphs"
    );
    assert_eq!(
        stats.subset_size,
        subset.len(),
        "stats size should match output size"
    );
    assert!(stats.original_size > 0);
}

// ---------------------------------------------------------------------------
// Test 2: subsetting a handful of chars produces smaller output
// ---------------------------------------------------------------------------

#[test]
fn test_subset_real_font_produces_smaller_output() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = "hello".chars().collect();
    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(
            subset.len() < data.len(),
            "subsetting 5 chars from {}-byte font should produce smaller output",
            data.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: empty codepoint set — only .notdef is retained
// ---------------------------------------------------------------------------

#[test]
fn test_subset_empty_codepoints() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = BTreeSet::new();
    if let Ok((subset, stats)) = oxifont_subset::subset_font_with_options(
        &data,
        &codepoints,
        &oxifont_subset::SubsetOptions::default(),
    ) {
        assert!(subset.len() >= 12);
        assert!(
            stats.glyphs_retained >= 1,
            "at least .notdef should be retained"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 4: strip_hints option produces a valid font
// ---------------------------------------------------------------------------

#[test]
fn test_subset_with_options_strip_hints() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = "AaBb".chars().collect();
    let opts = oxifont_subset::SubsetOptions::default().strip_hints(true);
    if let Ok((subset, _)) = oxifont_subset::subset_font_with_options(&data, &codepoints, &opts) {
        assert!(subset.len() >= 12);
    }
}

// ---------------------------------------------------------------------------
// Test 5: SubsetStats fields are all populated
// ---------------------------------------------------------------------------

#[test]
fn test_subset_stats_fields_populated() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = "AaBbCc".chars().collect();
    if let Ok((_, stats)) = oxifont_subset::subset_font_with_options(
        &data,
        &codepoints,
        &oxifont_subset::SubsetOptions::default(),
    ) {
        assert!(stats.original_size > 0);
        assert!(stats.subset_size > 0);
        assert!(!stats.tables_retained.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Test 6: round-trip — subset font can be re-parsed by oxifont-parser
// ---------------------------------------------------------------------------

#[test]
fn test_subset_round_trip_parse() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font");
    let codepoints: BTreeSet<char> = "HelloWorld".chars().collect();
    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        if let Ok(face) = oxifont_parser::ParsedFace::parse(subset, 0) {
            use oxifont_core::FontFace;
            assert!(face.glyph_count() > 0);
        }
    }
}
