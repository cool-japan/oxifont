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
// Noto CJK faces are never vendored (too large). Enabling a CJK feature exposes
// an accessor that returns REAL font bytes when a developer supplied one at build
// time, or a typed `FontError::NotFound` otherwise. It must NEVER hand out empty
// or fabricated bytes. These tests assert that honest contract regardless of
// whether a font happens to be present in the current build.

/// Assert the honest CJK contract for one accessor / provider name pair.
#[cfg(any(
    feature = "bundled-noto-cjk-jp",
    feature = "bundled-noto-cjk-kr",
    feature = "bundled-noto-cjk-sc",
    feature = "bundled-noto-cjk-tc",
))]
fn assert_cjk_contract(
    accessor: Result<&'static [u8], oxifont_core::FontError>,
    provider_name: &str,
) {
    // The accessor either yields a real, valid font or a typed "not found" error
    // — never an empty or fake slice.
    match accessor {
        Ok(bytes) => {
            assert!(
                bytes.len() > 1024,
                "{provider_name}: bundled CJK font must be a substantial file, got {} bytes",
                bytes.len()
            );
            assert!(
                is_valid_sfnt_magic(bytes),
                "{provider_name}: bundled CJK font must have a valid SFNT magic"
            );
        }
        Err(e) => {
            assert!(
                matches!(e, oxifont_core::FontError::NotFound),
                "{provider_name}: not-bundled must be a typed FontError::NotFound, got {e:?}"
            );
        }
    }

    // The provider must still expose the Latin faces, and must never list a CJK
    // entry with empty or invalid bytes.
    let provider = BundledFontProvider::new();
    let names: Vec<&str> = provider.font_data().iter().map(|(n, _)| *n).collect();
    assert!(
        names.contains(&"NotoSans-Regular"),
        "Latin NotoSans-Regular must remain available; got {names:?}"
    );
    if let Some(bytes) = provider.by_name(provider_name) {
        assert!(
            !bytes.is_empty(),
            "{provider_name}: provider must never expose an empty CJK slice"
        );
        assert!(
            is_valid_sfnt_magic(bytes),
            "{provider_name}: provider CJK bytes must have a valid SFNT magic"
        );
    }
}

#[test]
#[cfg(feature = "bundled-noto-cjk-jp")]
fn cjk_jp_resolves_to_real_font_or_typed_error() {
    assert_cjk_contract(
        oxifont_bundled::noto_sans_jp_regular(),
        "NotoSansJP-Regular",
    );
}

#[test]
#[cfg(feature = "bundled-noto-cjk-kr")]
fn cjk_kr_resolves_to_real_font_or_typed_error() {
    assert_cjk_contract(
        oxifont_bundled::noto_sans_kr_regular(),
        "NotoSansKR-Regular",
    );
}

#[test]
#[cfg(feature = "bundled-noto-cjk-sc")]
fn cjk_sc_resolves_to_real_font_or_typed_error() {
    assert_cjk_contract(
        oxifont_bundled::noto_sans_sc_regular(),
        "NotoSansSC-Regular",
    );
}

#[test]
#[cfg(feature = "bundled-noto-cjk-tc")]
fn cjk_tc_resolves_to_real_font_or_typed_error() {
    assert_cjk_contract(
        oxifont_bundled::noto_sans_tc_regular(),
        "NotoSansTC-Regular",
    );
}
