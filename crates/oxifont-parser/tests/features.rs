//! Integration tests for GSUB/GPOS feature-tag extraction, from_path, preload,
//! FontCapabilities trait implementation, and related `ParsedFace` methods.

use oxifont_core::{FontCapabilities, FontFace as _};
use oxifont_parser::ParsedFace;

/// Fixture bytes compiled in at test time.
static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

// ---------------------------------------------------------------------------
// GSUB feature tag tests
// ---------------------------------------------------------------------------

#[test]
fn test_gsub_feature_tags_not_empty_on_real_font() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Noto Sans Regular has a GSUB table with multiple features (liga, kern,
    // calt, etc.). If GSUB is present the tag list must be non-empty.
    if face.has_table(*b"GSUB") {
        let tags = face.gsub_feature_tags();
        assert!(
            !tags.is_empty(),
            "GSUB feature tags must be non-empty when GSUB table is present"
        );
        // Each tag must be exactly 4 bytes of printable ASCII (sanity check).
        for tag in &tags {
            assert!(
                tag.iter().all(|b| *b >= 0x20 && *b <= 0x7e),
                "feature tag {tag:?} contains non-printable bytes"
            );
        }
    } else {
        // Font has no GSUB: the Vec must be empty (no panic).
        let tags = face.gsub_feature_tags();
        assert!(
            tags.is_empty(),
            "gsub_feature_tags must return empty Vec when GSUB is absent"
        );
    }
}

// ---------------------------------------------------------------------------
// GPOS feature tag tests
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_feature_tags_parses() {
    // This test asserts the method does not panic regardless of whether GPOS
    // is present.
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let tags = face.gpos_feature_tags();
    // Noto Sans Regular typically has a GPOS table; but even if it doesn't,
    // an empty Vec is the correct return value.
    if face.has_table(*b"GPOS") {
        assert!(
            !tags.is_empty(),
            "GPOS feature tags should be non-empty when GPOS table is present"
        );
    } else {
        assert!(
            tags.is_empty(),
            "gpos_feature_tags must return empty Vec when GPOS is absent"
        );
    }
}

// ---------------------------------------------------------------------------
// Supported scripts tests
// ---------------------------------------------------------------------------

#[test]
fn test_supported_scripts_not_empty() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let has_gsub = face.has_table(*b"GSUB");
    let has_gpos = face.has_table(*b"GPOS");
    let scripts = face.supported_scripts();
    if has_gsub || has_gpos {
        assert!(
            !scripts.is_empty(),
            "supported_scripts must be non-empty when GSUB or GPOS is present"
        );
        // Noto Sans must include the Latin script (b"latn").
        let has_latn = scripts.iter().any(|s| s == b"latn");
        assert!(has_latn, "Noto Sans must include the Latin (latn) script");
    } else {
        assert!(
            scripts.is_empty(),
            "supported_scripts must be empty when neither GSUB nor GPOS is present"
        );
    }
}

#[test]
fn test_supported_scripts_no_duplicates() {
    // Scripts returned must not contain duplicates (union deduplication).
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let scripts = face.supported_scripts();
    let unique_count = {
        let mut seen: Vec<[u8; 4]> = Vec::new();
        for s in &scripts {
            if !seen.contains(s) {
                seen.push(*s);
            }
        }
        seen.len()
    };
    assert_eq!(
        scripts.len(),
        unique_count,
        "supported_scripts must not contain duplicate tags"
    );
}

// ---------------------------------------------------------------------------
// supported_languages tests
// ---------------------------------------------------------------------------

#[test]
fn test_supported_languages_for_missing_script_is_empty() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // An invented script tag must yield an empty language list without panic.
    let langs = face.supported_languages(*b"ZZZZ");
    assert!(
        langs.is_empty(),
        "supported_languages must return empty Vec for absent script"
    );
}

#[test]
fn test_supported_languages_for_known_script() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Whether or not latn has explicit LangSys records, the call must not panic.
    let _langs = face.supported_languages(*b"latn");
    // (No further assertions — the number of LangSys records depends on the
    // specific font build; zero is also valid if no explicit LangSys tags exist.)
}

// ---------------------------------------------------------------------------
// from_path tests
// ---------------------------------------------------------------------------

#[test]
fn test_from_path() {
    // Write the fixture to a temp file and load it via from_path.
    let tmp = std::env::temp_dir().join("oxifont_parser_test_from_path.ttf");
    std::fs::write(&tmp, FIXTURE_BYTES).expect("must write temp font file");
    let face = ParsedFace::from_path(&tmp, 0).expect("from_path must succeed for valid font data");
    assert!(
        !face.family_name().is_empty(),
        "family_name must be non-empty after from_path"
    );
    // Clean up.
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_from_path_nonexistent_returns_error() {
    let nonexistent_path = std::env::temp_dir().join("oxifont_no_such_font_xyz.ttf");
    let nonexistent = nonexistent_path.as_path();
    let result = ParsedFace::from_path(nonexistent, 0);
    assert!(
        result.is_err(),
        "from_path must return an error for missing file"
    );
}

// ---------------------------------------------------------------------------
// preload tests
// ---------------------------------------------------------------------------

#[test]
fn test_preload_is_identity() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let family_before = face.family_name().to_owned();
    let weight_before = face.weight();
    let upm_before = face.units_per_em();
    // preload() consumes and returns self; the metadata must be unchanged.
    let preloaded = face.preload();
    assert_eq!(
        preloaded.family_name(),
        family_before,
        "family_name must be unchanged after preload"
    );
    assert_eq!(
        preloaded.weight(),
        weight_before,
        "weight must be unchanged after preload"
    );
    assert_eq!(
        preloaded.units_per_em(),
        upm_before,
        "units_per_em must be unchanged after preload"
    );
}

// ---------------------------------------------------------------------------
// variation_coordinates tests
// ---------------------------------------------------------------------------

#[test]
fn test_variation_coordinates_returns_none_for_non_variable_font() {
    // Noto Sans Regular is a static (non-variable) font, so it has no fvar
    // table; variation_coordinates must return None.
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    if !face.has_table(*b"fvar") {
        let result = face.variation_coordinates(&[(*b"wght", 700.0)]);
        assert!(
            result.is_none(),
            "variation_coordinates must return None for non-variable fonts"
        );
    }
}

#[test]
fn test_variation_settings_empty_by_default() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    assert!(
        face.variation_settings().is_empty(),
        "variation_settings must be empty for a freshly parsed face"
    );
}

// ---------------------------------------------------------------------------
// FontCapabilities trait implementation tests
// ---------------------------------------------------------------------------

/// Verify that `FontCapabilities::gsub_features` returns the same tags as
/// the inherent `gsub_feature_tags` method (they must be identical).
#[test]
fn test_font_capabilities_gsub_features_matches_inherent() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let via_trait = face.gsub_features();
    let via_inherent = face.gsub_feature_tags();
    assert_eq!(
        via_trait, via_inherent,
        "FontCapabilities::gsub_features must return identical data to gsub_feature_tags"
    );
}

/// Verify that `FontCapabilities::gpos_features` returns the same tags as
/// the inherent `gpos_feature_tags` method.
#[test]
fn test_font_capabilities_gpos_features_matches_inherent() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let via_trait = face.gpos_features();
    let via_inherent = face.gpos_feature_tags();
    assert_eq!(
        via_trait, via_inherent,
        "FontCapabilities::gpos_features must return identical data to gpos_feature_tags"
    );
}

/// Verify that `FontCapabilities::supported_scripts` returns the same tags as
/// the inherent `supported_scripts` method.
#[test]
fn test_font_capabilities_supported_scripts_matches_inherent() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Call through the trait via explicit UFCS to force vtable dispatch.
    let via_trait: Vec<[u8; 4]> = <ParsedFace as FontCapabilities>::supported_scripts(&face);
    // Inherent method
    let via_inherent = face.supported_scripts();
    assert_eq!(
        via_trait, via_inherent,
        "FontCapabilities::supported_scripts must match the inherent method"
    );
}

/// Verify that `FontCapabilities::has_feature` returns `true` for features
/// that are in GSUB and `false` for an invented tag.
#[test]
fn test_font_capabilities_has_feature() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // An invented tag must never appear.
    assert!(
        !face.has_feature(*b"ZZZZ"),
        "invented tag ZZZZ must not be reported as present"
    );
    // Any GSUB feature the inherent method reports must also be reported by
    // has_feature.
    for tag in face.gsub_feature_tags() {
        assert!(
            face.has_feature(tag),
            "has_feature must return true for GSUB feature {tag:?}"
        );
    }
    // Same for GPOS features.
    for tag in face.gpos_feature_tags() {
        assert!(
            face.has_feature(tag),
            "has_feature must return true for GPOS feature {tag:?}"
        );
    }
}

/// Verify that `FontCapabilities::supported_languages` returns the same tags
/// as the inherent `supported_languages` method for the Latin script.
#[test]
fn test_font_capabilities_supported_languages_via_trait() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // Explicit UFCS to force the FontCapabilities vtable path.
    let via_trait: Vec<[u8; 4]> =
        <ParsedFace as FontCapabilities>::supported_languages(&face, *b"latn");
    let via_inherent = face.supported_languages(*b"latn");
    assert_eq!(
        via_trait, via_inherent,
        "FontCapabilities::supported_languages must match inherent method for latn"
    );
}

/// Verify `ParsedFace` is usable as `Box<dyn FontCapabilities>`.
///
/// This ensures the trait is object-safe when used through `ParsedFace`, which
/// is important for downstream consumers (e.g. oxitext-shape) that may store
/// a `Box<dyn FontCapabilities>` in a cache or shaper state.
#[test]
fn test_parsed_face_is_dyn_font_capabilities() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let boxed: Box<dyn oxifont_core::FontCapabilities> = Box::new(face);
    // Calling through the vtable must not panic.
    let _ = boxed.gsub_features();
    let _ = boxed.gpos_features();
    let _ = boxed.supported_scripts();
    let _ = boxed.supported_languages(*b"latn");
}
