//! Integration tests for `SfntTableMap` using a real TTF fixture.

use oxifont_core::sfnt::{SfntError, SfntTableMap};

/// The same fixture used by `oxifont-parser` tests.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

#[test]
fn parse_real_ttf_succeeds() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse without error");
    assert!(
        map.num_tables() > 0,
        "a real TTF must have at least one table"
    );
}

#[test]
fn parse_real_ttf_finds_glyf() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    let glyf = map.table(b"glyf").expect("real TTF must have a glyf table");
    assert!(!glyf.is_empty(), "glyf table must not be empty");
}

#[test]
fn parse_real_ttf_finds_cmap() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    assert!(
        map.table(b"cmap").is_some(),
        "real TTF must have a cmap table"
    );
}

#[test]
fn parse_real_ttf_finds_head() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    assert!(
        map.table(b"head").is_some(),
        "real TTF must have a head table"
    );
}

#[test]
fn tags_are_sorted() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    let tags: Vec<&[u8; 4]> = map.tags().collect();
    let mut sorted = tags.clone();
    sorted.sort();
    assert_eq!(
        tags, sorted,
        "tags() must yield tags in sorted (BTreeMap) order"
    );
}

#[test]
fn raw_returns_same_bytes() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    assert_eq!(
        map.raw().len(),
        TTF.len(),
        "raw() must return the full original data slice"
    );
}

#[test]
fn num_tables_matches_tag_count() {
    let map = SfntTableMap::parse(TTF).expect("real TTF must parse");
    let tag_count = map.tags().count();
    assert_eq!(
        map.num_tables(),
        tag_count,
        "num_tables() must match the number of tags yielded by tags()"
    );
}

#[test]
fn corrupt_magic_returns_bad_magic() {
    let mut data = vec![0u8; 32];
    // SFNT header: bad magic at [0..4]
    data[0..4].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    // numTables = 0 to avoid other errors
    data[4] = 0;
    data[5] = 0;
    match SfntTableMap::parse(&data) {
        Err(SfntError::BadMagic(m)) => {
            assert_eq!(m, 0xDEAD_BEEF_u32);
        }
        other => panic!("expected BadMagic, got {:?}", other),
    }
}

#[test]
fn ttcf_magic_is_rejected() {
    // "ttcf" magic (0x74746366) should be rejected — it is a TTC container,
    // not a per-face SFNT.
    let mut data = vec![0u8; 32];
    data[0..4].copy_from_slice(b"ttcf");
    data[4] = 0; // numTables = 0
    data[5] = 0;
    match SfntTableMap::parse(&data) {
        Err(SfntError::BadMagic(m)) => {
            assert_eq!(
                m, 0x7474_6366_u32,
                "ttcf magic must be reported in BadMagic"
            );
        }
        other => panic!("expected BadMagic for ttcf magic, got {:?}", other),
    }
}

#[test]
fn truncated_header_returns_truncated() {
    // 3 bytes — too short for the 12-byte SFNT header.
    let result = SfntTableMap::parse(&[0x00, 0x01, 0x00]);
    match result {
        Err(SfntError::Truncated) => {}
        other => panic!("expected Truncated, got {:?}", other),
    }
}

#[test]
fn truncated_directory_returns_truncated() {
    // Valid TrueType magic + numTables = 5 but no actual directory entries.
    let mut data = vec![0u8; 12];
    data[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    data[4..6].copy_from_slice(&5u16.to_be_bytes()); // claims 5 tables
                                                     // No directory entries follow → Truncated
    match SfntTableMap::parse(&data) {
        Err(SfntError::Truncated) => {}
        other => panic!(
            "expected Truncated for missing directory entries, got {:?}",
            other
        ),
    }
}

#[test]
fn out_of_bounds_table_returns_out_of_bounds() {
    // Build a minimal TTF header + 1 directory entry that points outside data.
    let mut data = vec![0u8; 12 + 16]; // header + 1 entry, no table data
    data[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes()); // TrueType magic
    data[4..6].copy_from_slice(&1u16.to_be_bytes()); // numTables = 1
    let tag = *b"glyf";
    data[12..16].copy_from_slice(&tag);
    // checksum = 0
    let bad_offset: u32 = 9999; // way beyond data
    data[20..24].copy_from_slice(&bad_offset.to_be_bytes());
    let bad_length: u32 = 100;
    data[24..28].copy_from_slice(&bad_length.to_be_bytes());

    match SfntTableMap::parse(&data) {
        Err(SfntError::OutOfBounds(t)) => {
            assert_eq!(t, tag, "OutOfBounds must report the offending tag");
        }
        other => panic!("expected OutOfBounds, got {:?}", other),
    }
}
