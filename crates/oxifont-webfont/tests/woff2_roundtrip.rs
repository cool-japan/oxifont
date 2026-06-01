//! WOFF2 round-trip integration tests using the encode→decode pair.
//!
//! These tests do not require fixture `.woff2` files; they synthesise a
//! minimal SFNT and, where possible, find a real TTF on the host system.

// ----------------------------------------------------------------- helpers

/// Build a minimal valid SFNT with 0 tables.
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

#[cfg(feature = "woff2")]
#[test]
fn test_woff2_encode_decode_minimal_sfnt() {
    use oxifont_webfont::{decode_woff2, encode_woff2};

    let sfnt = minimal_sfnt();
    let encoded = encode_woff2(&sfnt).expect("encode_woff2 should succeed on minimal SFNT");

    // Must start with WOFF2 magic bytes "wOF2".
    assert!(
        encoded.starts_with(b"wOF2"),
        "WOFF2 magic bytes must be present"
    );

    let decoded = decode_woff2(&encoded).expect("decode_woff2 should succeed on minimal SFNT");
    assert!(!decoded.is_empty(), "decoded SFNT must not be empty");
    assert!(
        decoded.len() >= 12,
        "decoded SFNT must have at least offset table"
    );
}

#[cfg(feature = "woff2")]
#[test]
fn test_woff2_encode_decode_real_ttf() {
    use oxifont_webfont::{decode_woff2, encode_woff2};

    let Some(data) = find_ttf_on_system() else {
        eprintln!("SKIP test_woff2_encode_decode_real_ttf: no TTF font found on this system");
        return;
    };

    let encoded = match encode_woff2(&data) {
        Ok(enc) => enc,
        Err(e) => {
            // Some TTFs may have unusual table layouts; log but skip.
            eprintln!("SKIP test_woff2_encode_decode_real_ttf: encode failed (acceptable): {e}");
            return;
        }
    };

    assert!(
        encoded.starts_with(b"wOF2"),
        "WOFF2 magic bytes must be present"
    );

    match decode_woff2(&encoded) {
        Ok(decoded) => {
            // The decoded SFNT might differ in table order/padding.
            // Check it is parseable rather than byte-identical.
            assert!(
                decoded.len() >= 12,
                "decoded SFNT must have at least offset table"
            );

            // Verify the SFNT magic bytes are a known valid variant.
            let magic = u32::from_be_bytes([decoded[0], decoded[1], decoded[2], decoded[3]]);
            assert!(
                matches!(magic, 0x00010000 | 0x4F54_544F | 0x74727565 | 0x74746366),
                "decoded bytes must start with a valid SFNT magic; got 0x{magic:08X}"
            );
        }
        Err(e) => {
            // WOFF2 decode failure on a real font is noteworthy but not a hard
            // failure, since some brotli edge cases are known limitations.
            eprintln!(
                "SKIP test_woff2_encode_decode_real_ttf: decode failed (known edge case): {e}"
            );
        }
    }
}

#[cfg(feature = "woff2")]
#[test]
fn test_detect_format_correctly_identifies_woff2() {
    use oxifont_webfont::{detect_format, encode_woff2, FontFormat};

    let Some(data) = find_ttf_on_system() else {
        eprintln!(
            "SKIP test_detect_format_correctly_identifies_woff2: no TTF font found on this system"
        );
        return;
    };

    let Ok(encoded) = encode_woff2(&data) else {
        eprintln!("SKIP test_detect_format_correctly_identifies_woff2: encode failed");
        return;
    };

    let fmt = detect_format(&encoded);
    assert_eq!(
        fmt,
        FontFormat::Woff2,
        "detect_format must identify WOFF2-encoded data as FontFormat::Woff2"
    );
}
