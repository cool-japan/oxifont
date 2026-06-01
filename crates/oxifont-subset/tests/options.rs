//! Tests for [`SubsetOptions`], [`SubsetStats`], [`subset_by_gids`], and preset functions.

use std::collections::BTreeSet;

use oxifont_subset::{
    subset_by_gids, subset_font, subset_font_for_pdf, subset_font_for_web,
    subset_font_with_options, SubsetOptions,
};

static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn load_test_font() -> &'static [u8] {
    FIXTURE_BYTES
}

// ---------------------------------------------------------------------------
// Helper: check that a 4-byte tag is (or is not) present in an SFNT buffer.
// ---------------------------------------------------------------------------

fn sfnt_has_table(data: &[u8], tag: &[u8; 4]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 4 > data.len() {
            break;
        }
        if &data[base..base + 4] == tag {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// test_subset_options_default
// ---------------------------------------------------------------------------

/// Default options must produce the same byte output as the plain `subset_font`.
#[test]
fn test_subset_options_default() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();

    let plain = subset_font(font, &cps).expect("plain subset_font failed");

    let opts = SubsetOptions::default();
    let (with_opts, _stats) =
        subset_font_with_options(font, &cps, &opts).expect("subset_font_with_options failed");

    assert_eq!(
        plain, with_opts,
        "default options must produce identical output to subset_font"
    );
}

// ---------------------------------------------------------------------------
// test_strip_hints
// ---------------------------------------------------------------------------

/// With `strip_hints = true`, `fpgm`, `prep`, and `cvt ` must be absent.
#[test]
fn test_strip_hints() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A', 'B'].iter().copied().collect();

    let opts = SubsetOptions::default().strip_hints(true);
    let (result, _stats) =
        subset_font_with_options(font, &cps, &opts).expect("strip_hints subset failed");

    assert!(
        !sfnt_has_table(&result, b"fpgm"),
        "fpgm must be absent when strip_hints=true"
    );
    assert!(
        !sfnt_has_table(&result, b"prep"),
        "prep must be absent when strip_hints=true"
    );
    assert!(
        !sfnt_has_table(&result, b"cvt "),
        "cvt  must be absent when strip_hints=true"
    );

    // Sanity: font is still parseable.
    ttf_parser::Face::parse(&result, 0).expect("strip_hints result must be parseable");
}

// ---------------------------------------------------------------------------
// test_strip_hints_false_preserves_tables
// ---------------------------------------------------------------------------

/// With `strip_hints = false` (default), hint tables present in the source
/// must be preserved.
#[test]
fn test_strip_hints_false_preserves_tables() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A'].iter().copied().collect();

    // Check whether the test fixture actually has hint tables.
    let has_fpgm = sfnt_has_table(font, b"fpgm");
    let has_prep = sfnt_has_table(font, b"prep");
    let has_cvt = sfnt_has_table(font, b"cvt ");

    let opts = SubsetOptions::default().strip_hints(false);
    let (result, _stats) =
        subset_font_with_options(font, &cps, &opts).expect("no-strip_hints subset failed");

    // Tables present in the source must also be present in the output.
    if has_fpgm {
        assert!(
            sfnt_has_table(&result, b"fpgm"),
            "fpgm must be retained when strip_hints=false"
        );
    }
    if has_prep {
        assert!(
            sfnt_has_table(&result, b"prep"),
            "prep must be retained when strip_hints=false"
        );
    }
    if has_cvt {
        assert!(
            sfnt_has_table(&result, b"cvt "),
            "cvt  must be retained when strip_hints=false"
        );
    }
}

// ---------------------------------------------------------------------------
// test_retain_layout_false
// ---------------------------------------------------------------------------

/// With `retain_layout_tables = false`, GSUB/GPOS/GDEF must be absent.
#[test]
fn test_retain_layout_false() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A', 'B'].iter().copied().collect();

    let opts = SubsetOptions::default().retain_layout_tables(false);
    let (result, _stats) =
        subset_font_with_options(font, &cps, &opts).expect("retain_layout=false subset failed");

    assert!(
        !sfnt_has_table(&result, b"GSUB"),
        "GSUB must be absent when retain_layout_tables=false"
    );
    assert!(
        !sfnt_has_table(&result, b"GPOS"),
        "GPOS must be absent when retain_layout_tables=false"
    );
    assert!(
        !sfnt_has_table(&result, b"GDEF"),
        "GDEF must be absent when retain_layout_tables=false"
    );

    // Font must still be parseable.
    ttf_parser::Face::parse(&result, 0).expect("retain_layout=false result must be parseable");
}

// ---------------------------------------------------------------------------
// test_subset_by_gids
// ---------------------------------------------------------------------------

/// `subset_by_gids` must produce a valid SFNT without a cmap codepoint scan.
#[test]
fn test_subset_by_gids() {
    let font = load_test_font();

    // GID 0 (.notdef) + GIDs 1, 2, 3 (arbitrary).
    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0);
    gids.insert(1);
    gids.insert(2);
    gids.insert(3);

    let result = subset_by_gids(font, &gids).expect("subset_by_gids failed");

    let face =
        ttf_parser::Face::parse(&result, 0).expect("subset_by_gids result must be parseable");

    // Must contain at least the requested GIDs (plus any composites).
    assert!(
        face.number_of_glyphs() >= gids.len() as u16,
        "Expected at least {} glyphs, got {}",
        gids.len(),
        face.number_of_glyphs()
    );
}

// ---------------------------------------------------------------------------
// test_subset_font_for_web
// ---------------------------------------------------------------------------

/// `subset_font_for_web` must strip hint tables and produce a parseable font.
#[test]
fn test_subset_font_for_web() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['H', 'e', 'l', 'o'].iter().copied().collect();

    let result = subset_font_for_web(font, &cps).expect("subset_font_for_web failed");

    // Hint tables must be absent.
    assert!(
        !sfnt_has_table(&result, b"fpgm"),
        "fpgm must be absent in web subset"
    );
    assert!(
        !sfnt_has_table(&result, b"prep"),
        "prep must be absent in web subset"
    );
    assert!(
        !sfnt_has_table(&result, b"cvt "),
        "cvt  must be absent in web subset"
    );

    // Font must be smaller than the original.
    assert!(
        result.len() < font.len(),
        "Web subset ({} bytes) must be smaller than original ({} bytes)",
        result.len(),
        font.len()
    );

    ttf_parser::Face::parse(&result, 0).expect("web subset result must be parseable");
}

// ---------------------------------------------------------------------------
// test_subset_font_for_pdf
// ---------------------------------------------------------------------------

/// `subset_font_for_pdf` must retain hint tables (if present) and produce a
/// parseable font.
#[test]
fn test_subset_font_for_pdf() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();

    let result = subset_font_for_pdf(font, &cps).expect("subset_font_for_pdf failed");

    ttf_parser::Face::parse(&result, 0).expect("pdf subset result must be parseable");

    // Hint tables present in the source must survive.
    if sfnt_has_table(font, b"fpgm") {
        assert!(
            sfnt_has_table(&result, b"fpgm"),
            "fpgm must be retained in pdf subset"
        );
    }
    if sfnt_has_table(font, b"prep") {
        assert!(
            sfnt_has_table(&result, b"prep"),
            "prep must be retained in pdf subset"
        );
    }
}

// ---------------------------------------------------------------------------
// test_subset_stats
// ---------------------------------------------------------------------------

/// `SubsetStats` fields must be populated correctly.
#[test]
fn test_subset_stats() {
    let font = load_test_font();
    let cps: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();

    let opts = SubsetOptions::default();
    let (result, stats) = subset_font_with_options(font, &cps, &opts).expect("stats subset failed");

    assert_eq!(
        stats.original_size,
        font.len(),
        "original_size must match font length"
    );
    assert_eq!(
        stats.subset_size,
        result.len(),
        "subset_size must match result length"
    );
    // .notdef + A + B + C = 4 glyphs minimum.
    assert!(
        stats.glyphs_retained >= 4,
        "Expected at least 4 glyphs retained, got {}",
        stats.glyphs_retained
    );
    // Must include the core rewritten tables.
    let core_tags: &[[u8; 4]] = &[*b"glyf", *b"loca", *b"cmap", *b"hmtx", *b"head"];
    for tag in core_tags {
        assert!(
            stats.tables_retained.contains(tag),
            "tables_retained must include {}",
            std::str::from_utf8(tag).unwrap_or("????")
        );
    }
    assert!(
        stats.subset_size < stats.original_size,
        "subset_size ({}) should be less than original_size ({})",
        stats.subset_size,
        stats.original_size
    );
}

// ---------------------------------------------------------------------------
// test_retain_codepoint_range
// ---------------------------------------------------------------------------

/// `retain_codepoint_range` must exclude codepoints outside [lo, hi].
#[test]
fn test_retain_codepoint_range() {
    let font = load_test_font();

    // Request A–Z and digits 0–9, but restrict range to ASCII uppercase only.
    let mut cps: BTreeSet<char> = ('A'..='Z').collect();
    for c in '0'..='9' {
        cps.insert(c);
    }

    let opts = SubsetOptions::default().retain_codepoint_range('A', 'Z');
    let (result, _stats) =
        subset_font_with_options(font, &cps, &opts).expect("retain_codepoint_range subset failed");

    let face = ttf_parser::Face::parse(&result, 0)
        .expect("retain_codepoint_range result must be parseable");

    // Digits should not be mapped in the cmap.
    for c in '0'..='9' {
        assert!(
            face.glyph_index(c).is_none(),
            "digit '{}' must not be in the cmap after range restriction",
            c
        );
    }

    // Uppercase letters should be mapped (assuming they exist in the test font).
    // We test at least one to confirm the range was applied.
    let found_upper = ('A'..='Z').any(|c| face.glyph_index(c).is_some());
    assert!(
        found_upper,
        "At least one uppercase letter must be mapped after range restriction"
    );
}
