//! Integration tests for `BundledFont` and `BundledCatalog`.
//!
//! Run with:
//!   cargo test -p oxifont-bundled
//!   cargo test -p oxifont-bundled --features bundled-noto

use oxifont_bundled::{all, BundledCatalog};
use oxifont_core::FontCatalog as _;

// ── Accessor-level tests (no font file required) ──────────────────────────────

#[test]
fn bundled_catalog_default_constructs() {
    let catalog = BundledCatalog::default();
    // Calling fonts() should not panic.
    let _fonts = catalog.fonts();
}

#[test]
fn all_returns_static_slice() {
    // all() must be callable without panicking; may be empty.
    let _fonts = all();
}

#[test]
fn all_fonts_no_duplicates() {
    use std::collections::HashSet;
    let fonts = all();
    let mut seen: HashSet<(&str, u16, String)> = HashSet::new();
    for f in fonts {
        let key = (f.family, f.weight, format!("{:?}", f.style()));
        assert!(
            seen.insert(key),
            "Duplicate bundled font: {} weight={}",
            f.family,
            f.weight
        );
    }
}

#[test]
fn bundled_catalog_faces_matches_all_len() {
    let catalog = BundledCatalog::default();
    assert_eq!(
        catalog.faces().len(),
        all().len(),
        "faces() count must equal all() count"
    );
}

#[test]
fn bundled_catalog_find_by_family_case_insensitive() {
    let catalog = BundledCatalog::default();
    // These calls must not panic regardless of which features are active.
    let a = catalog.find_by_family("Noto Sans");
    let b = catalog.find_by_family("noto sans");
    let c = catalog.find_by_family("NOTO SANS");
    // All variants must agree on whether a match exists.
    assert_eq!(
        a.is_some(),
        b.is_some(),
        "case variants must agree on presence"
    );
    assert_eq!(
        a.is_some(),
        c.is_some(),
        "case variants must agree on presence"
    );
}

#[test]
fn bundled_catalog_find_returns_none_for_unknown() {
    let catalog = BundledCatalog::default();
    use oxifont_core::FontQuery;
    let result = catalog.find(&FontQuery::new().family("This Font Does Not Exist 99"));
    assert!(result.is_none(), "find must return None for unknown family");
}

// ── Tests that require bundled-noto feature ───────────────────────────────────

#[cfg(feature = "bundled-noto")]
mod with_noto {
    use super::*;
    use oxifont_bundled::{BundledCatalog, SANS_BOLD, SANS_REGULAR, SERIF_REGULAR};
    use oxifont_core::{FontFace as _, FontQuery, FontStretch, FontStyle};

    // ── SANS_REGULAR ─────────────────────────────────────────────────────────

    #[test]
    fn sans_regular_accessors() {
        assert_eq!(SANS_REGULAR.family_name(), "Noto Sans");
        assert_eq!(SANS_REGULAR.weight(), 400);
        assert_eq!(SANS_REGULAR.style(), FontStyle::Normal);
        assert!(!SANS_REGULAR.data().is_empty(), "data() must not be empty");
    }

    #[test]
    fn sans_regular_ttf_magic() {
        // Use decompressed_data() so the test works with both `compressed` and
        // non-compressed builds (under `compressed`, `data()` holds zlib bytes).
        let d = SANS_REGULAR
            .decompressed_data()
            .expect("NotoSans-Regular decompressed_data must succeed");
        assert!(d.len() >= 4, "NotoSans-Regular too short");
        let magic = &d[..4];
        let valid = magic == [0x00, 0x01, 0x00, 0x00]
            || magic == b"OTTO"
            || magic == b"ttcf"
            || magic == b"true";
        assert!(valid, "Unexpected magic: {:?}", magic);
    }

    #[test]
    fn sans_regular_parses_successfully() {
        let face = SANS_REGULAR
            .parse()
            .expect("NotoSans-Regular must parse without error");
        assert!(
            !face.family_name().is_empty(),
            "parsed face must have a non-empty family name"
        );
    }

    #[test]
    fn sans_regular_family_name_from_parsed() {
        use oxifont_core::FontFace as _;
        let face = SANS_REGULAR.parse().expect("NotoSans-Regular must parse");
        assert_eq!(face.family_name(), "Noto Sans");
        assert_eq!(face.weight(), 400);
    }

    // ── SANS_BOLD ─────────────────────────────────────────────────────────────

    #[test]
    fn sans_bold_accessors() {
        assert_eq!(SANS_BOLD.family_name(), "Noto Sans");
        assert_eq!(SANS_BOLD.weight(), 700);
        assert_eq!(SANS_BOLD.style(), FontStyle::Normal);
        assert!(!SANS_BOLD.data().is_empty(), "data() must not be empty");
    }

    #[test]
    fn sans_bold_ttf_magic() {
        // Use decompressed_data() so the test works with both `compressed` and
        // non-compressed builds (under `compressed`, `data()` holds zlib bytes).
        let d = SANS_BOLD
            .decompressed_data()
            .expect("NotoSans-Bold decompressed_data must succeed");
        assert!(d.len() >= 4, "NotoSans-Bold too short");
        let magic = &d[..4];
        let valid = magic == [0x00, 0x01, 0x00, 0x00]
            || magic == b"OTTO"
            || magic == b"ttcf"
            || magic == b"true";
        assert!(valid, "Unexpected magic: {:?}", magic);
    }

    #[test]
    fn sans_bold_parses_successfully() {
        let face = SANS_BOLD
            .parse()
            .expect("NotoSans-Bold must parse without error");
        assert!(
            !face.family_name().is_empty(),
            "parsed face must have a non-empty family name"
        );
    }

    #[test]
    fn sans_bold_family_name_from_parsed() {
        let face = SANS_BOLD.parse().expect("NotoSans-Bold must parse");
        assert_eq!(face.family_name(), "Noto Sans");
        assert_eq!(face.weight(), 700);
    }

    #[test]
    fn catalog_find_bold_by_weight() {
        let catalog = BundledCatalog::default();
        let q = FontQuery::new()
            .family("Noto Sans")
            .weight(700)
            .style(FontStyle::Normal)
            .stretch(FontStretch::Normal);
        assert!(
            catalog.find(&q).is_some(),
            "find must succeed for Noto Sans weight=700 Normal (SANS_BOLD)"
        );
    }

    // ── SERIF_REGULAR ─────────────────────────────────────────────────────────

    #[test]
    fn serif_regular_accessors() {
        assert_eq!(SERIF_REGULAR.family_name(), "Noto Serif");
        assert_eq!(SERIF_REGULAR.weight(), 400);
        assert_eq!(SERIF_REGULAR.style(), FontStyle::Normal);
        assert!(!SERIF_REGULAR.data().is_empty());
    }

    #[test]
    fn serif_regular_ttf_magic() {
        // Use decompressed_data() so the test works with both `compressed` and
        // non-compressed builds (under `compressed`, `data()` holds zlib bytes).
        let d = SERIF_REGULAR
            .decompressed_data()
            .expect("NotoSerif-Regular decompressed_data must succeed");
        assert!(d.len() >= 4, "NotoSerif-Regular too short");
        let magic = &d[..4];
        let valid = magic == [0x00, 0x01, 0x00, 0x00]
            || magic == b"OTTO"
            || magic == b"ttcf"
            || magic == b"true";
        assert!(valid, "Unexpected magic: {:?}", magic);
    }

    #[test]
    fn serif_regular_parses_successfully() {
        let face = SERIF_REGULAR
            .parse()
            .expect("NotoSerif-Regular must parse without error");
        assert!(!face.family_name().is_empty());
    }

    // ── all() with bundled-noto ───────────────────────────────────────────────

    #[test]
    fn all_has_expected_count_with_noto_feature() {
        // With bundled-noto: 5 fonts (NotoSans-Regular, NotoSans-Bold, NotoSerif-Regular,
        // NotoSans-Italic, NotoSansMono-Regular).
        assert_eq!(
            all().len(),
            5,
            "Expected 5 bundled fonts with bundled-noto feature"
        );
    }

    #[test]
    fn all_fonts_have_valid_ttf_magic() {
        for font in all() {
            // Under the `compressed` feature, `data()` holds zlib bytes —
            // use `decompressed_data()` to get the raw TTF for magic checks.
            let d = font
                .decompressed_data()
                .expect("decompressed_data must succeed");
            assert!(d.len() >= 4, "Font {} data too short", font.family);
            let magic = &d[..4];
            let valid = magic == [0x00, 0x01, 0x00, 0x00]
                || magic == b"OTTO"
                || magic == b"ttcf"
                || magic == b"true";
            assert!(valid, "Font {} has invalid magic: {:?}", font.family, magic);
        }
    }

    // ── BundledCatalog with bundled-noto ──────────────────────────────────────

    #[test]
    fn catalog_faces_count_equals_three() {
        let catalog = BundledCatalog::default();
        assert_eq!(
            catalog.faces().len(),
            5,
            "Expected 5 faces in default catalog with bundled-noto (Regular, Bold, Serif, Italic, Mono)"
        );
    }

    #[test]
    fn catalog_find_by_family_finds_noto_sans() {
        let catalog = BundledCatalog::default();
        let result = catalog.find_by_family("Noto Sans");
        assert!(result.is_some(), "find_by_family must find Noto Sans");
        let font = result.expect("Noto Sans must be present");
        assert_eq!(font.family, "Noto Sans");
    }

    #[test]
    fn catalog_find_by_family_finds_noto_serif() {
        let catalog = BundledCatalog::default();
        let result = catalog.find_by_family("Noto Serif");
        assert!(result.is_some(), "find_by_family must find Noto Serif");
    }

    #[test]
    fn catalog_find_by_query_family() {
        let catalog = BundledCatalog::default();
        let q = FontQuery::new().family("Noto Sans");
        let result = catalog.find(&q);
        assert!(result.is_some(), "find must locate Noto Sans by FontQuery");
        let info = result.expect("Noto Sans face must be present");
        assert_eq!(&*info.family, "Noto Sans");
        assert_eq!(info.weight, 400);
    }

    #[test]
    fn catalog_find_by_query_weight_and_style() {
        let catalog = BundledCatalog::default();
        let q = FontQuery::new()
            .family("Noto Sans")
            .weight(400)
            .style(FontStyle::Normal)
            .stretch(FontStretch::Normal);
        assert!(
            catalog.find(&q).is_some(),
            "find must succeed for Noto Sans weight=400 Normal"
        );
    }

    #[test]
    fn catalog_find_wrong_weight_returns_none() {
        let catalog = BundledCatalog::default();
        let q = FontQuery::new().family("Noto Sans").weight(900);
        assert!(
            catalog.find(&q).is_none(),
            "find must return None for Noto Sans weight=900 (not bundled)"
        );
    }

    #[test]
    fn catalog_fonts_by_family_iterator() {
        let catalog = BundledCatalog::default();
        let variants: Vec<_> = catalog.fonts_by_family("Noto Sans").collect();
        assert_eq!(
            variants.len(),
            3,
            "Three Noto Sans variants are bundled (Regular, Bold, Italic)"
        );
    }

    #[test]
    fn catalog_find_by_postscript_name() {
        let catalog = BundledCatalog::default();
        let q = FontQuery::new().postscript_name("NotoSans-Regular");
        let result = catalog.find(&q);
        assert!(
            result.is_some(),
            "find by postscript name NotoSans-Regular must succeed"
        );
    }

    // ── Round-trip integration: catalog → parse → glyph_for_char ─────────────

    #[test]
    fn catalog_round_trip_glyph_for_char() {
        use oxifont_core::FontFace as _;
        let catalog = BundledCatalog::default();
        let q = FontQuery::new().family("Noto Sans");
        let info = catalog.find(&q).expect("Noto Sans must be in catalog");
        // Find the matching BundledFont descriptor and parse it.
        let font = catalog
            .find_by_family(&info.family)
            .expect("find_by_family must succeed");
        let face = font.parse().expect("NotoSans-Regular must parse");
        // 'A' (U+0041) must have a glyph in Noto Sans.
        let gid = face.glyph_for_char('A');
        assert!(gid.is_some(), "Noto Sans must have a glyph for 'A'");
    }
}

// ── decompressed_data() — new Task 1 API ────────────────────────────────────

#[cfg(feature = "bundled-noto")]
mod decompressed_data_tests {
    use oxifont_bundled::{SANS_BOLD, SANS_REGULAR, SERIF_REGULAR};

    /// `decompressed_data()` must return the raw TTF bytes unchanged.
    ///
    /// Verifies both with and without the `compressed` feature: the magic-aware
    /// short-circuit in `compressed::decompress_font` must pass raw SFNT bytes
    /// through as-is until the build.rs compression step is implemented.
    #[test]
    fn sans_regular_decompressed_data_is_valid_ttf() {
        let bytes = SANS_REGULAR
            .decompressed_data()
            .expect("decompressed_data must succeed for NotoSans-Regular");
        assert!(bytes.len() >= 4, "decompressed data too short");
        // Must start with TrueType magic (0x00 0x01 0x00 0x00).
        assert_eq!(
            &bytes[..4],
            &[0x00, 0x01, 0x00, 0x00],
            "NotoSans-Regular must have TrueType magic after decompress"
        );
    }

    #[test]
    fn sans_bold_decompressed_data_is_valid_ttf() {
        let bytes = SANS_BOLD
            .decompressed_data()
            .expect("decompressed_data must succeed for NotoSans-Bold");
        assert!(bytes.len() >= 4);
        assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn serif_regular_decompressed_data_is_valid_ttf() {
        let bytes = SERIF_REGULAR
            .decompressed_data()
            .expect("decompressed_data must succeed for NotoSerif-Regular");
        assert!(bytes.len() >= 4);
        assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn decompressed_data_length_matches_raw_data() {
        let decompressed = SANS_REGULAR
            .decompressed_data()
            .expect("decompress must succeed");
        // The decompressed output must be a non-empty valid font.
        assert!(
            !decompressed.is_empty(),
            "decompressed data must not be empty"
        );
        // Must start with a valid TrueType magic.
        assert_eq!(
            &decompressed[..4],
            &[0x00, 0x01, 0x00, 0x00],
            "decompressed NotoSans-Regular must start with TrueType magic"
        );

        // Under the compressed feature the stored bytes are zlib-wrapped, so
        // raw.len() < decompressed.len().  Without compression they are equal.
        #[cfg(not(feature = "compressed"))]
        {
            let raw = SANS_REGULAR.data();
            assert_eq!(
                raw.len(),
                decompressed.len(),
                "without compressed feature: decompressed length must equal raw length"
            );
        }
        #[cfg(feature = "compressed")]
        {
            let raw = SANS_REGULAR.data();
            assert!(
                raw.len() < decompressed.len(),
                "with compressed feature: stored bytes must be smaller than decompressed (raw={}, decompressed={})",
                raw.len(),
                decompressed.len()
            );
        }
    }
}

// ── Compressed feature: round-trip and validity tests ────────────────────────

/// Tests that are only meaningful when both `bundled-noto` and `compressed` are
/// active — the build script has run and the font data is zlib-compressed.
#[cfg(all(feature = "bundled-noto", feature = "compressed"))]
mod compressed_tests {
    use oxifont_bundled::{SANS_BOLD, SANS_REGULAR, SERIF_REGULAR};

    /// Verify that the raw `data()` slice is NOT a raw TTF (it is zlib).
    ///
    /// Under the compressed feature `data()` returns the zlib-compressed bytes
    /// produced by `build.rs`.  A raw TrueType file starts with the magic
    /// `0x00 0x01 0x00 0x00`; zlib starts with `0x78 xx`.
    #[test]
    fn compressed_data_is_not_raw_sfnt() {
        for font in oxifont_bundled::all() {
            let d = font.data();
            assert!(
                d.len() >= 2,
                "compressed data too short for {}",
                font.family
            );
            // zlib header: CMF byte 0x78, FLG byte (0x01, 0x9C, 0xDA, …).
            assert_eq!(
                d[0], 0x78,
                "Font {} stored data must start with zlib CMF byte 0x78, got 0x{:02x}",
                font.family, d[0]
            );
        }
    }

    /// Verify that `decompressed_data()` yields valid TrueType/OTF magic bytes.
    #[test]
    fn decompressed_data_has_valid_font_magic() {
        let fonts = [
            SANS_REGULAR
                .decompressed_data()
                .expect("SANS_REGULAR decompress"),
            SANS_BOLD.decompressed_data().expect("SANS_BOLD decompress"),
            SERIF_REGULAR
                .decompressed_data()
                .expect("SERIF_REGULAR decompress"),
        ];
        for bytes in &fonts {
            assert!(bytes.len() >= 4, "decompressed font too short");
            let magic = &bytes[..4];
            let valid = magic == [0x00, 0x01, 0x00, 0x00]
                || magic == b"OTTO"
                || magic == b"ttcf"
                || magic == b"true";
            assert!(valid, "decompressed font has unexpected magic: {:?}", magic);
        }
    }

    /// Verify zlib compress→decompress round-trip using `oxiarc_deflate` directly.
    ///
    /// This is a unit-level check that the compression algorithm used by
    /// `build.rs` (`zlib_compress`) round-trips correctly with the runtime
    /// decompressor (`zlib_decompress`).
    #[test]
    fn zlib_round_trip() {
        let original = SANS_REGULAR
            .decompressed_data()
            .expect("SANS_REGULAR decompress");
        // Re-compress at level 6 (same as build.rs).
        let recompressed =
            oxiarc_deflate::zlib_compress(&original, 6).expect("zlib_compress must succeed");
        let roundtripped =
            oxiarc_deflate::zlib_decompress(&recompressed).expect("zlib_decompress must succeed");
        assert_eq!(
            original, roundtripped,
            "round-trip compress→decompress must reproduce the original bytes"
        );
    }

    /// Verify that compression actually reduces the font size.
    ///
    /// Font files (TTF/OTF) contain many structured binary tables that compress
    /// well; a good compressor should always produce output smaller than input.
    #[test]
    fn compressed_size_is_smaller_than_raw() {
        let raw = SANS_REGULAR
            .decompressed_data()
            .expect("SANS_REGULAR decompress");
        let stored = SANS_REGULAR.data();
        assert!(
            stored.len() < raw.len(),
            "compressed size ({}) must be less than raw size ({})",
            stored.len(),
            raw.len()
        );
    }
}

// ── Without bundled-noto: catalog is empty but functional ────────────────────

#[cfg(not(feature = "bundled-noto"))]
mod without_noto {
    use super::*;
    use oxifont_core::FontQuery;

    #[test]
    fn all_is_empty_without_noto_feature() {
        assert!(
            all().is_empty(),
            "all() must be empty when bundled-noto is not active"
        );
    }

    #[test]
    fn catalog_faces_empty_without_noto_feature() {
        let catalog = BundledCatalog::default();
        assert!(catalog.faces().is_empty());
    }

    #[test]
    fn catalog_find_returns_none_without_noto_feature() {
        let catalog = BundledCatalog::default();
        let result = catalog.find(&FontQuery::new().family("Noto Sans"));
        assert!(result.is_none());
    }
}
