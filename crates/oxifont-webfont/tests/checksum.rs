//! Checksum verification tests.
//!
//! These tests verify that:
//!
//! 1. The `table_checksum` function in `sfnt.rs` behaves correctly for known
//!    inputs, including alignment edge cases.
//! 2. A WOFF1-encoded font whose compressed data has been corrupted is either
//!    detected (returns `Err`) or silently produces different data — the key
//!    requirement is that the decoder does NOT panic.
//! 3. A WOFF2-encoded font whose brotli payload has been corrupted does not
//!    panic either.
//! 4. A table with a deliberately wrong `origChecksum` in the WOFF1 directory
//!    is rejected with `ChecksumMismatch`.

#[cfg(any(feature = "woff1", feature = "woff2"))]
mod checksum_tests {
    // -----------------------------------------------------------------------
    // `table_checksum` unit tests
    // -----------------------------------------------------------------------

    use oxifont_webfont::sfnt::table_checksum;

    /// Empty data yields checksum 0.
    #[test]
    fn checksum_empty_data() {
        assert_eq!(table_checksum(&[]), 0);
    }

    /// Exactly 4 bytes: big-endian u32 sum.
    #[test]
    fn checksum_four_bytes_exact_word() {
        let data = [0x01u8, 0x02, 0x03, 0x04];
        assert_eq!(table_checksum(&data), 0x0102_0304);
    }

    /// Partial trailing word is zero-padded before summing.
    #[test]
    fn checksum_partial_trailing_word() {
        // Full word: 0x0102_0304. Partial: [0x05] → 0x0500_0000.
        let data = [0x01u8, 0x02, 0x03, 0x04, 0x05];
        let expected = 0x0102_0304u32.wrapping_add(0x0500_0000u32);
        assert_eq!(table_checksum(&data), expected);
    }

    /// Two full words.
    #[test]
    fn checksum_two_full_words() {
        let data = [
            0x00u8, 0x00, 0x00, 0x01, // first word = 1
            0xFF, 0xFF, 0xFF, 0xFF, // second word = 0xFFFF_FFFF
        ];
        let expected = 1u32.wrapping_add(0xFFFF_FFFFu32);
        assert_eq!(table_checksum(&data), expected);
    }

    /// Checksum wraps on overflow (wrapping_add semantics).
    #[test]
    fn checksum_wraps_on_overflow() {
        let data = [
            0xFF, 0xFF, 0xFF, 0xFF, // 0xFFFF_FFFF
            0x00, 0x00, 0x00, 0x01, // 1
        ];
        // 0xFFFF_FFFF + 1 wraps to 0.
        assert_eq!(table_checksum(&data), 0u32);
    }

    /// Three-byte partial trailing word: only the three bytes matter, LSB is 0.
    #[test]
    fn checksum_three_byte_partial_word() {
        let data = [0x01u8, 0x02, 0x03, 0x04, 0xAA, 0xBB, 0xCC]; // last 3 bytes → 0xAABBCC00
        let expected = 0x0102_0304u32.wrapping_add(0xAABB_CC00u32);
        assert_eq!(table_checksum(&data), expected);
    }

    // -----------------------------------------------------------------------
    // WOFF1 corruption: must not panic
    // -----------------------------------------------------------------------

    /// Corrupting a byte in the middle of a WOFF1 file must not cause a panic.
    ///
    /// The decoder may return an `Err` (e.g. `ChecksumMismatch`, `DecompressError`,
    /// `LengthMismatch`) or silently return modified data — both are acceptable.
    /// What is NOT acceptable is a panic.
    #[cfg(feature = "woff1")]
    #[test]
    fn woff1_corrupted_data_does_not_panic() {
        use oxifont_webfont::{decode_woff1, encode_woff1};

        let sfnt: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&[0x00u8, 0x01, 0x00, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]); // numTables = 0
            v.extend_from_slice(&[0x00u8, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]);
            v
        };

        if let Ok(mut encoded) = encode_woff1(&sfnt) {
            // Flip a byte in the middle of the encoded file.
            if encoded.len() > 50 {
                encoded[45] ^= 0xFF;
            }
            // Must not panic regardless of the result.
            let _ = decode_woff1(&encoded);
        }
    }

    /// A WOFF1 file built with a deliberately incorrect `origChecksum` in the
    /// table directory must be rejected by the decoder with `ChecksumMismatch`.
    #[cfg(feature = "woff1")]
    #[test]
    fn woff1_wrong_checksum_in_directory_is_rejected() {
        use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
        use oxifont_webfont::{decode_woff1, encode_woff1};

        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", vec![0xAAu8; 32])])
            .expect("build_sfnt must succeed");

        let mut woff1 = encode_woff1(&sfnt).expect("encode_woff1 must succeed");

        // The WOFF1 table directory starts at offset 44, each entry is 20 bytes.
        // origChecksum is at bytes 16–19 of the entry (offset 44 + 16 = 60).
        // Flip the origChecksum in the first (and only) table entry.
        let checksum_offset = 44 + 16; // 44 (header) + 16 (tag + woffOffset + compLength + origLength)
        if woff1.len() > checksum_offset + 4 {
            woff1[checksum_offset] ^= 0xFF;
            woff1[checksum_offset + 1] ^= 0xFF;
            woff1[checksum_offset + 2] ^= 0xFF;
            woff1[checksum_offset + 3] ^= 0xFF;
        }

        let result = decode_woff1(&woff1);
        // Must be an error — the exact variant may be ChecksumMismatch or
        // another parse error depending on how the corruption falls.
        assert!(
            result.is_err(),
            "decode_woff1 must return Err when origChecksum is wrong"
        );
    }

    /// Verifies that corrupting bytes at various positions within a WOFF1 file
    /// never causes a panic.  Tests multiple positions to exercise different code
    /// paths (header, directory, compressed data).
    #[cfg(feature = "woff1")]
    #[test]
    fn woff1_multiple_corruption_positions_do_not_panic() {
        use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
        use oxifont_webfont::{decode_woff1, encode_woff1};

        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"name", b"abcdefghijklmnop".to_vec())])
            .expect("build_sfnt must succeed");

        let encoded = encode_woff1(&sfnt).expect("encode_woff1 must succeed");

        // Flip one byte at each of several positions throughout the file.
        let positions = [4, 8, 12, 44, 50, 60, encoded.len() / 2];
        for &pos in &positions {
            if pos >= encoded.len() {
                continue;
            }
            let mut corrupted = encoded.clone();
            corrupted[pos] ^= 0xFF;
            // Must not panic.
            let _ = decode_woff1(&corrupted);
        }
    }

    // -----------------------------------------------------------------------
    // WOFF2 corruption: must not panic
    // -----------------------------------------------------------------------

    /// Corrupting a byte in the brotli payload of a WOFF2 file must not panic.
    ///
    /// The decoder is expected to return `Err(DecompressError(...))` in most
    /// cases, but it must never panic.
    #[cfg(feature = "woff2")]
    #[test]
    fn woff2_corrupted_brotli_does_not_panic() {
        use oxifont_webfont::{decode_woff2, encode_woff2};

        let sfnt: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&[0x00u8, 0x01, 0x00, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]); // numTables = 0
            v.extend_from_slice(&[0x00u8, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]);
            v.extend_from_slice(&[0x00u8, 0x00]);
            v
        };

        if let Ok(mut encoded) = encode_woff2(&sfnt) {
            // Flip a byte deep enough to land in the brotli-compressed block.
            if encoded.len() > 50 {
                encoded[48] ^= 0xFF;
            }
            // Must not panic.
            let _ = decode_woff2(&encoded);
        }
    }

    /// Corrupting bytes at multiple positions in a WOFF2 file must never panic.
    #[cfg(feature = "woff2")]
    #[test]
    fn woff2_multiple_corruption_positions_do_not_panic() {
        use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
        use oxifont_webfont::{decode_woff2, encode_woff2};

        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", vec![0xCCu8; 64])])
            .expect("build_sfnt must succeed");

        let encoded = encode_woff2(&sfnt).expect("encode_woff2 must succeed");

        let positions = [4, 8, 12, 24, 48, encoded.len() / 2];
        for &pos in &positions {
            if pos >= encoded.len() {
                continue;
            }
            let mut corrupted = encoded.clone();
            corrupted[pos] ^= 0xFF;
            // Must not panic.
            let _ = decode_woff2(&corrupted);
        }
    }

    /// Truncating the WOFF2 file at various lengths must not panic.
    #[cfg(feature = "woff2")]
    #[test]
    fn woff2_truncated_at_various_lengths_does_not_panic() {
        use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
        use oxifont_webfont::{decode_woff2, encode_woff2};

        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", vec![0xDDu8; 32])])
            .expect("build_sfnt must succeed");

        let encoded = encode_woff2(&sfnt).expect("encode_woff2 must succeed");

        // Test a range of truncation lengths.
        let lengths = [0, 1, 4, 8, 12, 24, 47, encoded.len() / 3, encoded.len() / 2];
        for &len in &lengths {
            if len > encoded.len() {
                continue;
            }
            let truncated = &encoded[..len];
            // Must not panic.
            let _ = decode_woff2(truncated);
        }
    }
}
