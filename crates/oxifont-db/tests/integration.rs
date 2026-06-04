//! Integration tests for oxifont-db cross-crate integration APIs.
//!
//! Covers:
//! - `FontDatabase::locale_families_for()` — locale-aware family enumeration
//!   (integration with oxitext-icu locale-specific rendering pipelines)
//! - `FontDatabase::faces_for_script()` — script-tag based face selection
//!   (integration with oxitext-shape per-script font metadata)
//! - `FontDatabase::db` feature as backend for the oxifont facade crate

use oxifont_db::{FaceInfo, FontDatabase, Source, VariationAxis};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_face_with_locale(
    family: &str,
    weight: u16,
    locale_families: Vec<(u16, String)>,
    unicode_ranges: u128,
) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::<VariationAxis>::new(),
        locale_families,
        unicode_ranges,
    }
}

// ---------------------------------------------------------------------------
// FontDatabase::locale_families_for()
// ---------------------------------------------------------------------------

/// Verifies that locale_families_for returns the correct Japanese family names
/// when faces carry ja-JP locale metadata (LCID 0x0411).
#[test]
fn test_locale_families_for_japanese() {
    let mut db = FontDatabase::new();

    // LCID 0x0411 = ja-JP
    db.add_face(make_face_with_locale(
        "Noto Sans",
        400,
        vec![(0x0411, "ノトサンス".to_string())],
        0,
    ));
    db.add_face(make_face_with_locale(
        "Noto Sans",
        700,
        vec![(0x0411, "ノトサンス".to_string())], // same locale family, should be deduped
        0,
    ));
    db.add_face(make_face_with_locale(
        "Noto Serif",
        400,
        vec![(0x0411, "ノトセリフ".to_string())],
        0,
    ));
    // This face has no locale data — should not appear for ja-JP.
    db.add_face(make_face_with_locale("Arial", 400, Vec::new(), 0));

    let families = db.locale_families_for("ja-JP");

    assert_eq!(
        families.len(),
        2,
        "two distinct ja-JP family names expected"
    );
    assert!(
        families.contains(&"ノトサンス".to_string()),
        "ja-JP Noto Sans name must appear"
    );
    assert!(
        families.contains(&"ノトセリフ".to_string()),
        "ja-JP Noto Serif name must appear"
    );
    assert!(
        !families.contains(&"Arial".to_string()),
        "faces without locale metadata must not appear"
    );
}

/// Verifies that locale_families_for is case-insensitive on the BCP-47 tag.
#[test]
fn test_locale_families_for_case_insensitive() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_locale(
        "Test Font",
        400,
        vec![(0x0411, "テストフォント".to_string())],
        0,
    ));

    let lower = db.locale_families_for("ja-jp");
    let upper = db.locale_families_for("JA-JP");
    let mixed = db.locale_families_for("Ja-Jp");

    assert_eq!(lower, upper, "case must be normalised");
    assert_eq!(lower, mixed, "case must be normalised");
    assert!(!lower.is_empty(), "should find Japanese families");
}

/// Verifies that locale_families_for returns empty for unknown locales.
#[test]
fn test_locale_families_for_unknown_locale_returns_empty() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_locale(
        "Test Font",
        400,
        vec![(0x0411, "テスト".to_string())],
        0,
    ));

    let result = db.locale_families_for("xx-ZZ");
    assert!(result.is_empty(), "unknown locale should return empty Vec");
}

/// Verifies that locale_families_for returns empty for an empty database.
#[test]
fn test_locale_families_for_empty_db() {
    let db = FontDatabase::new();
    let result = db.locale_families_for("en-US");
    assert!(result.is_empty(), "empty database yields empty list");
}

/// Verifies that locale_families_for works with Chinese (Simplified) locale.
#[test]
fn test_locale_families_for_chinese_simplified() {
    let mut db = FontDatabase::new();

    // LCID 0x0804 = zh-CN
    db.add_face(make_face_with_locale(
        "Noto Sans CJK SC",
        400,
        vec![(0x0804, "Noto Sans CJK 简体中文".to_string())],
        0,
    ));

    let families = db.locale_families_for("zh-CN");
    assert!(!families.is_empty(), "zh-CN families should be found");
    assert_eq!(families[0], "Noto Sans CJK 简体中文");
}

/// Verifies that locale_families_for correctly deduplicates family names.
#[test]
fn test_locale_families_for_deduplication() {
    let mut db = FontDatabase::new();

    // Multiple faces for the same family — should produce only one entry.
    for weight in [100u16, 200, 300, 400, 500, 600, 700, 800, 900] {
        db.add_face(make_face_with_locale(
            "MyFamily",
            weight,
            vec![(0x0409, "MyFamily".to_string())],
            0,
        ));
    }

    let families = db.locale_families_for("en-US");
    assert_eq!(
        families.len(),
        1,
        "same locale family name from 9 faces should be deduplicated to 1"
    );
}

/// Verifies that locale_families_for handles subtag stripping:
/// "ja-JP-x-custom" → tries "ja-JP" → finds LCID 0x0411.
#[test]
fn test_locale_families_for_subtag_stripping() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_locale(
        "Test Font",
        400,
        vec![(0x0411, "テスト".to_string())], // ja-JP = 0x0411
        0,
    ));

    // Should strip "x-custom" and find "ja-JP" → LCID 0x0411.
    let result = db.locale_families_for("ja-JP-x-custom");
    assert!(
        !result.is_empty(),
        "subtag stripping should find ja-JP families"
    );
}

// ---------------------------------------------------------------------------
// FontDatabase::faces_for_script()
// ---------------------------------------------------------------------------

/// Verifies that faces_for_script returns fonts that claim Arabic coverage.
#[test]
fn test_faces_for_script_arabic() {
    let mut db = FontDatabase::new();

    // Bit 13 = Arabic (both 0x0600-0x06FF and Arabic Presentation Forms).
    // unicode_ranges bit 13 => (1u128 << 13) = 8192
    let arabic_ranges: u128 = 1u128 << 13;
    let latin_ranges: u128 = 1u128 << 0; // only Basic Latin

    db.add_face(make_face_with_locale(
        "Arabic Font",
        400,
        Vec::new(),
        arabic_ranges,
    ));
    db.add_face(make_face_with_locale(
        "Latin Font",
        400,
        Vec::new(),
        latin_ranges,
    ));
    // Unknown ranges (0) — conservatively included.
    db.add_face(make_face_with_locale("Unknown Font", 400, Vec::new(), 0));

    let arabic_faces = db.faces_for_script(b"arab");

    // Arabic Font and Unknown Font should be included.
    let family_names: Vec<&str> = arabic_faces.iter().map(|f| f.family.as_str()).collect();
    assert!(
        family_names.contains(&"Arabic Font"),
        "Arabic font must be included"
    );
    assert!(
        family_names.contains(&"Unknown Font"),
        "Unknown-range font must be included conservatively"
    );
    assert!(
        !family_names.contains(&"Latin Font"),
        "Latin-only font must NOT appear for Arabic"
    );
}

/// Verifies that faces_for_script returns fonts with CJK coverage.
#[test]
fn test_faces_for_script_cjk() {
    let mut db = FontDatabase::new();

    // Bit 59 = CJK Unified Ideographs
    let cjk_ranges: u128 = 1u128 << 59;
    let latin_only: u128 = 1u128 << 0;

    db.add_face(make_face_with_locale(
        "CJK Font",
        400,
        Vec::new(),
        cjk_ranges,
    ));
    db.add_face(make_face_with_locale(
        "Latin Only",
        400,
        Vec::new(),
        latin_only,
    ));

    let cjk_faces = db.faces_for_script(b"hani");
    let names: Vec<&str> = cjk_faces.iter().map(|f| f.family.as_str()).collect();

    assert!(names.contains(&"CJK Font"), "CJK font must appear for hani");
    assert!(
        !names.contains(&"Latin Only"),
        "Latin-only font must not appear for hani"
    );
}

/// Verifies that faces_for_script returns all faces when all have unicode_ranges = 0.
#[test]
fn test_faces_for_script_all_unknown_ranges_included() {
    let mut db = FontDatabase::new();

    for family in ["FontA", "FontB", "FontC"] {
        db.add_face(make_face_with_locale(family, 400, Vec::new(), 0));
    }

    let faces = db.faces_for_script(b"latn");
    assert_eq!(
        faces.len(),
        3,
        "all unknown-range faces must be included conservatively"
    );
}

/// Verifies that faces_for_script returns empty when no faces cover the script.
#[test]
fn test_faces_for_script_no_match() {
    let mut db = FontDatabase::new();

    // Only Latin (bit 0) — no Tibetan (bit 70).
    let latin_only: u128 = 1u128 << 0;
    db.add_face(make_face_with_locale(
        "Latin Font",
        400,
        Vec::new(),
        latin_only,
    ));

    let tibetan_faces = db.faces_for_script(b"tibt");
    assert!(
        tibetan_faces.is_empty(),
        "no faces should match Tibetan when only Latin ranges are set"
    );
}

/// Verifies that faces_for_script returns empty for an empty database.
#[test]
fn test_faces_for_script_empty_db() {
    let db = FontDatabase::new();
    let faces = db.faces_for_script(b"latn");
    assert!(faces.is_empty(), "empty database yields empty result");
}

// ---------------------------------------------------------------------------
// Integration with oxifont facade crate's `db` feature
// ---------------------------------------------------------------------------
// These tests verify that the types and APIs are consistent with what the
// oxifont facade re-exports under its `db` feature module.

/// Verify that `FontDatabase::new()` / `add_face()` / `faces_by_family()` work
/// consistently as the indexed backend for higher-level crates.
#[test]
fn test_indexed_backend_basic_workflow() {
    let mut db = FontDatabase::new();

    // Simulate what the facade `db` feature does: populate from an external source.
    db.add_face(make_face_with_locale(
        "Source Sans Pro",
        400,
        vec![
            (0x0409, "Source Sans Pro".to_string()),  // en-US
            (0x0411, "ソースサンスプロ".to_string()), // ja-JP
        ],
        // Latin + CJK coverage
        (1u128 << 0) | (1u128 << 59),
    ));

    // family lookup must work
    let faces = db.faces_by_family("Source Sans Pro");
    assert_eq!(faces.len(), 1);

    // locale lookup must work (integration with oxitext-icu pipeline)
    let ja_families = db.locale_families_for("ja-JP");
    assert_eq!(ja_families.len(), 1);
    assert_eq!(ja_families[0], "ソースサンスプロ");

    // script lookup must work (integration with oxitext-shape pipeline)
    let latin_faces = db.faces_for_script(b"latn");
    assert_eq!(latin_faces.len(), 1);
    assert_eq!(latin_faces[0].family, "Source Sans Pro");
}
