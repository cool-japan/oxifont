//! Tests for BCP-47 to Windows LCID mapping.

use oxifont_db::locale::bcp47_to_lcid;

#[test]
fn ja_jp_maps_to_0x0411() {
    assert_eq!(bcp47_to_lcid("ja-JP"), Some(0x0411));
}

#[test]
fn ja_jp_lowercase_maps_to_0x0411() {
    assert_eq!(bcp47_to_lcid("ja-jp"), Some(0x0411));
}

#[test]
fn en_us_maps_to_0x0409() {
    assert_eq!(bcp47_to_lcid("en-US"), Some(0x0409));
}

#[test]
fn en_gb_maps_to_0x0809() {
    assert_eq!(bcp47_to_lcid("en-GB"), Some(0x0809));
}

#[test]
fn zh_cn_maps_to_0x0804() {
    assert_eq!(bcp47_to_lcid("zh-CN"), Some(0x0804));
}

#[test]
fn ko_kr_maps_to_0x0412() {
    assert_eq!(bcp47_to_lcid("ko-KR"), Some(0x0412));
}

#[test]
fn de_de_maps_to_0x0407() {
    assert_eq!(bcp47_to_lcid("de-DE"), Some(0x0407));
}

#[test]
fn unknown_locale_returns_none() {
    assert_eq!(bcp47_to_lcid("xx-ZZ"), None);
}

#[test]
fn empty_locale_returns_none() {
    assert_eq!(bcp47_to_lcid(""), None);
}

#[test]
fn mixed_case_normalised_correctly() {
    // BCP-47 tags are case-insensitive; our lookup must normalise.
    assert_eq!(bcp47_to_lcid("JA-JP"), Some(0x0411));
    assert_eq!(bcp47_to_lcid("EN-US"), Some(0x0409));
}

// ---------------------------------------------------------------------------
// Smoke test: locale family names from a real font
// ---------------------------------------------------------------------------

static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

#[test]
fn fixture_family_for_en_us_returns_non_empty() {
    let mut db = oxifont_db::FontDatabase::new();
    db.load_bytes(FIXTURE_BYTES.to_vec());
    assert!(!db.faces().is_empty(), "fixture must load");
    let face = &db.faces()[0];
    // family_for_locale falls back to the canonical family when no locale
    // record matches — either way, the result must be non-empty.
    let name = face.family_for_locale("en-US");
    assert!(
        !name.is_empty(),
        "family_for_locale('en-US') must return a non-empty string"
    );
}
