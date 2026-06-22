//! Integration tests for the `oxifont` facade crate.

use oxifont::FontFace as _;

/// Fixture bytes compiled in at test time (Noto Sans Regular).
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

#[test]
fn detect_format_truetype() {
    assert_eq!(
        oxifont::detect_format(FIXTURE_BYTES),
        oxifont::FontFormat::TrueType
    );
}

#[test]
fn detect_format_unknown_for_short_data() {
    assert_eq!(
        oxifont::detect_format(&[0x00, 0x00]),
        oxifont::FontFormat::Unknown
    );
}

#[test]
fn detect_format_woff1_magic() {
    let data = b"wOFF\x00\x00\x00\x00";
    assert_eq!(oxifont::detect_format(data), oxifont::FontFormat::Woff1);
}

#[test]
fn detect_format_woff2_magic() {
    let data = b"wOF2\x00\x00\x00\x00";
    assert_eq!(oxifont::detect_format(data), oxifont::FontFormat::Woff2);
}

#[test]
fn detect_format_opentype_magic() {
    let data = b"OTTO\x00\x00\x00\x00";
    assert_eq!(oxifont::detect_format(data), oxifont::FontFormat::OpenType);
}

#[test]
fn detect_format_ttc_magic() {
    let data = b"ttcf\x00\x00\x00\x00";
    assert_eq!(
        oxifont::detect_format(data),
        oxifont::FontFormat::TrueTypeCollection
    );
}

#[test]
fn load_font_bytes_parses_successfully() {
    let face = oxifont::load_font_bytes(FIXTURE_BYTES.to_vec(), 0)
        .expect("load_font_bytes with fixture should succeed");
    assert!(!face.family_name().is_empty());
}

#[test]
fn decode_and_parse_truetype() {
    let face = oxifont::decode_and_parse(FIXTURE_BYTES)
        .expect("decode_and_parse with TTF fixture should succeed");
    assert!(face.weight() > 0);
}

#[test]
fn decode_and_parse_unknown_format_returns_error() {
    let bad = vec![0xFFu8; 100];
    let result = oxifont::decode_and_parse(&bad);
    assert!(result.is_err(), "unknown format should return an error");
}

#[test]
fn version_returns_nonempty_string() {
    let v = oxifont::version();
    assert!(!v.is_empty(), "version string must not be empty");
}

#[test]
fn face_count_returns_one_for_ttf() {
    assert_eq!(oxifont::face_count(FIXTURE_BYTES), 1);
}

#[test]
fn font_format_display() {
    assert_eq!(oxifont::FontFormat::TrueType.to_string(), "TrueType");
    assert_eq!(oxifont::FontFormat::Woff2.to_string(), "WOFF2");
    assert_eq!(oxifont::FontFormat::Unknown.to_string(), "Unknown");
}

#[test]
fn prelude_imports_core_types() {
    // Verify that prelude re-exports compile without issue.
    use oxifont::prelude::*;
    let _q = FontQuery::new().family("test");
    let _s = FontStyle::Normal;
    let _st = FontStretch::Normal;
}

#[test]
fn facade_reexports_core_types() {
    // Verify all new core types are accessible through the facade.
    let _stretch = oxifont::FontStretch::Condensed;
    let _format = oxifont::ColorGlyphFormat::ColrV0;
    let _outline = oxifont::GlyphOutline::Close;
    let _pair = oxifont::KerningPair {
        left_gid: 1,
        right_gid: 2,
        value: -50,
    };
}

// ── bundled_fonts() — Task 2 API ─────────────────────────────────────────────

#[cfg(feature = "bundled-noto")]
mod bundled_api_tests {
    use oxifont_core::FontCatalog as _;

    #[test]
    fn bundled_fonts_returns_non_empty_catalog() {
        let catalog = oxifont::bundled_fonts();
        assert!(
            !catalog.faces().is_empty(),
            "bundled_fonts() must return a non-empty catalog with bundled-noto feature"
        );
    }

    #[test]
    fn bundled_fonts_contains_noto_sans() {
        use oxifont_core::FontQuery;
        let catalog = oxifont::bundled_fonts();
        let q = FontQuery::new().family("Noto Sans");
        assert!(
            catalog.find(&q).is_some(),
            "bundled_fonts() catalog must contain Noto Sans"
        );
    }

    #[test]
    fn bundled_fonts_catalog_has_five_faces() {
        let catalog = oxifont::bundled_fonts();
        assert_eq!(
            catalog.faces().len(),
            5,
            "bundled_fonts() must return 5 faces (NotoSans-Regular, NotoSans-Bold, NotoSerif-Regular, NotoSans-Italic, NotoSansMono-Regular)"
        );
    }
}

// ── system_fonts_with_bundled_fallback() — Task 2 API ───────────────────────

#[cfg(all(feature = "db", feature = "bundled-noto"))]
mod bundled_fallback_tests {
    #[test]
    fn system_fonts_with_bundled_fallback_is_ok() {
        let result = oxifont::system_fonts_with_bundled_fallback();
        assert!(
            result.is_ok(),
            "system_fonts_with_bundled_fallback() must not return an error"
        );
    }

    #[test]
    fn system_fonts_with_bundled_fallback_has_faces() {
        let db = oxifont::system_fonts_with_bundled_fallback()
            .expect("system_fonts_with_bundled_fallback must succeed");
        // Should have either system fonts OR bundled fallback, never zero.
        assert!(
            db.stats().face_count > 0,
            "database must have at least one face (system or bundled fallback)"
        );
    }
}
