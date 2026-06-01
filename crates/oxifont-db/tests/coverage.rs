//! Tests for unicode-range coverage, script tag lookup, and fallback matching.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal [`FaceInfo`] with the given `unicode_ranges` bitmap.
fn make_face_with_ranges(family: &str, unicode_ranges: u128) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight: 400,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges,
    }
}

/// Bit 0 = Basic Latin (U+0000–007F).
const BIT_BASIC_LATIN: u128 = 1u128 << 0;

/// Bit 6 = Greek and Coptic (U+0370–03FF).
const BIT_GREEK: u128 = 1u128 << 6;

/// Bit 9 = Cyrillic (U+0400–052F).
const BIT_CYRILLIC: u128 = 1u128 << 9;

/// Bit 59 = CJK Unified Ideographs (U+4E00–9FFF and extensions).
const BIT_CJK: u128 = 1u128 << 59;

/// Bit 49 = Hiragana (U+3040–309F).
const BIT_HIRAGANA: u128 = 1u128 << 49;

/// Bit 50 = Katakana (U+30A0–30FF).
const BIT_KATAKANA: u128 = 1u128 << 50;

// ---------------------------------------------------------------------------
// Task 1: covers_char_approx
// ---------------------------------------------------------------------------

/// A face with the Basic Latin bit set must report coverage for 'A'.
#[test]
fn test_covers_char_basic_latin() {
    let face = make_face_with_ranges("LatinFont", BIT_BASIC_LATIN);
    assert!(
        face.covers_char_approx('A'),
        "Basic Latin bit set → 'A' (U+0041) must be covered"
    );
    assert!(
        face.covers_char_approx('z'),
        "Basic Latin bit set → 'z' (U+007A) must be covered"
    );
}

/// A face whose Basic Latin bit is NOT set must not cover 'A'.
#[test]
fn test_covers_char_basic_latin_not_set() {
    // Only Greek bit set, not Basic Latin.
    let face = make_face_with_ranges("GreekOnly", BIT_GREEK);
    assert!(
        !face.covers_char_approx('A'),
        "only Greek bit set → 'A' (U+0041) must NOT be covered"
    );
    assert!(
        face.covers_char_approx('\u{0391}'),
        "Greek bit set → U+0391 (Greek Alpha) must be covered"
    );
}

/// A face with `unicode_ranges == 0` must return `true` for any character
/// (unknown coverage → assume yes).
#[test]
fn test_covers_char_approx_unknown_returns_true() {
    let face = make_face_with_ranges("UnknownFont", 0);
    assert!(
        face.covers_char_approx('A'),
        "unicode_ranges=0 must return true for ASCII"
    );
    assert!(
        face.covers_char_approx('\u{4E2D}'), // CJK character 中
        "unicode_ranges=0 must return true for CJK"
    );
    assert!(
        face.covers_char_approx('\u{0400}'), // Cyrillic А
        "unicode_ranges=0 must return true for Cyrillic"
    );
}

/// CJK bit covers ideographs in the main block.
#[test]
fn test_covers_char_cjk_ideograph() {
    let face = make_face_with_ranges("CJKFont", BIT_CJK);
    assert!(
        face.covers_char_approx('\u{4E2D}'), // 中
        "CJK bit set → U+4E2D must be covered"
    );
    assert!(
        !face.covers_char_approx('A'),
        "only CJK bit set → 'A' must NOT be covered"
    );
}

/// Hiragana bit covers hiragana characters.
#[test]
fn test_covers_char_hiragana() {
    let face = make_face_with_ranges("HiraFont", BIT_HIRAGANA);
    assert!(
        face.covers_char_approx('\u{3042}'), // あ (HIRAGANA LETTER A)
        "Hiragana bit set → U+3042 must be covered"
    );
    assert!(
        !face.covers_char_approx('\u{30A2}'), // ア (KATAKANA LETTER A)
        "Hiragana bit set, katakana bit NOT set → U+30A2 must NOT be covered"
    );
}

// ---------------------------------------------------------------------------
// Task 2: supported_scripts_approx
// ---------------------------------------------------------------------------

/// Basic Latin bit → script list must include "latn".
#[test]
fn test_supported_scripts_latn() {
    let face = make_face_with_ranges("LatinFont", BIT_BASIC_LATIN);
    let scripts = face.supported_scripts_approx();
    assert!(
        scripts.contains(b"latn"),
        "Basic Latin bit → 'latn' must appear; got: {:?}",
        scripts
            .iter()
            .map(|s| std::str::from_utf8(s).unwrap_or("????"))
            .collect::<Vec<_>>()
    );
}

/// Greek bit → script list must include "grek".
#[test]
fn test_supported_scripts_grek() {
    let face = make_face_with_ranges("GreekFont", BIT_GREEK);
    let scripts = face.supported_scripts_approx();
    assert!(
        scripts.contains(b"grek"),
        "Greek bit → 'grek' must appear; got: {:?}",
        scripts
            .iter()
            .map(|s| std::str::from_utf8(s).unwrap_or("????"))
            .collect::<Vec<_>>()
    );
}

/// Cyrillic bit → script list must include "cyrl".
#[test]
fn test_supported_scripts_cyrl() {
    let face = make_face_with_ranges("CyrlFont", BIT_CYRILLIC);
    let scripts = face.supported_scripts_approx();
    assert!(
        scripts.contains(b"cyrl"),
        "Cyrillic bit → 'cyrl' must appear; got: {:?}",
        scripts
            .iter()
            .map(|s| std::str::from_utf8(s).unwrap_or("????"))
            .collect::<Vec<_>>()
    );
}

/// CJK + Hiragana + Katakana bits → script list must include "hani", "hira",
/// and "kana".
#[test]
fn test_supported_scripts_cjk_japanese() {
    let face = make_face_with_ranges(
        "JapaneseFont",
        BIT_CJK | BIT_HIRAGANA | BIT_KATAKANA | BIT_BASIC_LATIN,
    );
    let scripts = face.supported_scripts_approx();
    let tag_strs: Vec<&str> = scripts
        .iter()
        .map(|s| std::str::from_utf8(s).unwrap_or("????"))
        .collect();
    assert!(
        scripts.contains(b"hani"),
        "CJK bit → 'hani' must appear; got: {:?}",
        tag_strs
    );
    assert!(
        scripts.contains(b"hira"),
        "Hiragana bit → 'hira' must appear; got: {:?}",
        tag_strs
    );
    assert!(
        scripts.contains(b"kana"),
        "Katakana bit → 'kana' must appear; got: {:?}",
        tag_strs
    );
    // Result must be sorted (deduplicated).
    let mut sorted = scripts.clone();
    sorted.sort_unstable();
    assert_eq!(
        scripts, sorted,
        "supported_scripts_approx must return a sorted list"
    );
}

/// unicode_ranges == 0 → empty script list (no claims).
#[test]
fn test_supported_scripts_unknown_empty() {
    let face = make_face_with_ranges("UnknownFont", 0);
    let scripts = face.supported_scripts_approx();
    assert!(
        scripts.is_empty(),
        "unicode_ranges=0 must return an empty script list; got: {:?}",
        scripts
            .iter()
            .map(|s| std::str::from_utf8(s).unwrap_or("????"))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Task 4: match_with_fallback
// ---------------------------------------------------------------------------

/// ASCII-only text: the primary face (Latin bits set) covers everything.
#[test]
fn test_match_with_fallback_ascii() {
    let mut db = FontDatabase::new();
    let latin_face = make_face_with_ranges("LatinFont", BIT_BASIC_LATIN);
    db.add_face(latin_face);

    let chain = Query::new(&db)
        .family("LatinFont")
        .match_with_fallback("Hello, World!");

    assert!(
        !chain.is_empty(),
        "fallback chain must contain at least one face for ASCII text"
    );
    assert_eq!(
        chain[0].family, "LatinFont",
        "primary face must be first in the chain"
    );
}

/// Empty text returns an empty chain.
#[test]
fn test_match_with_fallback_empty_text() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_ranges("LatinFont", BIT_BASIC_LATIN));

    let chain = Query::new(&db).family("LatinFont").match_with_fallback("");

    assert!(
        chain.is_empty(),
        "fallback chain must be empty for empty text"
    );
}

/// Mixed-script text triggers multi-face fallback.
///
/// "A" (Latin) + "中" (CJK) — LatinFont covers Latin but not CJK,
/// CJKFont covers CJK.  The chain should contain both.
#[test]
fn test_match_with_fallback_mixed_scripts() {
    let mut db = FontDatabase::new();
    // Primary: Latin only
    db.add_face(make_face_with_ranges("LatinFont", BIT_BASIC_LATIN));
    // Fallback: CJK
    db.add_face(make_face_with_ranges("CJKFont", BIT_CJK));

    let text = "A\u{4E2D}"; // "A中"
    let chain = Query::new(&db)
        .family("LatinFont")
        .match_with_fallback(text);

    assert!(
        chain.len() >= 2,
        "mixed Latin+CJK text should produce a chain of at least 2 faces; got {}",
        chain.len()
    );
    assert_eq!(
        chain[0].family, "LatinFont",
        "primary face must be LatinFont"
    );
    assert!(
        chain.iter().any(|f| f.family == "CJKFont"),
        "CJKFont must be added to cover U+4E2D"
    );
}

/// Fallback chain contains no duplicates.
#[test]
fn test_match_with_fallback_no_duplicates() {
    let mut db = FontDatabase::new();
    // A font that covers both Latin and CJK.
    db.add_face(make_face_with_ranges("PanFont", BIT_BASIC_LATIN | BIT_CJK));
    // Another CJK font (should not be added twice if PanFont already covers all).
    db.add_face(make_face_with_ranges("CJKFont", BIT_CJK));

    let chain = Query::new(&db)
        .family("PanFont")
        .match_with_fallback("A\u{4E2D}");

    let ids: Vec<u32> = chain.iter().map(|f| f.id).collect();
    let mut unique_ids = ids.clone();
    unique_ids.dedup();
    assert_eq!(
        ids, unique_ids,
        "fallback chain must not contain duplicate face IDs"
    );
}

/// A face with unknown ranges (0) is treated as covering everything,
/// so it appears as the single primary face for any text.
#[test]
fn test_match_with_fallback_unknown_ranges_covers_all() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_ranges("UnknownFont", 0));

    let chain = Query::new(&db)
        .family("UnknownFont")
        .match_with_fallback("Hello\u{4E2D}\u{0400}");

    assert_eq!(
        chain.len(),
        1,
        "a face with unicode_ranges=0 claims all coverage; chain must have exactly 1 entry"
    );
}

// ---------------------------------------------------------------------------
// Task 5 (supplementary): load from real fixture, verify unicode_ranges non-zero
// ---------------------------------------------------------------------------

static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

/// Verify that loading a real font file populates `unicode_ranges` with a
/// non-zero value (the fixture is a well-formed font with an OS/2 table).
#[test]
fn test_fixture_unicode_ranges_non_zero() {
    let mut db = FontDatabase::new();
    db.load_bytes(FIXTURE_BYTES.to_vec());
    assert!(
        !db.faces().is_empty(),
        "fixture must load at least one face"
    );
    let face = &db.faces()[0];
    assert_ne!(
        face.unicode_ranges, 0,
        "real font fixture must have non-zero unicode_ranges"
    );
}

/// Verify the fixture face covers ASCII characters via `covers_char_approx`.
#[test]
fn test_fixture_covers_ascii() {
    let mut db = FontDatabase::new();
    db.load_bytes(FIXTURE_BYTES.to_vec());
    assert!(
        !db.faces().is_empty(),
        "fixture must load at least one face"
    );
    let face = &db.faces()[0];
    assert!(
        face.covers_char_approx('A'),
        "fixture face must cover 'A' via covers_char_approx"
    );
}
