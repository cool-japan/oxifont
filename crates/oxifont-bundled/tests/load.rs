//! Integration tests for oxifont-bundled.
//!
//! Run with:
//!   cargo test -p oxifont-bundled --features bundled-noto
//!   cargo test -p oxifont-bundled --features bundled-noto-cjk-jp

#[cfg(any(
    feature = "bundled-noto",
    feature = "bundled-noto-cjk-jp",
    feature = "bundled-noto-cjk-kr",
    feature = "bundled-noto-cjk-sc",
    feature = "bundled-noto-cjk-tc",
))]
use oxifont_bundled::provider::BundledFontProvider;

// ── TTF magic-byte helpers ────────────────────────────────────────────────────

/// Returns `true` when `bytes` starts with a recognised OpenType/TTF signature.
#[cfg(any(
    feature = "bundled-noto",
    feature = "bundled-noto-cjk-jp",
    feature = "bundled-noto-cjk-kr",
    feature = "bundled-noto-cjk-sc",
    feature = "bundled-noto-cjk-tc",
))]
fn is_valid_sfnt_magic(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let magic = &bytes[..4];
    magic == [0x00, 0x01, 0x00, 0x00] // TrueType
        || magic == b"OTTO"            // CFF / OpenType CFF
        || magic == b"ttcf" // TrueType Collection
}

// ── Core bundled-noto tests ───────────────────────────────────────────────────

#[test]
#[cfg(feature = "bundled-noto")]
fn bundled_noto_provider_is_nonempty() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    assert!(
        !fonts.is_empty(),
        "font_data() must return at least one entry"
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn noto_sans_regular_is_present() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(
        names.contains(&"NotoSans-Regular"),
        "Expected NotoSans-Regular in font_data(); got: {:?}",
        names
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn noto_serif_regular_is_present() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(
        names.contains(&"NotoSerif-Regular"),
        "Expected NotoSerif-Regular in font_data(); got: {:?}",
        names
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn noto_sans_regular_ttf_magic() {
    let bytes = oxifont_bundled::NOTO_SANS_REGULAR;
    assert!(bytes.len() > 1024, "NotoSans-Regular must be > 1 KB");
    assert!(
        is_valid_sfnt_magic(bytes),
        "NotoSans-Regular does not start with a valid SFNT magic: {:?}",
        &bytes[..4.min(bytes.len())]
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn noto_serif_regular_ttf_magic() {
    let bytes = oxifont_bundled::NOTO_SERIF_REGULAR;
    assert!(bytes.len() > 1024, "NotoSerif-Regular must be > 1 KB");
    assert!(
        is_valid_sfnt_magic(bytes),
        "NotoSerif-Regular does not start with a valid SFNT magic: {:?}",
        &bytes[..4.min(bytes.len())]
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn ofl_license_is_present_and_mentions_sil() {
    let license = BundledFontProvider::ofl_license_text();
    assert!(!license.is_empty(), "OFL license text must not be empty");
    assert!(
        license.contains("SIL") || license.contains("Open Font License"),
        "License text must mention SIL or Open Font License"
    );
}

#[test]
#[cfg(feature = "bundled-noto")]
fn by_name_returns_noto_sans() {
    let provider = BundledFontProvider::new();
    let bytes = provider
        .by_name("NotoSans-Regular")
        .expect("by_name(\"NotoSans-Regular\") should not return None");
    assert!(bytes.len() > 1024);
    assert!(is_valid_sfnt_magic(bytes));
}

#[test]
#[cfg(feature = "bundled-noto")]
fn by_name_unknown_returns_none() {
    let provider = BundledFontProvider::new();
    assert!(
        provider.by_name("DoesNotExist-Regular").is_none(),
        "by_name with unknown key must return None"
    );
}

// ── CJK feature tests ─────────────────────────────────────────────────────────
//
// When CJK fonts are zero-byte placeholders, the provider silently omits them
// from font_data(). These tests verify that behaviour rather than asserting
// the font is present with valid magic bytes.

#[test]
#[cfg(feature = "bundled-noto-cjk-jp")]
fn cjk_jp_feature_compiles_and_provider_works() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    // NotoSans-Regular and NotoSerif-Regular must still be present.
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(
        names.contains(&"NotoSans-Regular"),
        "bundled-noto-cjk-jp should still include NotoSans-Regular"
    );
    // JP font is a placeholder; if present in font_data it must be valid.
    if let Some(jp_bytes) = provider.by_name("NotoSansJP-Regular") {
        assert!(
            is_valid_sfnt_magic(jp_bytes),
            "NotoSansJP-Regular has invalid SFNT magic"
        );
    }
    // Static is accessible (zero-byte is fine for placeholder).
    let _ = oxifont_bundled::NOTO_SANS_JP_REGULAR;
}

#[test]
#[cfg(feature = "bundled-noto-cjk-kr")]
fn cjk_kr_feature_compiles_and_provider_works() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"NotoSans-Regular"));
    if let Some(kr_bytes) = provider.by_name("NotoSansKR-Regular") {
        assert!(is_valid_sfnt_magic(kr_bytes));
    }
    let _ = oxifont_bundled::NOTO_SANS_KR_REGULAR;
}

#[test]
#[cfg(feature = "bundled-noto-cjk-sc")]
fn cjk_sc_feature_compiles_and_provider_works() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"NotoSans-Regular"));
    if let Some(sc_bytes) = provider.by_name("NotoSansSC-Regular") {
        assert!(is_valid_sfnt_magic(sc_bytes));
    }
    let _ = oxifont_bundled::NOTO_SANS_SC_REGULAR;
}

#[test]
#[cfg(feature = "bundled-noto-cjk-tc")]
fn cjk_tc_feature_compiles_and_provider_works() {
    let provider = BundledFontProvider::new();
    let fonts = provider.font_data();
    let names: Vec<&str> = fonts.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"NotoSans-Regular"));
    if let Some(tc_bytes) = provider.by_name("NotoSansTC-Regular") {
        assert!(is_valid_sfnt_magic(tc_bytes));
    }
    let _ = oxifont_bundled::NOTO_SANS_TC_REGULAR;
}
