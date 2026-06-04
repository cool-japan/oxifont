// WOFF2 font collection (TTC-in-WOFF2) decoding tests.
//
// Tests cover:
//   - Error-handling paths (wrong signature, wrong flavor, short input).
//   - A synthetic TTC-in-WOFF2 with two fonts sharing a `post` table,
//     exercising the real `parse_collection_header` + `extract_tables_by_index`
//     + `select_font_tables_indexed` decode path.

// ---------------------------------------------------------- error-path tests

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

// -------------------------------------------------------- synthetic TTC test

/// Build a synthetic TTC-in-WOFF2 with two fonts sharing a `post` table.
///
/// Collection structure:
///   - Shared table directory: 3 entries → [0]=head-A, [1]=head-B, [2]=post-shared
///   - Font 0: TrueType, tables = [0=head-A, 2=post-shared]
///   - Font 1: TrueType, tables = [1=head-B, 2=post-shared]
///
/// The decompressed brotli stream layout (for WOFF2 collection):
///   CollectionHeader | table_payload (head-A bytes | head-B bytes | post bytes)
///
/// All tables use null transform (transform_version=3 for non-glyf/loca tables
/// in the collection encoder, or 0 for "identity" per spec wording).
#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_synthetic_two_fonts() {
    use oxiarc_brotli::compress;

    // ---- Build minimal table payloads -----------------------------------

    // head table (minimal, 54 bytes): enough for a parseable SFNT in ttf-parser.
    // We build a fake head-A and head-B that differ in their magic (byte 12-15).
    fn make_head(magic_extra: u8) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        // head.version = 0x00010000
        head[0] = 0x00;
        head[1] = 0x01;
        head[2] = 0x00;
        head[3] = 0x00;
        // fontRevision (4 bytes) = 0
        // checkSumAdjustment (4 bytes) = 0 (will be fixed by SFNT assembler)
        // magicNumber = 0x5F0F3CF5
        head[12] = 0x5F;
        head[13] = 0x0F;
        head[14] = magic_extra;
        head[15] = 0xF5;
        // flags (2 bytes) = 0
        // unitsPerEm (2 bytes) = 1000
        head[18] = 0x03;
        head[19] = 0xE8;
        // indexToLocFormat (2 bytes at offset 50) = 0
        head
    }

    // post table v3 (minimal, 32 bytes): version=0x00030000, all zeros.
    let post_shared = {
        let mut p = vec![0u8; 32];
        p[0] = 0x00;
        p[1] = 0x03;
        p
    };

    let head_a = make_head(0x3C);
    let head_b = make_head(0x4D);

    // ---- Build the decompressed collection stream -----------------------
    // Layout: CollectionHeader | head_a | head_b | post_shared

    fn encode_255_u16_simple(v: u16) -> Vec<u8> {
        if v <= 252 {
            vec![v as u8]
        } else if v <= 508 {
            vec![255u8, (v - 253) as u8]
        } else {
            let [hi, lo] = v.to_be_bytes();
            vec![253u8, hi, lo]
        }
    }

    let mut collection_header: Vec<u8> = Vec::new();
    // CollectionHeader.version = 0x00010000
    collection_header.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    // numFonts = 2 (255UInt16)
    collection_header.extend_from_slice(&encode_255_u16_simple(2));

    // Font 0: numTables=2, flavor=0x00010000 (TrueType), indices=[0, 2]
    collection_header.extend_from_slice(&encode_255_u16_simple(2)); // numTables
    collection_header.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // flavor
    collection_header.extend_from_slice(&encode_255_u16_simple(0)); // index 0 = head-A
    collection_header.extend_from_slice(&encode_255_u16_simple(2)); // index 2 = post-shared

    // Font 1: numTables=2, flavor=0x00010000 (TrueType), indices=[1, 2]
    collection_header.extend_from_slice(&encode_255_u16_simple(2)); // numTables
    collection_header.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // flavor
    collection_header.extend_from_slice(&encode_255_u16_simple(1)); // index 1 = head-B
    collection_header.extend_from_slice(&encode_255_u16_simple(2)); // index 2 = post-shared

    // Table payload: head-A | head-B | post_shared
    let mut table_payload: Vec<u8> = Vec::new();
    table_payload.extend_from_slice(&head_a);
    table_payload.extend_from_slice(&head_b);
    table_payload.extend_from_slice(&post_shared);

    // Full decompressed data = CollectionHeader + table_payload
    let mut decompressed: Vec<u8> = collection_header;
    decompressed.extend_from_slice(&table_payload);

    // Brotli-compress the collection stream.
    let compressed = compress(&decompressed, 11).expect("brotli compress must succeed");

    // ---- Build the WOFF2 table directory --------------------------------
    // We have 3 shared tables: head (×2) and post.
    // Format: flags_byte (1B) + UIntBase128(origLength) + optional UIntBase128(transformLength)
    //
    // For null-transformed tables (no glyf/loca/hmtx here), transform_version=0.
    // The `head` tag is at KNOWN_TAGS index 1; `post` is at index 7.
    // flags_byte = (transform_version << 6) | tag_index
    //
    // head: known index=1, transform_version=0 → flags_byte=0x01
    // post: known index=7, transform_version=0 → flags_byte=0x07
    // No transformLength field for null-transformed tables.

    fn encode_uint_base128_simple(v: u32) -> Vec<u8> {
        let mut buf = [0u8; 5];
        let mut len = 0usize;
        let mut val = v;
        loop {
            buf[len] = (val & 0x7F) as u8;
            len += 1;
            val >>= 7;
            if val == 0 {
                break;
            }
        }
        let mut out = Vec::with_capacity(len);
        for i in (0..len).rev() {
            let byte = buf[i];
            if i > 0 {
                out.push(byte | 0x80);
            } else {
                out.push(byte);
            }
        }
        out
    }

    let mut table_dir: Vec<u8> = Vec::new();
    // Entry 0: head-A, null transform.
    table_dir.push(0x01u8); // flags: known tag index 1 = "head", transform=0
    table_dir.extend_from_slice(&encode_uint_base128_simple(head_a.len() as u32));
    // Entry 1: head-B, null transform.
    table_dir.push(0x01u8);
    table_dir.extend_from_slice(&encode_uint_base128_simple(head_b.len() as u32));
    // Entry 2: post, null transform.
    table_dir.push(0x07u8); // flags: known tag index 7 = "post", transform=0
    table_dir.extend_from_slice(&encode_uint_base128_simple(post_shared.len() as u32));

    // ---- Compute WOFF2 sizes ----------------------------------------
    const WOFF2_HEADER_SIZE: usize = 48;
    let total_compressed_size = compressed.len() as u32;
    // totalSfntSize: nominal TTC size.  We set it to 0 for simplicity
    // (the decoder doesn't enforce this for collections).
    let total_sfnt_size = 0u32;
    let total_length = (WOFF2_HEADER_SIZE + table_dir.len() + compressed.len()) as u32;
    let num_tables = 3u16;
    let sf_version: u32 = 0x7474_6366; // "ttcf"

    // ---- Assemble WOFF2 file ----------------------------------------
    let mut woff2: Vec<u8> = Vec::with_capacity(total_length as usize);

    // Header (48 bytes)
    woff2.extend_from_slice(&0x774F_4632u32.to_be_bytes()); // signature "wOF2"
    woff2.extend_from_slice(&sf_version.to_be_bytes()); // flavor "ttcf"
    woff2.extend_from_slice(&total_length.to_be_bytes()); // length
    woff2.extend_from_slice(&num_tables.to_be_bytes()); // numTables
    woff2.extend_from_slice(&0u16.to_be_bytes()); // reserved
    woff2.extend_from_slice(&total_sfnt_size.to_be_bytes()); // totalSfntSize
    woff2.extend_from_slice(&total_compressed_size.to_be_bytes()); // totalCompressedSize
    woff2.extend_from_slice(&0u16.to_be_bytes()); // majorVersion
    woff2.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    woff2.extend_from_slice(&0u32.to_be_bytes()); // metaOffset
    woff2.extend_from_slice(&0u32.to_be_bytes()); // metaLength
    woff2.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength
    woff2.extend_from_slice(&0u32.to_be_bytes()); // privOffset
    woff2.extend_from_slice(&0u32.to_be_bytes()); // privLength
    assert_eq!(woff2.len(), WOFF2_HEADER_SIZE);

    // Table directory
    woff2.extend_from_slice(&table_dir);

    // Compressed data block
    woff2.extend_from_slice(&compressed);

    // ---- Decode and verify ------------------------------------------
    let result = oxifont_webfont::decode_woff2_collection(&woff2);
    assert!(
        result.is_ok(),
        "synthetic TTC-in-WOFF2 decode must succeed; error: {:?}",
        result.err()
    );

    let sfnts = result.expect("decode_collection must succeed");
    assert_eq!(sfnts.len(), 2, "collection must yield 2 fonts");

    // Each SFNT must start with a valid TrueType magic number.
    for (i, sfnt) in sfnts.iter().enumerate() {
        assert!(
            sfnt.len() >= 12,
            "decoded SFNT[{i}] must be at least 12 bytes"
        );
        let magic = u32::from_be_bytes([sfnt[0], sfnt[1], sfnt[2], sfnt[3]]);
        assert_eq!(
            magic, 0x0001_0000,
            "decoded SFNT[{i}] must have TrueType magic"
        );
        // Verify numTables = 2 (head + post).
        let num_t = u16::from_be_bytes([sfnt[4], sfnt[5]]);
        assert_eq!(
            num_t, 2,
            "decoded SFNT[{i}] must have 2 tables (head + post)"
        );
    }

    // Verify that font 0 has head-A data and font 1 has head-B data
    // by locating the `head` table in each SFNT and checking magic_extra.
    fn find_table_in_sfnt(sfnt: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
        if sfnt.len() < 12 {
            return None;
        }
        let n = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
        for i in 0..n {
            let base = 12 + i * 16;
            if sfnt.get(base..base + 4)? == tag.as_ref() {
                let offset =
                    u32::from_be_bytes(sfnt[base + 8..base + 12].try_into().ok()?) as usize;
                let length =
                    u32::from_be_bytes(sfnt[base + 12..base + 16].try_into().ok()?) as usize;
                return Some(sfnt.get(offset..offset + length)?.to_vec());
            }
        }
        None
    }

    let head_a_decoded = find_table_in_sfnt(&sfnts[0], b"head");
    let head_b_decoded = find_table_in_sfnt(&sfnts[1], b"head");

    if let (Some(ha), Some(hb)) = (&head_a_decoded, &head_b_decoded) {
        // Byte 14 (magic_extra) should differ between font 0 and font 1.
        assert_ne!(
            ha.get(14),
            hb.get(14),
            "font 0 and font 1 must have distinct head.magicNumber[14]"
        );
        assert_eq!(ha.get(14), Some(&0x3Cu8), "font 0 must have head-A magic");
        assert_eq!(hb.get(14), Some(&0x4Du8), "font 1 must have head-B magic");
    }

    // Verify that the shared `post` table is identical in both fonts.
    let post_a = find_table_in_sfnt(&sfnts[0], b"post");
    let post_b = find_table_in_sfnt(&sfnts[1], b"post");
    if let (Some(pa), Some(pb)) = (&post_a, &post_b) {
        assert_eq!(pa, pb, "shared post table must be identical in both fonts");
        assert_eq!(
            pa.len(),
            post_shared.len(),
            "post table length must match original"
        );
    }
}

/// Verify that decode_woff2_collection is consistent with the flavour check:
/// a valid WOFF2 with sf_version = ttcf must succeed; one without must fail.
#[cfg(feature = "woff2")]
#[test]
fn test_decode_woff2_collection_flavor_is_checked() {
    // A well-formed single-font WOFF2 (flavor != ttcf) must fail decode_collection.
    let minimal_sfnt = vec![
        0x00u8, 0x01, 0x00, 0x00, // TrueType sfnt_version
        0x00, 0x00, // numTables=0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // padding
    ];
    if let Ok(woff2) = oxifont_webfont::encode_woff2(&minimal_sfnt) {
        let result = oxifont_webfont::decode_woff2_collection(&woff2);
        assert!(
            result.is_err(),
            "WOFF2 with TT flavor must be rejected by decode_woff2_collection"
        );
        // Regular decode must succeed.
        assert!(
            oxifont_webfont::decode_woff2(&woff2).is_ok(),
            "decode_woff2 must still succeed on the same data"
        );
    }
}
