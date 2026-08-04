//! Integration tests for the `ttcf` (TrueType Collection) support in the
//! `sfnt` module: [`face_count`], [`face_offset`], and
//! [`SfntTableMap::parse_face`].
//!
//! Every stock Windows CJK family ships as a collection, so selecting a face
//! out of one has to work — and a hostile collection header has to be refused
//! rather than trusted.

use oxifont_core::sfnt::{face_count, face_offset, SfntError, SfntTableMap, TTC_MAGIC};

/// The same fixture used by `oxifont-parser` tests.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Synthetic collection builder
// ---------------------------------------------------------------------------

/// Read `(tag, bytes)` for every table of a plain per-face SFNT.
fn read_tables(sfnt: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    (0..num_tables)
        .map(|i| {
            let rec = 12 + i * 16;
            let tag = [sfnt[rec], sfnt[rec + 1], sfnt[rec + 2], sfnt[rec + 3]];
            let offset =
                u32::from_be_bytes([sfnt[rec + 8], sfnt[rec + 9], sfnt[rec + 10], sfnt[rec + 11]])
                    as usize;
            let length = u32::from_be_bytes([
                sfnt[rec + 12],
                sfnt[rec + 13],
                sfnt[rec + 14],
                sfnt[rec + 15],
            ]) as usize;
            (tag, sfnt[offset..offset + length].to_vec())
        })
        .collect()
}

/// Build a spec-shaped two-face `ttcf` collection wrapping two copies of
/// `sfnt`, with absolute table offsets as the OpenType spec requires.
fn build_two_face_ttc(sfnt: &[u8]) -> Vec<u8> {
    let sfnt_version = u32::from_be_bytes([sfnt[0], sfnt[1], sfnt[2], sfnt[3]]);
    let tables = read_tables(sfnt);
    let n = tables.len();

    let ttc_header_len = 12 + 4 * 2;
    let face0_dir = ttc_header_len;
    let face1_dir = face0_dir + 12 + n * 16;
    let mut cursor = (face1_dir + 12 + n * 16 + 3) & !3;

    let mut placed: Vec<Vec<(usize, usize)>> = Vec::with_capacity(2);
    for _ in 0..2 {
        let mut offsets = Vec::with_capacity(n);
        for (_, data) in &tables {
            offsets.push((cursor, data.len()));
            cursor += (data.len() + 3) & !3;
        }
        placed.push(offsets);
    }

    let mut out = vec![0u8; cursor];
    out[0..4].copy_from_slice(&TTC_MAGIC.to_be_bytes());
    out[4..6].copy_from_slice(&1u16.to_be_bytes()); // majorVersion
    out[6..8].copy_from_slice(&0u16.to_be_bytes()); // minorVersion
    out[8..12].copy_from_slice(&2u32.to_be_bytes()); // numFonts
    out[12..16].copy_from_slice(&(face0_dir as u32).to_be_bytes());
    out[16..20].copy_from_slice(&(face1_dir as u32).to_be_bytes());

    for (face_idx, dir_start) in [face0_dir, face1_dir].into_iter().enumerate() {
        out[dir_start..dir_start + 4].copy_from_slice(&sfnt_version.to_be_bytes());
        out[dir_start + 4..dir_start + 6].copy_from_slice(&(n as u16).to_be_bytes());
        for (i, (tag, data)) in tables.iter().enumerate() {
            let (offset, length) = placed[face_idx][i];
            let rec = dir_start + 12 + i * 16;
            out[rec..rec + 4].copy_from_slice(tag);
            out[rec + 8..rec + 12].copy_from_slice(&(offset as u32).to_be_bytes());
            out[rec + 12..rec + 16].copy_from_slice(&(length as u32).to_be_bytes());
            out[offset..offset + length].copy_from_slice(data);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// face_count / face_offset
// ---------------------------------------------------------------------------

#[test]
fn plain_sfnt_holds_one_face_at_offset_zero() {
    assert_eq!(face_count(TTF).expect("a plain TTF must report a count"), 1);
    assert_eq!(face_offset(TTF, 0).expect("face 0 must resolve"), 0);
}

#[test]
fn plain_sfnt_rejects_a_non_zero_face_index() {
    match face_offset(TTF, 1) {
        Err(SfntError::FaceIndexOutOfRange { index, count }) => {
            assert_eq!(index, 1);
            assert_eq!(count, 1);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other:?}"),
    }
}

#[test]
fn collection_reports_its_declared_face_count() {
    let ttc = build_two_face_ttc(TTF);
    assert_eq!(face_count(&ttc).expect("collection must report a count"), 2);
    assert!(face_offset(&ttc, 0).expect("face 0 must resolve") > 0);
    assert!(
        face_offset(&ttc, 1).expect("face 1 must resolve")
            > face_offset(&ttc, 0).expect("face 0 must resolve")
    );
}

#[test]
fn collection_rejects_an_out_of_range_face_index() {
    let ttc = build_two_face_ttc(TTF);
    match face_offset(&ttc, 2) {
        Err(SfntError::FaceIndexOutOfRange { index, count }) => {
            assert_eq!(index, 2);
            assert_eq!(count, 2);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other:?}"),
    }
}

#[test]
fn unrecognised_magic_is_bad_magic() {
    match face_count(b"WOFF\x00\x01\x00\x00\x00\x00\x00\x01") {
        Err(SfntError::BadMagic(m)) => assert_eq!(m, 0x574F_4646),
        other => panic!("expected BadMagic, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// parse_face
// ---------------------------------------------------------------------------

#[test]
fn parse_face_reads_every_face_of_a_collection() {
    let ttc = build_two_face_ttc(TTF);
    let reference = SfntTableMap::parse(TTF).expect("fixture must parse");

    for index in 0..2 {
        let map = SfntTableMap::parse_face(&ttc, index).expect("collection face must parse");
        assert_eq!(map.num_tables(), reference.num_tables());
        assert_eq!(
            map.table(b"glyf"),
            reference.table(b"glyf"),
            "face {index} must expose the same glyf bytes as the source font"
        );
    }
}

#[test]
fn parse_face_zero_of_a_plain_sfnt_matches_parse() {
    let via_face = SfntTableMap::parse_face(TTF, 0).expect("face 0 must parse");
    let direct = SfntTableMap::parse(TTF).expect("fixture must parse");
    assert_eq!(via_face.num_tables(), direct.num_tables());
    assert_eq!(via_face.sfnt_version, direct.sfnt_version);
    assert_eq!(via_face.table(b"head"), direct.table(b"head"));
}

// ---------------------------------------------------------------------------
// Hostile collection headers
// ---------------------------------------------------------------------------

#[test]
fn zero_num_fonts_is_malformed() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[8..12].copy_from_slice(&0u32.to_be_bytes());
    assert_eq!(face_count(&ttc), Err(SfntError::MalformedCollection));
    assert_eq!(face_offset(&ttc, 0), Err(SfntError::MalformedCollection));
}

#[test]
fn unknown_collection_version_is_malformed() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[4..6].copy_from_slice(&3u16.to_be_bytes());
    assert_eq!(face_count(&ttc), Err(SfntError::MalformedCollection));
}

#[test]
fn num_fonts_larger_than_the_buffer_is_truncated() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[8..12].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    assert_eq!(
        face_count(&ttc),
        Err(SfntError::Truncated),
        "an offset table that cannot fit must be refused before it is read"
    );
}

#[test]
fn truncated_collection_header_is_truncated() {
    let mut stub = TTC_MAGIC.to_be_bytes().to_vec();
    stub.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]); // version only
    assert_eq!(face_count(&stub), Err(SfntError::Truncated));
}

#[test]
fn face_offset_past_the_end_is_refused_by_parse_face() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[16..20].copy_from_slice(&0xFFFF_0000u32.to_be_bytes());
    // The offset itself is resolvable — it is only the SFNT header at it that
    // cannot be read.
    assert_eq!(face_offset(&ttc, 1), Ok(0xFFFF_0000usize));
    match SfntTableMap::parse_face(&ttc, 1) {
        Err(SfntError::Truncated) => {}
        other => panic!("a face offset past EOF must not be trusted, got {other:?}"),
    }
    assert!(
        SfntTableMap::parse_face(&ttc, 0).is_ok(),
        "a broken face 1 must not poison face 0"
    );
}

#[test]
fn face_offset_at_non_sfnt_bytes_is_refused_by_parse_face() {
    let mut ttc = build_two_face_ttc(TTF);
    // Offset 8 is `numFonts` (0x00000002) — inside the buffer, but not an SFNT
    // header.
    ttc[16..20].copy_from_slice(&8u32.to_be_bytes());
    match SfntTableMap::parse_face(&ttc, 1) {
        Err(SfntError::BadMagic(m)) => assert_eq!(m, 2),
        other => panic!("expected BadMagic, got {other:?}"),
    }
}
