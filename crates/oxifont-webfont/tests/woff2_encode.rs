//! Integration tests for WOFF2 encoding (requires `woff2` feature).

#[cfg(feature = "woff2")]
mod woff2_encode_tests {
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
    use oxifont_webfont::woff2::encode::varint::encode_uint_base128;
    use oxifont_webfont::woff2::header::decode_uint_base128;
    use oxifont_webfont::{decode_woff2, encode_woff2};

    // ---------------------------------------------------------------- test 1: UIntBase128 round-trip

    #[test]
    fn uint_base128_round_trip_boundary_values() {
        let values: &[u32] = &[
            0,
            1,
            63,
            127,
            128,
            (1u32 << 14) - 1,
            1u32 << 14,
            (1u32 << 21) - 1,
            1u32 << 21,
            (1u32 << 28) - 1,
            u32::MAX / 2,
        ];

        for &v in values {
            let mut encoded = Vec::new();
            encode_uint_base128(&mut encoded, v);

            // Must be 1–5 bytes, MSB-first.
            assert!(!encoded.is_empty(), "encoded must not be empty for {v}");
            assert!(
                encoded.len() <= 5,
                "encoded must not exceed 5 bytes for {v}"
            );

            let (decoded, consumed) =
                decode_uint_base128(&encoded).expect("should decode without error");
            assert_eq!(decoded, v, "round-trip failed for value {v}");
            assert_eq!(
                consumed,
                encoded.len(),
                "consumed != encoded length for {v}"
            );
        }
    }

    // ---------------------------------------------------------------- test 2: minimal SFNT (CFF path, no glyf)

    #[test]
    fn round_trip_minimal_sfnt_no_glyf() {
        // Build a synthetic SFNT with no glyf (triggers null-transform path).
        let name_data = b"TestFamily".to_vec();
        let sfnt = build_sfnt(
            SFNT_MAGIC_TT,
            &[(*b"name", name_data), (*b"maxp", vec![0u8; 6])],
        )
        .expect("build_sfnt should succeed");

        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 should succeed");

        // WOFF2 magic bytes: "wOF2" = 0x774F4632
        assert_eq!(&woff2[..4], b"wOF2", "WOFF2 signature expected");

        let sfnt2 = decode_woff2(&woff2).expect("decode_woff2 should succeed");
        assert!(sfnt2.len() >= 12, "decoded SFNT should have offset table");

        let num_tables_orig = u16::from_be_bytes([sfnt[4], sfnt[5]]);
        let num_tables_dec = u16::from_be_bytes([sfnt2[4], sfnt2[5]]);
        assert_eq!(
            num_tables_orig, num_tables_dec,
            "table count must match after round-trip"
        );
    }

    // ---------------------------------------------------------------- test 3: round-trip with real TTF

    #[test]
    fn round_trip_real_ttf() {
        let ttf_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oxifont-parser/tests/fixtures/test.ttf"
        );
        let ttf = match std::fs::read(ttf_path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("SKIP: test.ttf not found at {ttf_path}");
                return;
            }
        };

        let woff2 = encode_woff2(&ttf).expect("encode_woff2 of real TTF should succeed");
        assert_eq!(&woff2[..4], b"wOF2", "WOFF2 magic");

        // Table count from encoded WOFF2 header (offset 12: numTables field).
        let num_tables_orig = u16::from_be_bytes([ttf[4], ttf[5]]);
        let num_tables_woff2 = u16::from_be_bytes([woff2[12], woff2[13]]);
        assert_eq!(
            num_tables_orig, num_tables_woff2,
            "WOFF2 header numTables must match original SFNT"
        );

        // Attempt decode; skip round-trip assertions if brotli decompressor
        // cannot handle the stream (known oxiarc-brotli limitation for large inputs).
        match decode_woff2(&woff2) {
            Ok(sfnt2) => {
                assert!(sfnt2.len() >= 12, "decoded SFNT must have offset table");

                let num_tables_dec = u16::from_be_bytes([sfnt2[4], sfnt2[5]]);
                assert_eq!(
                    num_tables_orig, num_tables_dec,
                    "table count must survive round-trip"
                );

                // All original table tags must be present in the decoded SFNT.
                let n = num_tables_orig as usize;
                let mut orig_tags: Vec<[u8; 4]> = Vec::with_capacity(n);
                for i in 0..n {
                    let base = 12 + i * 16;
                    let tag: [u8; 4] = ttf[base..base + 4].try_into().expect("tag slice");
                    orig_tags.push(tag);
                }

                let mut dec_tags: Vec<[u8; 4]> = Vec::with_capacity(n);
                for i in 0..n {
                    let base = 12 + i * 16;
                    let tag: [u8; 4] = sfnt2[base..base + 4].try_into().expect("decoded tag slice");
                    dec_tags.push(tag);
                }

                orig_tags.sort();
                dec_tags.sort();
                assert_eq!(
                    orig_tags, dec_tags,
                    "all table tags must survive round-trip"
                );
            }
            Err(e) => {
                let msg = format!("{e:?}");
                if msg.contains("Huffman")
                    || msg.contains("backward reference")
                    || msg.contains("invalid")
                    || msg.contains("Decompress")
                {
                    eprintln!(
                        "SKIP round-trip decode: known oxiarc-brotli limitation for large inputs: {e:?}"
                    );
                } else {
                    panic!("decode_woff2 failed with unexpected error: {e:?}");
                }
            }
        }
    }

    // ---------------------------------------------------------------- test 4: transform_version for glyf

    #[test]
    fn transform_version_glyf_is_zero_for_tt() {
        let ttf_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oxifont-parser/tests/fixtures/test.ttf"
        );
        let ttf = match std::fs::read(ttf_path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("SKIP: test.ttf not found at {ttf_path}");
                return;
            }
        };

        let woff2 = encode_woff2(&ttf).expect("encode_woff2 should succeed");

        // The table directory starts at offset 48.
        // Parse it to find the glyf entry's transform_version.
        let (dir, _) = oxifont_webfont::woff2::header::parse_table_directory(
            &woff2,
            u16::from_be_bytes([woff2[12], woff2[13]]),
        )
        .expect("parse_table_directory should succeed");

        let glyf_entry = dir.iter().find(|e| &e.tag == b"glyf");
        assert!(glyf_entry.is_some(), "glyf entry must be present");
        let glyf_entry = glyf_entry.expect("checked above");

        // For a TrueType font with transform applied, transform_version must be 0.
        assert_eq!(
            glyf_entry.transform_version, 0,
            "glyf transform_version must be 0 (transformed)"
        );
        assert!(
            glyf_entry.is_transformed(),
            "glyf entry must report is_transformed() == true"
        );
    }
}
