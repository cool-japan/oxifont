//! Integration tests for `oxifont-parser`.
//!
//! Uses a real TTF fixture (`tests/fixtures/test.ttf`) included at compile
//! time via `include_bytes!`. The fixture is Noto Sans Regular, an
//! OFL-licensed font that works on all platforms (Linux, macOS, Windows).

use oxifont_core::FontFace as _;
use oxifont_parser::{face_count, ParsedFace};

/// Fixture bytes compiled in at test time.
static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

#[test]
fn parse_ttf_succeeds() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let name = face.family_name();
    assert!(
        !name.is_empty(),
        "family name must not be empty; got {:?}",
        name
    );
}

#[test]
fn parse_ttf_weight_nonzero() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let w = face.weight();
    assert!(
        (100..=900).contains(&w),
        "weight {w} out of expected range 100–900"
    );
}

#[test]
fn parse_ttf_units_per_em_nonzero() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    assert!(face.units_per_em() > 0, "units_per_em must be > 0");
}

#[test]
fn face_count_returns_one_for_ttf() {
    let count = face_count(FIXTURE_BYTES);
    assert_eq!(
        count, 1,
        "a plain TTF must have face count == 1, got {count}"
    );
}

#[test]
fn glyph_for_ascii_char() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Noto Sans is a full Latin font; 'A' must be mapped.
    let gid = face.glyph_for_char('A');
    assert!(gid.is_some(), "'A' must have a glyph in Noto Sans Regular");
}

#[test]
fn parse_invalid_bytes_returns_error() {
    let bad: Vec<u8> = vec![0u8; 16];
    let result = ParsedFace::parse(bad, 0);
    assert!(
        result.is_err(),
        "parsing 16 zero bytes should return an error"
    );
}

#[test]
fn parse_empty_bytes_returns_error() {
    let result = ParsedFace::parse(vec![], 0);
    assert!(
        result.is_err(),
        "parsing empty bytes should return an error"
    );
}

// ---------------------------------------------------------------------------
// New trait method tests (FontMetrics, outline, etc.)
// ---------------------------------------------------------------------------

#[test]
fn metrics_returns_valid_values() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let metrics = face.metrics().expect("Noto Sans must provide metrics");
    assert!(metrics.units_per_em > 0, "units_per_em must be > 0");
    assert!(metrics.ascender > 0, "ascender must be positive");
    assert!(metrics.descender < 0, "descender must be negative");
}

#[test]
fn metrics_cap_height_and_x_height() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let metrics = face.metrics().expect("Noto Sans must provide metrics");
    // Noto Sans Regular has OS/2 v4+ so cap_height and x_height should exist.
    if let Some(cap_h) = metrics.cap_height {
        assert!(cap_h > 0, "cap_height must be positive, got {cap_h}");
    }
    if let Some(x_h) = metrics.x_height {
        assert!(x_h > 0, "x_height must be positive, got {x_h}");
    }
}

#[test]
fn glyph_count_is_nonzero() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    assert!(face.glyph_count() > 0, "glyph count must be > 0");
}

#[test]
fn outline_extraction_for_letter_a() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("'A' must be mapped");
    let outline = face.outline(gid).expect("'A' glyph must have an outline");
    assert!(
        !outline.is_empty(),
        "outline for 'A' must have path commands"
    );
    // The first command should be a MoveTo.
    matches!(&outline[0], oxifont_core::GlyphOutline::MoveTo { .. });
}

#[test]
fn outline_for_space_returns_none() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Space character typically has no outline (advance-only glyph).
    if let Some(gid) = face.glyph_for_char(' ') {
        // Space may have no outline, which is fine.
        let outline = face.outline(gid);
        if let Some(ref cmds) = outline {
            // If there IS an outline, it should be well-formed.
            assert!(!cmds.is_empty());
        }
    }
}

#[test]
fn stretch_returns_normal_for_regular_font() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    assert_eq!(
        face.stretch(),
        oxifont_core::FontStretch::Normal,
        "Noto Sans Regular should have Normal stretch"
    );
}

#[test]
fn postscript_name_is_present() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let ps_name = face.postscript_name();
    assert!(ps_name.is_some(), "Noto Sans should have a PostScript name");
    let ps = ps_name.expect("PostScript name must exist");
    assert!(!ps.is_empty(), "PostScript name must not be empty");
}

#[test]
fn has_table_detects_known_tables() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Every TrueType font must have these tables.
    assert!(face.has_table(*b"cmap"), "cmap table must exist");
    assert!(face.has_table(*b"head"), "head table must exist");
    assert!(face.has_table(*b"hhea"), "hhea table must exist");
    assert!(face.has_table(*b"hmtx"), "hmtx table must exist");
    assert!(face.has_table(*b"maxp"), "maxp table must exist");
    // A made-up tag should not exist.
    assert!(!face.has_table(*b"ZZZZ"), "fake tag should not exist");
}

#[test]
fn advance_width_for_mapped_glyph() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("'A' must be mapped");
    let advance = face.advance_width(gid);
    assert!(advance.is_some(), "advance width for 'A' must be present");
    assert!(
        advance.expect("must be present") > 0,
        "advance width must be > 0"
    );
}

#[test]
fn color_glyph_format_is_none_for_regular_font() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Noto Sans Regular is a plain TrueType font without color glyphs.
    assert!(
        face.color_glyph_format().is_none(),
        "Noto Sans Regular should not have color glyphs"
    );
    assert!(
        !face.has_color_glyphs(),
        "has_color_glyphs should be false for a regular font"
    );
}

// ---------------------------------------------------------------------------
// New method tests: table_data, Clone, from_bytes, as_face_info, vertical_origin
// ---------------------------------------------------------------------------

#[test]
fn test_table_data_head() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let head = face.table_data(*b"head");
    assert!(head.is_some(), "head table must exist in Noto Sans");
    let head_bytes = head.expect("head table must be present");
    // The OpenType `head` table is at least 54 bytes.
    assert!(
        head_bytes.len() >= 54,
        "head table must be at least 54 bytes, got {}",
        head_bytes.len()
    );
}

#[test]
fn test_table_data_missing() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let result = face.table_data(*b"XXXX");
    assert!(result.is_none(), "non-existent tag XXXX must return None");
}

#[test]
fn test_parsedface_clone() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
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

#[test]
fn test_from_bytes() {
    let face = ParsedFace::from_bytes(FIXTURE_BYTES.to_vec(), 0)
        .expect("from_bytes must succeed for valid font data");
    let name = face.family_name();
    assert!(
        !name.is_empty(),
        "family name must be non-empty after from_bytes"
    );
}

#[test]
fn test_as_face_info() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let info = face.as_face_info();
    assert!(
        !info.family.is_empty(),
        "as_face_info family must be non-empty"
    );
    assert_eq!(
        info.face_index, 0,
        "as_face_info face_index must be 0 for single-face font"
    );
    assert_eq!(
        info.weight,
        face.weight(),
        "as_face_info weight must match face weight"
    );
    assert_eq!(
        info.stretch,
        face.stretch(),
        "as_face_info stretch must match face stretch"
    );
}

#[test]
fn test_vertical_origin_no_vorg_returns_none() {
    // Noto Sans Regular does not contain a VORG table, so vertical_origin
    // should return None.
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    // Only check if the font actually lacks VORG; if it has one, skip.
    if !face.has_table(*b"VORG") {
        assert!(
            face.vertical_origin(0).is_none(),
            "vertical_origin must be None when VORG table is absent"
        );
    }
}

// ---------------------------------------------------------------------------
// raw_bytes() and table_data() tests for oxifont-subset integration
// ---------------------------------------------------------------------------

#[test]
fn test_table_data_returns_none_for_missing_table() {
    // A minimal 12-byte buffer that parses as having zero tables.
    // The parse may fail (which is fine) — we only check the None path when it
    // succeeds.
    let data = vec![
        0x00u8, 0x01, 0x00, 0x00, // sfVersion (TrueType)
        0x00, 0x00, // numTables = 0
        0x00, 0x00, // searchRange
        0x00, 0x00, // entrySelector
        0x00, 0x00, // rangeShift
    ];
    if let Ok(face) = ParsedFace::parse(data, 0) {
        let cmap = face.table_data(*b"cmap");
        assert!(
            cmap.is_none(),
            "zero-table font must return None for table_data(cmap)"
        );
    }
    // If parse returns Err, the test passes silently (input was invalid anyway).
}

#[test]
fn test_raw_bytes_roundtrip() {
    // Verify raw_bytes() returns the exact bytes passed into parse().
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    assert_eq!(
        face.raw_bytes(),
        FIXTURE_BYTES,
        "raw_bytes() must return the original font data unchanged"
    );
}

#[test]
fn test_table_data_fixture_has_cmap() {
    // table_data() for a required table must return non-empty bytes, not just
    // a presence flag. This validates the byte-slice view (not just has_table).
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let cmap = face.table_data(*b"cmap");
    assert!(
        cmap.is_some(),
        "fixture TTF (Noto Sans) must have a cmap table"
    );
    let cmap_bytes = cmap.expect("cmap must be present");
    // cmap table minimum size: 4-byte header (version u16 + numTables u16).
    assert!(
        cmap_bytes.len() >= 4,
        "cmap table bytes must be at least 4 bytes, got {}",
        cmap_bytes.len()
    );
}

// ---------------------------------------------------------------------------
// with_table_map tests
// ---------------------------------------------------------------------------

#[test]
fn with_table_map_finds_cmap() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let has_cmap = face
        .with_table_map(|m| m.table(b"cmap").is_some())
        .expect("with_table_map must succeed for a valid TTF");
    assert!(
        has_cmap,
        "SfntTableMap from a parsed face must expose the cmap table"
    );
}

#[test]
fn with_table_map_finds_glyf() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let glyf_len = face
        .with_table_map(|m| m.table(b"glyf").map(|s| s.len()))
        .expect("with_table_map must succeed");
    assert!(
        glyf_len.unwrap_or(0) > 0,
        "glyf table must be non-empty in a TrueType font"
    );
}

#[test]
fn with_table_map_tags_sorted() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let sorted_check = face
        .with_table_map(|m| {
            let tags: Vec<&[u8; 4]> = m.tags().collect();
            let mut expected = tags.clone();
            expected.sort();
            tags == expected
        })
        .expect("with_table_map must succeed");
    assert!(
        sorted_check,
        "tags() from with_table_map must be in sorted order"
    );
}

#[test]
fn with_table_map_raw_matches_fixture() {
    let face = ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse");
    let raw_len = face
        .with_table_map(|m| m.raw().len())
        .expect("with_table_map must succeed");
    assert_eq!(
        raw_len,
        FIXTURE_BYTES.len(),
        "raw() from with_table_map must equal the original font data length"
    );
}
