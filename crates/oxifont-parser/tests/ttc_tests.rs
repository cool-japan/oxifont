//! Integration tests for TrueType Collection (TTC) parsing in `oxifont-parser`.
//!
//! These tests exercise TTC-specific behaviour using synthetic in-memory
//! buffers so no fixture file is required. The helper `build_minimal_ttc`
//! constructs the smallest valid TTC header followed by bare-minimum SFNT
//! headers for each face.
//!
//! # TTC file layout (version 1.0)
//! ```text
//! offset  0 : tag        "ttcf"  (4 bytes)
//! offset  4 : version    0x0001_0000  (u32 big-endian)
//! offset  8 : numFonts   (u32 big-endian)
//! offset 12 : offsetTable[0..numFonts]  (u32 each)
//! ```
//! Each SFNT header referenced by an offset in `offsetTable` is the
//! minimal 12-byte form: `sfVersion`(u32) + `numTables`(u16) + `searchRange`(u16)
//! + `entrySelector`(u16) + `rangeShift`(u16).

use oxifont_parser::{face_count, ParsedFace};

// ---------------------------------------------------------------------------
// Helper: build a syntactically correct minimal TTC
// ---------------------------------------------------------------------------

/// Builds the smallest byte sequence that ttf_parser accepts as a TTC with
/// `face_count` sub-faces.
///
/// Each sub-face is a minimal SFNT header (12 bytes) with zero tables.
/// `ttf_parser::Face::parse` will return an error for these because the
/// required tables (cmap, head, …) are absent, but the TTC wrapper itself
/// is valid and the face-count extraction should succeed.
fn build_minimal_ttc(num_faces: u32) -> Vec<u8> {
    let mut buf = Vec::new();

    // TTC tag.
    buf.extend_from_slice(b"ttcf");
    // version 1.0 = 0x0001_0000.
    buf.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    // numFonts.
    buf.extend_from_slice(&num_faces.to_be_bytes());

    // Offset of each SFNT header within the buffer.
    // Header ends at byte 12 + 4 * num_faces.
    let header_size: u32 = 12 + 4 * num_faces;
    for i in 0..num_faces {
        let offset = header_size + i * 12;
        buf.extend_from_slice(&offset.to_be_bytes());
    }

    // Minimal SFNT header for each face (12 bytes, no tables).
    for _ in 0..num_faces {
        // sfVersion: 0x0001_0000 (TrueType).
        buf.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        // numTables = 0.
        buf.extend_from_slice(&0u16.to_be_bytes());
        // searchRange, entrySelector, rangeShift: all zero.
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
    }

    buf
}

// ---------------------------------------------------------------------------
// face_count()
// ---------------------------------------------------------------------------

#[test]
fn face_count_returns_correct_value_for_synthetic_ttc() {
    let data = build_minimal_ttc(3);
    assert_eq!(
        face_count(&data),
        3,
        "face_count must return 3 for a TTC with numFonts=3"
    );
}

#[test]
fn face_count_returns_one_for_single_face_ttc() {
    let data = build_minimal_ttc(1);
    assert_eq!(
        face_count(&data),
        1,
        "face_count must return 1 for a TTC with numFonts=1"
    );
}

#[test]
fn face_count_returns_one_for_non_ttc_bytes() {
    // A plain TTF (non-TTC) has no `ttcf` tag; face_count defaults to 1.
    let data = vec![
        0x00, 0x01, 0x00, 0x00, // sfVersion: TrueType
        0x00, 0x00, // numTables = 0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // searchRange, entrySelector, rangeShift
    ];
    assert_eq!(
        face_count(&data),
        1,
        "face_count must return 1 for a plain TTF/SFNT buffer"
    );
}

// ---------------------------------------------------------------------------
// ParsedFace::parse with TTC index validation
// ---------------------------------------------------------------------------

#[test]
fn parse_ttc_index_zero_does_not_panic() {
    // Index 0 is within the valid range of a 3-face TTC; parse may succeed
    // or fail (no real tables) but must never panic.
    let data = build_minimal_ttc(3);
    let _ = ParsedFace::parse(data, 0);
}

#[test]
fn parse_ttc_index_within_range_does_not_panic() {
    // All of indices 0, 1, 2 are valid in a 3-face TTC.
    let data = build_minimal_ttc(3);
    for idx in 0..3u32 {
        let result = ParsedFace::parse(data.clone(), idx);
        // The minimal SFNT will likely fail (no required tables), but must
        // not panic regardless of outcome.
        let _ = result;
    }
}

#[test]
fn parse_ttc_out_of_range_index_returns_error() {
    // face_index=5 exceeds numFonts=2, so the parser must return an error.
    let data = build_minimal_ttc(2);
    let result = ParsedFace::parse(data, 5);
    assert!(
        result.is_err(),
        "face_index=5 must produce an error for a TTC with numFonts=2"
    );
}

#[test]
fn parse_ttc_out_of_range_index_does_not_panic() {
    // Same as above but emphasises the no-panic invariant with a larger index.
    let data = build_minimal_ttc(1);
    let result = ParsedFace::parse(data, u32::MAX);
    assert!(
        result.is_err(),
        "face_index=u32::MAX must produce an error, not a panic"
    );
}

#[test]
fn parse_ttc_truncated_header_does_not_panic() {
    // Only the first 8 bytes of a TTC header (tag + version, no numFonts).
    let data = b"ttcf\x00\x01\x00\x00".to_vec();
    let _ = ParsedFace::parse(data, 0);
}

#[test]
fn parse_ttc_empty_collection_out_of_range() {
    // A syntactically valid TTC with numFonts=0.
    let mut data = Vec::new();
    data.extend_from_slice(b"ttcf");
    data.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes()); // numFonts = 0
                                                 // face_index=0 is out of range (count=0).
    let result = ParsedFace::parse(data, 0);
    assert!(
        result.is_err(),
        "face_index=0 must be an error when numFonts=0"
    );
}

// ---------------------------------------------------------------------------
// face_count with edge-case inputs
// ---------------------------------------------------------------------------

#[test]
fn face_count_empty_bytes_does_not_panic() {
    // face_count must not panic on empty input.
    let count = face_count(&[]);
    // ttf_parser returns None for unrecognised data; face_count maps that to 1.
    assert_eq!(count, 1, "face_count of empty bytes should fall back to 1");
}

#[test]
fn face_count_short_non_ttc_bytes_does_not_panic() {
    // Three bytes — not enough to be any valid format.
    let data = [0x00u8, 0x01, 0x00];
    let count = face_count(&data);
    // Undefined format → 1.
    assert_eq!(
        count, 1,
        "face_count of 3-byte non-TTC input should fall back to 1"
    );
}
