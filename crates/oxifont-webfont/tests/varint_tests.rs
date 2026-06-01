//! Integration tests for WOFF2 UIntBase128 varint encoding edge cases.
//!
//! `decode_uint_base128` is already unit-tested inside `woff2/header.rs` (inline
//! `#[cfg(test)]`).  These integration tests exercise the *public API surface*:
//!
//!  * The encode ↔ decode identity for every boundary value.
//!  * Decoder rejection of illegal encodings (leading-zero byte, overlong, empty).
//!  * That `encode_woff2` / `decode_woff2` do not panic on minimal SFNT input
//!    (which exercises the varint path through the table directory).

#[cfg(feature = "woff2")]
mod uint_base128 {
    use oxifont_webfont::woff2::encode::varint::encode_uint_base128;
    use oxifont_webfont::woff2::header::decode_uint_base128;
    use oxifont_webfont::WebFontError;

    // -----------------------------------------------------------------------
    // Encode ↔ decode round-trip: boundary values
    // -----------------------------------------------------------------------

    /// Every 7-bit group boundary value must survive a round-trip through the
    /// encoder and decoder unchanged.
    #[test]
    fn round_trip_boundary_values() {
        let values: &[u32] = &[
            0,
            1,
            0x7F,        // 127: maximum single-byte value
            0x80,        // 128: first two-byte value
            0xFF,        // 255
            0x3FFF,      // 16 383: maximum two-byte value
            0x4000,      // 16 384: first three-byte value
            0x1F_FFFF,   // 2 097 151: maximum three-byte value
            0x20_0000,   // 2 097 152: first four-byte value
            0x0FFF_FFFF, // maximum four-byte value
            0x1000_0000, // first five-byte value
        ];

        for &v in values {
            let mut encoded = Vec::new();
            encode_uint_base128(&mut encoded, v);

            // Must be 1–5 bytes.
            assert!(
                !encoded.is_empty(),
                "encoded must not be empty for value {v}"
            );
            assert!(
                encoded.len() <= 5,
                "encoded must not exceed 5 bytes for value {v}"
            );

            let (decoded, consumed) =
                decode_uint_base128(&encoded).expect("round-trip decode must succeed");

            assert_eq!(decoded, v, "decoded value must match original for {v}");
            assert_eq!(
                consumed,
                encoded.len(),
                "consumed bytes must equal encoded length for {v}"
            );
        }
    }

    /// Zero must encode as a single byte `[0x00]`.
    #[test]
    fn zero_encodes_as_single_byte() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 0);
        assert_eq!(out.len(), 1, "zero must encode as 1 byte");
        assert_eq!(out[0], 0x00);
        let (v, n) = decode_uint_base128(&out).expect("decode zero");
        assert_eq!(v, 0);
        assert_eq!(n, 1);
    }

    /// 127 (0x7F) encodes as a single byte.
    #[test]
    fn max_single_byte_value_127() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 127);
        assert_eq!(out.len(), 1, "127 must encode as 1 byte");
        assert_eq!(out[0], 0x7F);
        let (v, n) = decode_uint_base128(&out).expect("decode 127");
        assert_eq!(v, 127);
        assert_eq!(n, 1);
    }

    /// 128 must encode as exactly two bytes.
    #[test]
    fn first_two_byte_value_128() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 128);
        assert_eq!(out.len(), 2, "128 must encode as 2 bytes");
        // Expected: [0x81, 0x00] — continuation bit set on first byte.
        assert_eq!(out[0] & 0x80, 0x80, "first byte must have continuation bit");
        assert_eq!(
            out[1] & 0x80,
            0x00,
            "last byte must not have continuation bit"
        );
        let (v, n) = decode_uint_base128(&out).expect("decode 128");
        assert_eq!(v, 128);
        assert_eq!(n, 2);
    }

    /// A value requiring exactly 5 bytes must encode and decode correctly.
    #[test]
    fn five_byte_value_round_trip() {
        // 0x1000_0000 = 268 435 456 is the smallest value needing 5 bytes.
        let value: u32 = 0x1000_0000;
        let mut out = Vec::new();
        encode_uint_base128(&mut out, value);
        assert_eq!(out.len(), 5, "0x1000_0000 must encode as 5 bytes");

        let (v, n) = decode_uint_base128(&out).expect("decode 5-byte value");
        assert_eq!(v, value);
        assert_eq!(n, 5);
    }

    // -----------------------------------------------------------------------
    // Decoder rejection of illegal encodings
    // -----------------------------------------------------------------------

    /// An empty slice must be rejected with `TooShort`.
    #[test]
    fn decoder_rejects_empty_slice() {
        let result = decode_uint_base128(&[]);
        assert!(
            matches!(result, Err(WebFontError::TooShort)),
            "empty slice must produce TooShort, got {result:?}"
        );
    }

    /// A leading-zero byte (`0x80`) is invalid per the WOFF2 spec and must be
    /// rejected with `InvalidVarInt`.
    #[test]
    fn decoder_rejects_leading_zero_byte() {
        // 0x80 has the continuation bit set but contributes zero value bits —
        // this is the "leading zero" prohibited by the spec.
        let result = decode_uint_base128(&[0x80, 0x01]);
        assert!(
            matches!(result, Err(WebFontError::InvalidVarInt)),
            "leading 0x80 byte must produce InvalidVarInt, got {result:?}"
        );
    }

    /// A 6-byte encoding (all continuation bits set) must be rejected as an
    /// overflow — the value cannot fit in a u32 with 6 × 7 = 42 bits.
    #[test]
    fn decoder_rejects_six_byte_overlong_encoding() {
        // Six bytes all with continuation bit set — would be 42 bits, exceeds u32.
        let result = decode_uint_base128(&[0x81, 0x80, 0x80, 0x80, 0x80, 0x00]);
        assert!(
            result.is_err(),
            "6-byte encoding must be rejected as overflow"
        );
    }

    /// The decoder must stop at the first byte without the continuation bit,
    /// regardless of trailing data.
    #[test]
    fn decoder_stops_at_first_non_continuation_byte() {
        // 0x05 has no continuation bit → value = 5, consumed = 1.
        // The trailing 0xFF bytes must be ignored.
        let data = [0x05u8, 0xFF, 0xFF];
        let (v, n) = decode_uint_base128(&data).expect("should decode");
        assert_eq!(v, 5);
        assert_eq!(n, 1, "must consume only 1 byte");
    }

    // -----------------------------------------------------------------------
    // End-to-end: the varint path is exercised through encode_woff2/decode_woff2
    // -----------------------------------------------------------------------

    /// Encoding a minimal (0-table) SFNT must produce a valid WOFF2 file.
    /// The table directory varint path is exercised even with 0 tables.
    #[test]
    fn woff2_encode_decode_minimal_sfnt_does_not_panic() {
        use oxifont_webfont::{decode_woff2, encode_woff2};

        // Minimal 10-byte SFNT: sfVersion(4) + numTables=0(2) + padding(4)
        let sfnt: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&[0x00u8, 0x01, 0x00, 0x00]); // TrueType
            v.extend_from_slice(&[0x00u8, 0x00]); // numTables = 0
            v.extend_from_slice(&[0x00u8, 0x00]); // searchRange
            v.extend_from_slice(&[0x00u8, 0x00]); // entrySelector
            v.extend_from_slice(&[0x00u8, 0x00]); // rangeShift
            v
        };

        let encoded = encode_woff2(&sfnt).expect("encode_woff2 must succeed on minimal SFNT");
        assert!(
            encoded.starts_with(b"wOF2"),
            "WOFF2 magic bytes must be present"
        );

        let decoded =
            decode_woff2(&encoded).expect("decode_woff2 must succeed on minimal WOFF2 file");
        assert!(
            decoded.len() >= 10,
            "decoded SFNT must not be shorter than the original"
        );
    }

    /// Encoding a single-table SFNT ensures the UIntBase128 path in the table
    /// directory is exercised with a non-zero origLength field.
    #[test]
    fn woff2_round_trip_single_table_exercises_varint() {
        use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
        use oxifont_webfont::{decode_woff2, encode_woff2};

        let table_data = vec![0xABu8; 64]; // 64 bytes of 0xAB
        let sfnt =
            build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", table_data)]).expect("build_sfnt must succeed");

        let encoded = encode_woff2(&sfnt).expect("encode_woff2 must not fail");
        assert!(encoded.starts_with(b"wOF2"), "WOFF2 magic must be present");

        // The WOFF2 table directory contains a UIntBase128-encoded origLength.
        // Successful decode proves that path is correct.
        let decoded = decode_woff2(&encoded).expect("decode_woff2 must succeed");
        assert!(
            decoded.len() >= 12,
            "decoded SFNT must have at least offset table"
        );
    }
}
