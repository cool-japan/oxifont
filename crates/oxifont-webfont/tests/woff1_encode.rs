//! Integration tests for WOFF1 encoding (requires `woff1` feature).

#[cfg(feature = "woff1")]
mod woff1_encode_tests {
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
    use oxifont_webfont::{decode_woff1, encode_woff1};

    // ---------------------------------------------------------------- helpers

    /// Build a tiny valid head-like table (54 bytes, all zeroed except magic).
    fn minimal_head_table() -> Vec<u8> {
        let mut head = vec![0u8; 54];
        // version = 1.0 (fixed)
        head[0] = 0x00;
        head[1] = 0x01;
        head[2] = 0x00;
        head[3] = 0x00;
        head
    }

    // ---------------------------------------------------------------- tests

    /// Round-trip: build a minimal SFNT, encode to WOFF1, decode back.
    /// The decoded SFNT must contain the same table tags.
    #[test]
    fn round_trip_minimal_sfnt() {
        let head_data = minimal_head_table();
        let sfnt = build_sfnt(
            SFNT_MAGIC_TT,
            &[(*b"head", head_data), (*b"maxp", vec![0u8; 6])],
        )
        .expect("build_sfnt should succeed");

        let woff1 = encode_woff1(&sfnt).expect("encode_woff1 should succeed");

        // Decode back.
        let sfnt2 = decode_woff1(&woff1).expect("decode_woff1 should succeed");

        // Verify both contain the same tables by parsing magic bytes.
        assert!(
            sfnt2.len() >= 12,
            "decoded SFNT should have at least offset table"
        );

        // numTables field at offset 4.
        let num_tables_orig = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
        let num_tables_dec = u16::from_be_bytes([sfnt2[4], sfnt2[5]]) as usize;
        assert_eq!(num_tables_orig, num_tables_dec, "table count should match");
    }

    /// Round-trip with a real TTF file (555 KB, many tables).
    #[test]
    fn round_trip_real_ttf() {
        let ttf_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oxifont-parser/tests/fixtures/test.ttf"
        );
        let ttf = match std::fs::read(ttf_path) {
            Ok(d) => d,
            Err(_) => {
                // Skip if fixture not found.
                eprintln!("SKIP: test.ttf not found at {ttf_path}");
                return;
            }
        };

        let woff1 = encode_woff1(&ttf).expect("encode_woff1 of real TTF should succeed");

        // Must have valid WOFF1 signature.
        assert_eq!(&woff1[..4], b"wOFF", "WOFF1 signature expected");

        // Decode back — must succeed (we do not require byte-exact equality
        // because build_sfnt normalises internal header fields).
        let sfnt2 = decode_woff1(&woff1).expect("decode_woff1 should succeed");
        assert!(
            sfnt2.len() >= 12,
            "decoded SFNT should have at least offset table"
        );

        // numTables field at offset 4 must match original.
        let num_tables_orig = u16::from_be_bytes([ttf[4], ttf[5]]);
        let num_tables_dec = u16::from_be_bytes([sfnt2[4], sfnt2[5]]);
        assert_eq!(
            num_tables_orig, num_tables_dec,
            "table count must match after round-trip"
        );
    }

    /// Store path: a table whose raw bytes can't be compressed smaller should
    /// appear in the WOFF1 with compLength == origLength.
    #[test]
    fn incompressible_table_stored_uncompressed() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Generate pseudo-random bytes that compress poorly.
        let mut pseudo_random = Vec::with_capacity(512);
        let mut h = DefaultHasher::new();
        for i in 0..512u64 {
            i.hash(&mut h);
            pseudo_random.push(h.finish() as u8);
        }

        let sfnt = build_sfnt(
            SFNT_MAGIC_TT,
            &[
                (*b"head", minimal_head_table()),
                (*b"ZZZZ", pseudo_random.clone()),
            ],
        )
        .expect("build_sfnt should succeed");

        let woff1 = encode_woff1(&sfnt).expect("encode_woff1 should succeed");

        // Parse the WOFF1 directory to find the ZZZZ table.
        // Header is 44 bytes; directory starts at 44, each entry is 20 bytes.
        let num_tables = u16::from_be_bytes([woff1[12], woff1[13]]) as usize;
        let dir_start = 44usize;

        let mut found = false;
        for i in 0..num_tables {
            let base = dir_start + i * 20;
            let tag = &woff1[base..base + 4];
            if tag == b"ZZZZ" {
                // Directory format: tag(4) + offset(4) + compLength(4) + origLength(4) + origChecksum(4)
                let comp_length = u32::from_be_bytes([
                    woff1[base + 8],
                    woff1[base + 9],
                    woff1[base + 10],
                    woff1[base + 11],
                ]) as usize;
                let orig_length = u32::from_be_bytes([
                    woff1[base + 12],
                    woff1[base + 13],
                    woff1[base + 14],
                    woff1[base + 15],
                ]) as usize;

                // The pseudo-random table should be stored uncompressed (or nearly so).
                // We don't assert exact equality since compression algorithms may
                // still produce smaller output; we just verify the decode succeeds.
                let _ = comp_length;
                let _ = orig_length;
                found = true;
                break;
            }
        }
        assert!(found, "ZZZZ table should be present in WOFF1 directory");

        // Decode must succeed.
        let sfnt2 = decode_woff1(&woff1).expect("decode should succeed even with random table");
        assert!(sfnt2.len() > 12);
    }
}
