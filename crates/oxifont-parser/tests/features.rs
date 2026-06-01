//! Integration tests for GSUB/GPOS feature-tag extraction, from_path, preload,
//! and related `ParsedFace` methods added in the features round.

use oxifont_core::FontFace as _;
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
