//! WOFF1 round-trip integration tests using the encode→decode pair.
//!
//! These tests do not require fixture `.woff` files; they synthesise a minimal
//! SFNT and, where possible, find a real TTF on the host system.

// ----------------------------------------------------------------- helpers

/// Build a minimal valid SFNT with 0 tables.
///
/// Layout (14 bytes):
/// - sfVersion  u32 = 0x00010000 (TrueType)
/// - numTables  u16 = 0
/// - searchRange u16 = 0
/// - entrySelector u16 = 0
/// - rangeShift u16 = 0
fn minimal_sfnt() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0x00u8, 0x01, 0x00, 0x00]); // sfVersion: TrueType
    buf.extend_from_slice(&[0x00u8, 0x00]); // numTables = 0
    buf.extend_from_slice(&[0x00u8, 0x00]); // searchRange
    buf.extend_from_slice(&[0x00u8, 0x00]); // entrySelector
    buf.extend_from_slice(&[0x00u8, 0x00]); // rangeShift
    buf
}

/// Search common system font directories for any `.ttf` file.
///
/// Returns the raw bytes of the first readable TTF found, or `None` if the
/// host system has no TTF fonts in the usual locations.
fn find_ttf_on_system() -> Option<Vec<u8>> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
    ];
    for dir in &dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());
            if ext.as_deref() == Some("ttf") {
                if let Ok(data) = std::fs::read(&p) {
                    return Some(data);
                }
            }
        }
    }
    None
}

// ------------------------------------------------------------------- tests

#[cfg(feature = "woff1")]
#[test]
fn test_woff1_encode_decode_minimal_sfnt() {
    use oxifont_webfont::{decode_woff1, encode_woff1};

    let sfnt = minimal_sfnt();
    let encoded = encode_woff1(&sfnt).expect("encode_woff1 should succeed on minimal SFNT");

    // Must start with WOFF1 magic bytes "wOFF".
    assert!(
        encoded.starts_with(b"wOFF"),
        "WOFF1 magic bytes must be present"
    );

    let decoded = decode_woff1(&encoded).expect("decode_woff1 should succeed");

    // The decoded SFNT must at least have a valid offset table.
    assert!(
        decoded.len() >= 12,
        "decoded SFNT must have at least offset table"
    );

    // numTables in the decoded SFNT must match the original.
    let num_tables_orig = u16::from_be_bytes([sfnt[4], sfnt[5]]);
    let num_tables_dec = u16::from_be_bytes([decoded[4], decoded[5]]);
    assert_eq!(
        num_tables_orig, num_tables_dec,
        "numTables must survive round-trip (minimal SFNT)"
    );
}

#[cfg(feature = "woff1")]
#[test]
fn test_woff1_encode_decode_real_ttf() {
    use oxifont_webfont::{decode_woff1, encode_woff1};

    let Some(data) = find_ttf_on_system() else {
        eprintln!("SKIP test_woff1_encode_decode_real_ttf: no TTF font found on this system");
        return;
    };

    let encoded = encode_woff1(&data).expect("encode_woff1 should succeed on real TTF");
    assert!(
        encoded.starts_with(b"wOFF"),
        "WOFF1 magic bytes must be present"
    );

    let decoded = decode_woff1(&encoded).expect("decode_woff1 should succeed on real TTF");
    assert!(
        decoded.len() >= 12,
        "decoded SFNT must have at least offset table"
    );

    // Table count must be preserved.
    let num_tables_orig = u16::from_be_bytes([data[4], data[5]]);
    let num_tables_dec = u16::from_be_bytes([decoded[4], decoded[5]]);
    assert_eq!(
        num_tables_orig, num_tables_dec,
        "numTables must survive round-trip (real TTF)"
    );
}

#[cfg(feature = "woff1")]
#[test]
fn test_woff1_encoded_is_smaller_or_equal() {
    use oxifont_webfont::encode_woff1;

    let Some(data) = find_ttf_on_system() else {
        eprintln!("SKIP test_woff1_encoded_is_smaller_or_equal: no TTF font found on this system");
        return;
    };

    let encoded = encode_woff1(&data).expect("encode_woff1 should succeed");
    // WOFF1 should not be more than 2× the original size.
    assert!(
        encoded.len() <= data.len() * 2,
        "WOFF1 encoded size ({}) must not exceed 2× input size ({})",
        encoded.len(),
        data.len()
    );
}

/// Verify that a raw SFNT (no WOFF1 magic) is rejected gracefully by the decoder.
///
/// Passing a plain SFNT to `decode_woff1` must return an `Err`, not panic or
/// silently succeed, because the WOFF1 decoder validates the `wOFF` magic header.
#[cfg(feature = "woff1")]
#[test]
fn test_woff1_magic_rejected_by_decoder_directly() {
    use oxifont_webfont::decode_woff1;

    let sfnt = minimal_sfnt();
    let result = decode_woff1(&sfnt);
    assert!(
        result.is_err(),
        "raw SFNT (no wOFF magic) must be rejected by decode_woff1 with Err"
    );
}

/// Verify byte-identical round-trip for the minimal zero-table SFNT.
///
/// The minimal SFNT has no tables, so the encoder and decoder have nothing to
/// reorder or repad; the decoded output must be byte-for-byte identical to the
/// input.
#[cfg(feature = "woff1")]
#[test]
fn test_woff1_round_trip_minimal_byte_identical() {
    use oxifont_webfont::{decode_woff1, encode_woff1};

    let sfnt = minimal_sfnt();
    let encoded = encode_woff1(&sfnt).expect("encode_woff1 must succeed on minimal SFNT");
    assert!(encoded.starts_with(b"wOFF"), "WOFF1 magic must be present");

    let decoded = decode_woff1(&encoded).expect("decode_woff1 must succeed");
    assert_eq!(
        decoded, sfnt,
        "minimal SFNT (0 tables) must survive WOFF1 round-trip byte-identically"
    );
}
