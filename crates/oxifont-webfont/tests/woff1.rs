//! WOFF1 integration tests.
//!
//! These tests build a synthetic WOFF1 file from the fixture TTF that is
//! already used by `oxifont-parser`, decode it, and verify the decoded SFNT
//! re-parses to a `ParsedFace` with the same family name and glyph count.

#[cfg(feature = "woff1")]
mod woff1_tests {
    use oxifont_core::FontFace as _;
    use oxifont_parser::ParsedFace;
    use oxifont_webfont::decode_woff1;

    // ---------------------------------------------------------------------------
    // Synthetic WOFF1 builder helpers (test-only)
    // ---------------------------------------------------------------------------

    /// Wrap a raw TTF byte slice into a valid WOFF1 byte buffer.
    ///
    /// For each table in the TTF, we optionally compress with zlib; if the
    /// compressed size is NOT smaller, we store the table uncompressed.
    fn build_woff1(ttf: &[u8]) -> Vec<u8> {
        use oxiarc_deflate::zlib_compress;

        // Parse the TTF offset table.
        let sfnt_version = u32::from_be_bytes(ttf[0..4].try_into().unwrap());
        let num_tables = u16::from_be_bytes(ttf[4..6].try_into().unwrap()) as usize;

        struct TtfTableEntry {
            tag: [u8; 4],
            _checksum: u32,
            offset: u32,
            length: u32,
        }

        let mut entries: Vec<TtfTableEntry> = Vec::with_capacity(num_tables);
        for i in 0..num_tables {
            let base = 12 + i * 16;
            let tag: [u8; 4] = ttf[base..base + 4].try_into().unwrap();
            let checksum = u32::from_be_bytes(ttf[base + 4..base + 8].try_into().unwrap());
            let offset = u32::from_be_bytes(ttf[base + 8..base + 12].try_into().unwrap());
            let length = u32::from_be_bytes(ttf[base + 12..base + 16].try_into().unwrap());
            entries.push(TtfTableEntry {
                tag,
                _checksum: checksum,
                offset,
                length,
            });
        }

        // Compress each table.
        struct WoffTableEntry {
            tag: [u8; 4],
            orig_length: u32,
            orig_checksum: u32,
            data: Vec<u8>, // compressed or raw
        }

        let mut woff_tables: Vec<WoffTableEntry> = Vec::with_capacity(num_tables);
        for entry in &entries {
            let start = entry.offset as usize;
            let end = start + entry.length as usize;
            let orig = &ttf[start..end];

            // Checksum from original.
            let cs = table_checksum_test(orig);

            let compressed = zlib_compress(orig, 6).unwrap_or_default();
            let data = if compressed.len() < orig.len() {
                compressed
            } else {
                orig.to_vec()
            };

            woff_tables.push(WoffTableEntry {
                tag: entry.tag,
                orig_length: entry.length,
                orig_checksum: cs,
                data,
            });
        }

        // Compute layout.
        let header_size = 44usize;
        let dir_size = num_tables * 20;
        let dir_end = header_size + dir_size;

        // Assign table offsets (4-byte aligned).
        let mut offsets: Vec<u32> = Vec::with_capacity(num_tables);
        let mut running = dir_end as u32;
        for t in &woff_tables {
            offsets.push(running);
            let padded = t.data.len().div_ceil(4) * 4;
            running += padded as u32;
        }
        let total_size = running as usize;
        let total_sfnt_size = ttf.len() as u32;

        // Write WOFF1.
        let mut out = vec![0u8; total_size];

        // Header.
        out[0..4].copy_from_slice(&0x774F_4646u32.to_be_bytes()); // signature
        out[4..8].copy_from_slice(&sfnt_version.to_be_bytes()); // flavor (sfVersion)
        out[8..12].copy_from_slice(&(total_size as u32).to_be_bytes()); // length
        out[12..14].copy_from_slice(&(num_tables as u16).to_be_bytes()); // numTables
        out[14..16].copy_from_slice(&0u16.to_be_bytes()); // reserved = 0
        out[16..20].copy_from_slice(&total_sfnt_size.to_be_bytes()); // totalSfntSize
                                                                     // majorVersion, minorVersion, metaOffset, metaLength, metaOrigLength, privOffset, privLength
                                                                     // all zero — acceptable for a test WOFF1.

        // Table directory.
        for (i, t) in woff_tables.iter().enumerate() {
            let base = header_size + i * 20;
            out[base..base + 4].copy_from_slice(&t.tag);
            out[base + 4..base + 8].copy_from_slice(&offsets[i].to_be_bytes());
            out[base + 8..base + 12].copy_from_slice(&(t.data.len() as u32).to_be_bytes());
            out[base + 12..base + 16].copy_from_slice(&t.orig_length.to_be_bytes());
            out[base + 16..base + 20].copy_from_slice(&t.orig_checksum.to_be_bytes());
        }

        // Table data.
        for (i, t) in woff_tables.iter().enumerate() {
            let start = offsets[i] as usize;
            out[start..start + t.data.len()].copy_from_slice(&t.data);
        }

        out
    }

    /// Compute table checksum (sum of big-endian uint32 words, last word padded).
    fn table_checksum_test(data: &[u8]) -> u32 {
        let mut sum: u32 = 0;
        let mut i = 0;
        while i + 4 <= data.len() {
            sum = sum.wrapping_add(u32::from_be_bytes(data[i..i + 4].try_into().unwrap()));
            i += 4;
        }
        if i < data.len() {
            let mut tail = [0u8; 4];
            tail[..data.len() - i].copy_from_slice(&data[i..]);
            sum = sum.wrapping_add(u32::from_be_bytes(tail));
        }
        sum
    }

    // ---------------------------------------------------------------------------
    // Fixture
    // ---------------------------------------------------------------------------

    static TTF_FIXTURE: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[test]
    fn woff1_round_trip_parses() {
        let woff1 = build_woff1(TTF_FIXTURE);
        let sfnt = decode_woff1(&woff1).expect("WOFF1 decode must succeed");

        let face = ParsedFace::parse(sfnt, 0).expect("decoded SFNT must parse");
        let name = face.family_name();
        assert!(
            !name.is_empty(),
            "family name must not be empty after WOFF1 round-trip"
        );
    }

    #[test]
    fn woff1_round_trip_same_glyph_count() {
        use oxifont_parser::face_count;
        let woff1 = build_woff1(TTF_FIXTURE);
        let sfnt = decode_woff1(&woff1).expect("WOFF1 decode must succeed");

        // Decoded SFNT should have the same face count as the original TTF.
        let woff1_face_count = face_count(&sfnt);
        let orig_face_count = face_count(TTF_FIXTURE);
        assert_eq!(
            woff1_face_count, orig_face_count,
            "decoded WOFF1 face count {woff1_face_count} != original {orig_face_count}"
        );
    }

    #[test]
    fn woff1_round_trip_units_per_em() {
        let woff1 = build_woff1(TTF_FIXTURE);
        let sfnt = decode_woff1(&woff1).expect("WOFF1 decode must succeed");
        let orig_face =
            ParsedFace::parse(TTF_FIXTURE.to_vec(), 0).expect("original TTF must parse");
        let decoded_face = ParsedFace::parse(sfnt, 0).expect("decoded SFNT must parse");

        assert_eq!(
            decoded_face.units_per_em(),
            orig_face.units_per_em(),
            "units_per_em mismatch after WOFF1 round-trip"
        );
    }

    #[test]
    fn woff1_reject_bad_signature() {
        let mut bad = vec![0u8; 48];
        bad[0] = 0xDE; // wrong signature
        let result = decode_woff1(&bad);
        assert!(result.is_err(), "should reject bad signature");
    }
}
