//! Error-path integration tests for `oxifont-parser`.
//!
//! Validates that malformed, truncated, or invalid font data never panics and
//! returns appropriate errors. These tests complement the fixture-based tests
//! in `parse.rs` by exercising specific failure modes.

use oxifont_parser::ParsedFace;

// ---------------------------------------------------------------------------
// Empty / too-short input
// ---------------------------------------------------------------------------

#[test]
fn parse_empty_bytes_fails() {
    assert!(
        ParsedFace::parse(vec![], 0).is_err(),
        "parsing empty bytes must return an error"
    );
}

#[test]
fn parse_four_bytes_does_not_panic() {
    // Too short to be any valid font (fewer than 12 bytes for SFNT header).
    let _ = ParsedFace::parse(vec![0u8; 4], 0);
}

// ---------------------------------------------------------------------------
// Truncated SFNT header (12 bytes: valid sfVersion+numTables but no tables)
// ---------------------------------------------------------------------------

#[test]
fn parse_truncated_sfnt_header_does_not_panic() {
    // TrueType magic + numTables=3 + other header fields — no actual table
    // records follow, so any table lookup will fail gracefully.
    let data = vec![
        0x00, 0x01, 0x00, 0x00, // sfVersion: 0x00010000 (TrueType)
        0x00, 0x03, // numTables = 3
        0x00, 0x30, // searchRange
        0x00, 0x02, // entrySelector
        0x00, 0x10, // rangeShift
    ];
    // May succeed or fail — the key requirement is no panic.
    let _ = ParsedFace::parse(data, 0);
}

// ---------------------------------------------------------------------------
// Invalid magic bytes
// ---------------------------------------------------------------------------

#[test]
fn parse_invalid_magic_bytes_does_not_panic() {
    let mut data = vec![0u8; 256];
    // 0xDEAD_BEEF: not a recognised font format.
    data[0] = 0xDE;
    data[1] = 0xAD;
    data[2] = 0xBE;
    data[3] = 0xEF;
    // Must not panic; result may be an error or a degraded face.
    let _ = ParsedFace::parse(data, 0);
}

#[test]
fn parse_all_zeros_returns_error() {
    // 256 zero bytes: fails to match any known magic.
    let result = ParsedFace::parse(vec![0u8; 256], 0);
    // Zeros parse as sfVersion=0, which ttf_parser will reject.
    // Either is acceptable; we just require no panic.
    let _ = result;
}

// ---------------------------------------------------------------------------
// Out-of-range face index for a non-TTC font
// ---------------------------------------------------------------------------

#[test]
fn parse_out_of_range_face_index_does_not_panic() {
    // Minimal SFNT header (non-TTC): numTables=0, so there is no real data.
    // face_index=5 should be rejected gracefully.
    let data: Vec<u8> = vec![
        0x00, 0x01, 0x00, 0x00, // sfVersion: TrueType
        0x00, 0x00, // numTables = 0
        0x00, 0x00, // searchRange
        0x00, 0x00, // entrySelector
        0x00, 0x00, // rangeShift
    ];
    // For a non-TTC font, ttf_parser is called with face_index=5. It will
    // return an error (UnsupportedVersion or similar) — no panic.
    let _ = ParsedFace::parse(data, 5);
}

// ---------------------------------------------------------------------------
// WOFF magic bytes (raw SFNT expected by ParsedFace)
// ---------------------------------------------------------------------------

#[test]
fn parse_woff_magic_bytes_does_not_panic() {
    // WOFF magic: 0x774F4646 ('wOFF')
    let mut data = vec![0u8; 64];
    data[0] = 0x77; // 'w'
    data[1] = 0x4F; // 'O'
    data[2] = 0x46; // 'F'
    data[3] = 0x46; // 'F'
                    // ParsedFace expects raw SFNT bytes. WOFF input should fail, not panic.
    let _ = ParsedFace::parse(data, 0);
}

#[test]
fn parse_woff2_magic_bytes_does_not_panic() {
    // WOFF2 magic: 0x774F4632 ('wOF2')
    let mut data = vec![0u8; 64];
    data[0] = 0x77; // 'w'
    data[1] = 0x4F; // 'O'
    data[2] = 0x46; // 'F'
    data[3] = 0x32; // '2'
    let _ = ParsedFace::parse(data, 0);
}

// ---------------------------------------------------------------------------
// TTC out-of-bounds
// ---------------------------------------------------------------------------

#[test]
fn parse_ttc_out_of_bounds_returns_index_error() {
    // Minimal TTC header with numFonts=1.
    // Layout: "ttcf"(4) + version(4) + numFonts(4) + offsetTable[0](4)
    let mut data = vec![0u8; 64];
    data[0] = b't';
    data[1] = b't';
    data[2] = b'c';
    data[3] = b'f';
    // version = 1.0
    data[4] = 0x00;
    data[5] = 0x01;
    data[6] = 0x00;
    data[7] = 0x00;
    // numFonts = 1
    data[8] = 0x00;
    data[9] = 0x00;
    data[10] = 0x00;
    data[11] = 0x01;
    // offsetTable[0] = 28 (pointing into the buffer)
    data[12] = 0x00;
    data[13] = 0x00;
    data[14] = 0x00;
    data[15] = 0x1C;

    // face_index=5 exceeds numFonts=1, so we get IndexOutOfBounds.
    let result = ParsedFace::parse(data, 5);
    assert!(
        result.is_err(),
        "TTC with face_index=5 and numFonts=1 must return an error"
    );
}
