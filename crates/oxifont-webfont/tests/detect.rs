//! Integration tests for detect_format / decode_auto / DecodeResult API.

use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_CFF, SFNT_MAGIC_TT};
use oxifont_webfont::{decode_auto, detect_format, FontFormat};

// --------------------------------------------------------------- detect_format

#[test]
fn detect_woff1_magic() {
    // wOFF = 0x774F4646
    let data = [0x77u8, 0x4F, 0x46, 0x46, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Woff1);
}

#[test]
fn detect_woff2_magic() {
    // wOF2 = 0x774F4632
    let data = [0x77u8, 0x4F, 0x46, 0x32, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Woff2);
}

#[test]
fn detect_sfnt_tt_magic() {
    // 0x00010000 = TrueType
    let data = [0x00u8, 0x01, 0x00, 0x00, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Sfnt);
}

#[test]
fn detect_sfnt_cff_magic() {
    // OTTO = 0x4F54544F
    let data = [0x4Fu8, 0x54, 0x54, 0x4F, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Sfnt);
}

#[test]
fn detect_sfnt_true_magic() {
    // "true" = 0x74727565
    let data = [0x74u8, 0x72, 0x75, 0x65, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Sfnt);
}

#[test]
fn detect_sfnt_ttcf_magic() {
    // "ttcf" = 0x74746366
    let data = [0x74u8, 0x74, 0x63, 0x66, 0, 0, 0, 0];
    assert_eq!(detect_format(&data), FontFormat::Sfnt);
}

#[test]
fn detect_empty_slice_never_panics() {
    // Must not panic; should return Unknown.
    let result = detect_format(&[]);
    assert_eq!(result, FontFormat::Unknown);
}

#[test]
fn detect_one_byte_never_panics() {
    let result = detect_format(&[0xFF]);
    assert_eq!(result, FontFormat::Unknown);
}

#[test]
fn detect_three_bytes_never_panics() {
    let result = detect_format(&[0x77, 0x4F, 0x46]);
    assert_eq!(result, FontFormat::Unknown);
}

#[test]
fn detect_garbage_is_unknown() {
    let data = [0xFFu8, 0xFE, 0xFD, 0xFC];
    assert_eq!(detect_format(&data), FontFormat::Unknown);
}

// ---------------------------------------------------------------- decode_auto

#[test]
fn decode_auto_sfnt_passthrough() {
    let sfnt = build_sfnt(SFNT_MAGIC_TT, &[]).expect("empty SFNT");
    let result = decode_auto(&sfnt).expect("decode_auto SFNT should succeed");
    assert_eq!(result.sfnt, sfnt);
    assert!(result.metadata.is_none());
}

#[test]
fn decode_auto_sfnt_cff_passthrough() {
    let sfnt = build_sfnt(SFNT_MAGIC_CFF, &[]).expect("empty CFF SFNT");
    let result = decode_auto(&sfnt).expect("decode_auto CFF SFNT should succeed");
    assert_eq!(result.sfnt, sfnt);
}

#[test]
fn decode_auto_unknown_returns_error() {
    let data = [0xFFu8, 0xFF, 0xFF, 0xFF];
    let result = decode_auto(&data);
    assert!(
        matches!(result, Err(oxifont_webfont::WebFontError::InvalidSignature)),
        "expected InvalidSignature, got {:?}",
        result.err()
    );
}

#[test]
fn decode_auto_empty_returns_error() {
    let result = decode_auto(&[]);
    assert!(result.is_err(), "decode_auto on empty should fail");
}

#[cfg(feature = "woff1")]
#[test]
fn decode_auto_woff1_roundtrip() {
    use oxifont_webfont::encode_woff1;

    let sfnt =
        build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", vec![0u8; 6])]).expect("build_sfnt should succeed");

    let woff1 = encode_woff1(&sfnt).expect("encode_woff1 should succeed");
    let result = decode_auto(&woff1).expect("decode_auto woff1 should succeed");
    assert_eq!(result.metadata, None);
    assert!(result.sfnt.len() >= 12);
}

#[cfg(feature = "woff2")]
#[test]
fn decode_auto_woff2_roundtrip() {
    use oxifont_webfont::encode_woff2;

    let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"name", b"Test".to_vec())])
        .expect("build_sfnt should succeed");

    let woff2 = encode_woff2(&sfnt).expect("encode_woff2 should succeed");
    let result = decode_auto(&woff2).expect("decode_auto woff2 should succeed");
    assert_eq!(result.metadata, None);
    assert!(result.sfnt.len() >= 12);
}
