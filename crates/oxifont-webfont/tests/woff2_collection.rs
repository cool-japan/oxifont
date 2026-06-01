/// WOFF2 font collection (TTC-in-WOFF2) decoding tests.
///
/// A real TTC-in-WOFF2 fixture requires a file we don't have; these tests
/// verify API shape and error-handling behaviour.
#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_rejects_non_collection() {
    // A standard WOFF2 (not ttcf) should return Err from decode_woff2_collection.
    let minimal_sfnt = vec![
        0x00u8, 0x01, 0x00, 0x00, // TrueType
        0x00, 0x00, // numTables=0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // padding
    ];
    if let Ok(woff2) = oxifont_webfont::encode_woff2(&minimal_sfnt) {
        let result = oxifont_webfont::decode_woff2_collection(&woff2);
        assert!(
            result.is_err(),
            "single-font WOFF2 should not decode as collection"
        );
    }
}

#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_short_input() {
    let result = oxifont_webfont::decode_woff2_collection(&[0u8; 20]);
    assert!(result.is_err(), "short input must return Err");
}

#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_empty_input() {
    let result = oxifont_webfont::decode_woff2_collection(&[]);
    assert!(result.is_err(), "empty input must return Err");
}

#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_wrong_signature() {
    // 48 bytes with a non-WOFF2 signature should fail at header parse.
    let data = vec![0u8; 48];
    let result = oxifont_webfont::decode_woff2_collection(&data);
    assert!(result.is_err(), "invalid signature must return Err");
}
