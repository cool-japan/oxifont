//! Tests for WOFF1 uncompressed table handling (comp_length == orig_length).
//!
//! The WOFF1 encoder stores a table uncompressed when zlib compression does not
//! produce a smaller result.  The decoder must handle both the compressed path
//! (`comp_length < orig_length`) and the uncompressed path
//! (`comp_length == orig_length`) without error.
//!
//! Reference: W3C WOFF 1.0 specification §5.3

#[cfg(feature = "woff1")]
mod woff1_uncompressed {
    use oxifont_webfont::sfnt::{build_sfnt, table_checksum, SFNT_MAGIC_TT};
    use oxifont_webfont::{decode_woff1, encode_woff1};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Generate pseudo-random bytes that compress poorly.
    ///
    /// Uses a simple linear congruential generator so the sequence is
    /// deterministic but lacks repeated patterns.
    fn incompressible_bytes(len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut state: u64 = 0x1234_5678_9ABC_DEF0;
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            out.push((state >> 33) as u8);
        }
        out
    }

    /// Build a minimal WOFF1 file from raw bytes that bypasses the encoder's
    /// compression step, forcing comp_length == orig_length in the directory.
    ///
    /// This lets us verify that the decoder accepts uncompressed table storage
    /// even when the data itself is compressible (the spec only requires that
    /// comp_length <= orig_length when compressed; == means "stored raw").
    fn build_woff1_with_uncompressed_table(tag: &[u8; 4], table_data: &[u8]) -> Vec<u8> {
        // WOFF1 sizes.
        let header_size: usize = 44;
        let dir_entry_size: usize = 20;
        let num_tables: usize = 1;

        // Table checksum.
        let orig_checksum = table_checksum(table_data);

        // 4-byte aligned table size.
        let orig_length = table_data.len() as u32;
        let padded_len = (table_data.len() + 3) & !3;

        // Table data offset inside the WOFF file.
        let table_offset = (header_size + num_tables * dir_entry_size) as u32;

        // Total WOFF file size.
        let total_len = header_size + num_tables * dir_entry_size + padded_len;

        // totalSfntSize: SFNT offset table (12) + 1 dir entry (16) + padded table.
        let sfnt_dir_entry_size: usize = 16;
        let total_sfnt_size = (12 + sfnt_dir_entry_size + padded_len) as u32;

        let mut out = vec![0u8; total_len];

        // WOFF1 header (44 bytes).
        let sig: u32 = 0x774F_4646; // "wOFF"
        out[0..4].copy_from_slice(&sig.to_be_bytes());
        out[4..8].copy_from_slice(&(0x0001_0000u32).to_be_bytes()); // flavor: TrueType
        out[8..12].copy_from_slice(&(total_len as u32).to_be_bytes()); // length
        out[12..14].copy_from_slice(&(num_tables as u16).to_be_bytes()); // numTables
        out[14..16].copy_from_slice(&0u16.to_be_bytes()); // reserved = 0
        out[16..20].copy_from_slice(&total_sfnt_size.to_be_bytes()); // totalSfntSize
                                                                     // majorVersion, minorVersion, metaOffset, metaLength, metaOrigLength,
                                                                     // privOffset, privLength — all zero (bytes 20–43).

        // Table directory entry (20 bytes at offset 44).
        let dir_base = header_size;
        out[dir_base..dir_base + 4].copy_from_slice(tag); // tag
        out[dir_base + 4..dir_base + 8].copy_from_slice(&table_offset.to_be_bytes()); // offset
                                                                                      // comp_length == orig_length → uncompressed storage.
        out[dir_base + 8..dir_base + 12].copy_from_slice(&orig_length.to_be_bytes()); // compLength
        out[dir_base + 12..dir_base + 16].copy_from_slice(&orig_length.to_be_bytes()); // origLength
        out[dir_base + 16..dir_base + 20].copy_from_slice(&orig_checksum.to_be_bytes()); // origChecksum

        // Table data (padded to 4 bytes).
        let data_base = header_size + num_tables * dir_entry_size;
        out[data_base..data_base + table_data.len()].copy_from_slice(table_data);
        // Remaining bytes (padding) are already zero.

        out
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// The encoder must produce a valid WOFF1 file for a zero-table SFNT, and
    /// the decoder must reconstruct the same SFNT byte-for-byte.
    #[test]
    fn zero_table_sfnt_round_trip() {
        let sfnt: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&[0x00u8, 0x01, 0x00, 0x00]); // sfVersion
            v.extend_from_slice(&[0x00u8, 0x00]); // numTables = 0
            v.extend_from_slice(&[0x00u8, 0x00]); // searchRange
            v.extend_from_slice(&[0x00u8, 0x00]); // entrySelector
            v.extend_from_slice(&[0x00u8, 0x00]); // rangeShift
            v
        };

        let enc = encode_woff1(&sfnt).expect("encode_woff1 must succeed on zero-table SFNT");
        assert!(enc.starts_with(b"wOFF"), "WOFF1 magic must be present");

        let dec = decode_woff1(&enc).expect("decode_woff1 must succeed on zero-table WOFF1");
        assert_eq!(
            dec, sfnt,
            "zero-table SFNT must survive WOFF1 round-trip byte-identically"
        );
    }

    /// A table with incompressible data should be stored raw by the encoder
    /// (comp_length == orig_length).  The round-trip must be lossless.
    #[test]
    fn incompressible_table_stored_raw_round_trip() {
        let raw_data = incompressible_bytes(512);

        // 'maxp' is a valid known tag — use it so the encoder doesn't reject
        // it (SFNT tables are not validated for content by the encoder).
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", raw_data.clone())])
            .expect("build_sfnt must succeed");

        let encoded = encode_woff1(&sfnt).expect("encode_woff1 must not fail");
        assert!(encoded.starts_with(b"wOFF"), "WOFF1 magic must be present");

        let decoded = decode_woff1(&encoded).expect("decode_woff1 must succeed");

        // The decoded SFNT must contain the same tables.
        assert!(
            decoded.len() >= 12,
            "decoded SFNT must have at least offset table"
        );
        let num_tables_enc = u16::from_be_bytes([sfnt[4], sfnt[5]]);
        let num_tables_dec = u16::from_be_bytes([decoded[4], decoded[5]]);
        assert_eq!(
            num_tables_enc, num_tables_dec,
            "numTables must survive WOFF1 round-trip"
        );

        // Locate the maxp table in the decoded SFNT and verify its data.
        let n = num_tables_dec as usize;
        let mut maxp_data: Option<&[u8]> = None;
        for i in 0..n {
            let base = 12 + i * 16;
            let tag = &decoded[base..base + 4];
            if tag == b"maxp" {
                let offset = u32::from_be_bytes([
                    decoded[base + 8],
                    decoded[base + 9],
                    decoded[base + 10],
                    decoded[base + 11],
                ]) as usize;
                let length = u32::from_be_bytes([
                    decoded[base + 12],
                    decoded[base + 13],
                    decoded[base + 14],
                    decoded[base + 15],
                ]) as usize;
                maxp_data = Some(&decoded[offset..offset + length]);
                break;
            }
        }

        let maxp_data = maxp_data.expect("maxp table must be present in decoded SFNT");
        assert_eq!(
            maxp_data,
            raw_data.as_slice(),
            "maxp table data must survive WOFF1 round-trip exactly"
        );
    }

    /// A WOFF1 file where comp_length == orig_length (uncompressed storage, not
    /// produced by the encoder but valid per spec) must decode without error.
    ///
    /// We construct the WOFF1 binary by hand to force this case, rather than
    /// relying on the encoder to choose this path.
    #[test]
    fn manual_woff1_with_uncompressed_table_decodes_successfully() {
        let table_data = b"Hello, world from uncompressed WOFF1 table.    ".to_vec();
        // Use 'maxp' tag (known, non-head table so checksum is verified).
        let woff1 = build_woff1_with_uncompressed_table(b"maxp", &table_data);

        let decoded = decode_woff1(&woff1)
            .expect("decode_woff1 must accept WOFF1 with comp_length == orig_length");

        assert!(
            decoded.len() >= 12,
            "decoded SFNT must have at least offset table"
        );
        let num_tables = u16::from_be_bytes([decoded[4], decoded[5]]);
        assert_eq!(num_tables, 1, "decoded SFNT must contain exactly 1 table");
    }

    /// When comp_length == orig_length, the decoder must copy the bytes directly
    /// without attempting to zlib-decompress them.  Verify by checking that the
    /// table data in the decoded SFNT matches what we wrote.
    #[test]
    fn uncompressed_table_data_survives_decode_verbatim() {
        // Use data with known content (not random) for exact comparison.
        let table_data: Vec<u8> = (0u8..=47).collect(); // [0, 1, 2, ..., 47] — 48 bytes

        let woff1 = build_woff1_with_uncompressed_table(b"maxp", &table_data);

        let decoded =
            decode_woff1(&woff1).expect("decode_woff1 must succeed for uncompressed table");

        // Find the maxp table offset and length in the decoded SFNT directory.
        let num_tables = u16::from_be_bytes([decoded[4], decoded[5]]) as usize;
        let mut found_data: Option<Vec<u8>> = None;

        for i in 0..num_tables {
            let base = 12 + i * 16;
            let tag = &decoded[base..base + 4];
            if tag == b"maxp" {
                let off = u32::from_be_bytes([
                    decoded[base + 8],
                    decoded[base + 9],
                    decoded[base + 10],
                    decoded[base + 11],
                ]) as usize;
                let len = u32::from_be_bytes([
                    decoded[base + 12],
                    decoded[base + 13],
                    decoded[base + 14],
                    decoded[base + 15],
                ]) as usize;
                found_data = Some(decoded[off..off + len].to_vec());
                break;
            }
        }

        let found_data = found_data.expect("maxp table must be present in decoded SFNT");
        assert_eq!(
            found_data, table_data,
            "uncompressed table data must be preserved verbatim through decode"
        );
    }

    /// Multiple tables where some compress and some do not must all round-trip
    /// correctly through encode + decode.
    #[test]
    fn mixed_compressed_and_uncompressed_tables_round_trip() {
        // Build an SFNT with two tables:
        //   'name' — repetitive data that compresses well (0x41 × 256)
        //   'post' — pseudo-random data that compresses poorly
        let compressible = vec![0x41u8; 256];
        let incompressible = incompressible_bytes(256);

        let sfnt = build_sfnt(
            SFNT_MAGIC_TT,
            &[
                (*b"name", compressible.clone()),
                (*b"post", incompressible.clone()),
            ],
        )
        .expect("build_sfnt must succeed");

        let encoded = encode_woff1(&sfnt).expect("encode_woff1 must succeed");
        assert!(encoded.starts_with(b"wOFF"), "WOFF1 magic must be present");

        let decoded = decode_woff1(&encoded).expect("decode_woff1 must succeed");
        assert!(decoded.len() >= 12, "decoded SFNT must have offset table");

        let n = u16::from_be_bytes([decoded[4], decoded[5]]) as usize;
        assert_eq!(n, 2, "decoded SFNT must have 2 tables");

        // Verify both tables are present and intact.
        for i in 0..n {
            let base = 12 + i * 16;
            let tag = &decoded[base..base + 4];
            let off = u32::from_be_bytes([
                decoded[base + 8],
                decoded[base + 9],
                decoded[base + 10],
                decoded[base + 11],
            ]) as usize;
            let len = u32::from_be_bytes([
                decoded[base + 12],
                decoded[base + 13],
                decoded[base + 14],
                decoded[base + 15],
            ]) as usize;
            let data = &decoded[off..off + len];

            if tag == b"name" {
                assert_eq!(data, compressible.as_slice(), "name table must match");
            } else if tag == b"post" {
                assert_eq!(data, incompressible.as_slice(), "post table must match");
            }
        }
    }

    /// The WOFF1 encoder must produce a file where each compressed table's
    /// comp_length is <= its orig_length (no inflated storage).
    #[test]
    fn encoder_never_inflates_tables() {
        let sfnt = build_sfnt(
            SFNT_MAGIC_TT,
            &[
                (*b"name", vec![0xBBu8; 128]),
                (*b"maxp", incompressible_bytes(128)),
            ],
        )
        .expect("build_sfnt must succeed");

        let woff1 = encode_woff1(&sfnt).expect("encode_woff1 must succeed");

        // Parse the WOFF1 directory (starts at offset 44, each entry is 20 bytes).
        let num_tables = u16::from_be_bytes([woff1[12], woff1[13]]) as usize;
        let dir_start = 44usize;

        for i in 0..num_tables {
            let base = dir_start + i * 20;
            let comp_length = u32::from_be_bytes([
                woff1[base + 8],
                woff1[base + 9],
                woff1[base + 10],
                woff1[base + 11],
            ]);
            let orig_length = u32::from_be_bytes([
                woff1[base + 12],
                woff1[base + 13],
                woff1[base + 14],
                woff1[base + 15],
            ]);
            let tag = &woff1[base..base + 4];
            assert!(
                comp_length <= orig_length,
                "table '{}' comp_length ({comp_length}) must not exceed orig_length ({orig_length})",
                std::str::from_utf8(tag).unwrap_or("????")
            );
        }
    }
}
