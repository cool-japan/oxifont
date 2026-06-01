//! Tests for CBDT/CBLC bitmap-table subsetting and MATH table Coverage remapping.

use oxifont_subset::cbdt::rewrite_cbdt_cblc;
use oxifont_subset::math::rewrite_math;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers: binary builders
// ---------------------------------------------------------------------------

fn u16be(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn u32be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// Build a minimal CBLC table.
///
/// Header: majorVersion=2, minorVersion=0, numSizes (u32).
/// Each BitmapSizeRecord is 48 bytes:
///   indexSubTableArrayOffset (u32)
///   indexTablesSize (u32)
///   numberOfIndexSubTables (u32)
///   colorRef (u32)
///   hori SbitLineMetrics (12 bytes, zero)
///   vert SbitLineMetrics (12 bytes, zero)
///   startGlyphIndex (u16)
///   endGlyphIndex (u16)
///   ppemX, ppemY, bitDepth, flags (1 byte each)
fn build_cblc(records: &[(u16, u16)]) -> Vec<u8> {
    let num_sizes = records.len() as u32;
    // Place IndexSubTable data immediately after the record array.
    let records_end_offset = (8 + num_sizes as usize * 48) as u32;

    let mut out = Vec::new();
    // Header
    out.extend_from_slice(&u16be(2)); // majorVersion
    out.extend_from_slice(&u16be(0)); // minorVersion
    out.extend_from_slice(&u32be(num_sizes)); // numSizes

    // Records
    for &(start_gid, end_gid) in records {
        // indexSubTableArrayOffset — points just after all records (body start)
        out.extend_from_slice(&u32be(records_end_offset));
        // indexTablesSize
        out.extend_from_slice(&u32be(0));
        // numberOfIndexSubTables
        out.extend_from_slice(&u32be(0));
        // colorRef
        out.extend_from_slice(&u32be(0));
        // hori SbitLineMetrics (12 bytes)
        out.extend_from_slice(&[0u8; 12]);
        // vert SbitLineMetrics (12 bytes)
        out.extend_from_slice(&[0u8; 12]);
        // startGlyphIndex
        out.extend_from_slice(&u16be(start_gid));
        // endGlyphIndex
        out.extend_from_slice(&u16be(end_gid));
        // ppemX, ppemY, bitDepth, flags
        out.extend_from_slice(&[16u8, 16u8, 32u8, 0u8]);
    }

    // Minimal body (no actual IndexSubTable data for these synthetic tests).
    // Write 4 zero bytes so the table is not empty after the records.
    out.extend_from_slice(&[0u8; 4]);

    out
}

/// Build a minimal CBDT table (8-byte header + empty body).
fn build_cbdt() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&u16be(2)); // majorVersion
    out.extend_from_slice(&u16be(0)); // minorVersion
    out.extend_from_slice(&u32be(0)); // (length-like field not in spec, zero pad)
    out
}

// ---------------------------------------------------------------------------
// CBDT/CBLC tests
// ---------------------------------------------------------------------------

/// A BitmapSizeRecord whose GID range has no survivors should be dropped.
#[test]
fn test_cblc_drops_out_of_range_strike() {
    // Two strikes:
    //   Strike A: GIDs 10..=15  (all survive)
    //   Strike B: GIDs 20..=25  (none survive — all removed)
    let cblc = build_cblc(&[(10, 15), (20, 25)]);
    let cbdt = build_cbdt();

    // Only GIDs 0 and 10..=15 survive.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    for (i, g) in (10u16..=15).enumerate() {
        remap.insert(g, 1 + i as u16);
    }

    let (new_cblc, _new_cbdt) = rewrite_cbdt_cblc(&cblc, &cbdt, &remap);

    // Parse numSizes from output header.
    let num_sizes = u32::from_be_bytes([new_cblc[4], new_cblc[5], new_cblc[6], new_cblc[7]]);
    assert_eq!(
        num_sizes, 1,
        "strike B (GIDs 20..=25 all removed) should be dropped"
    );
}

/// Surviving GID boundaries should be remapped to the new GID values.
#[test]
fn test_cblc_remaps_gid_boundaries() {
    // One strike: GIDs 5..=7.
    // Remap: {0→0, 5→3, 6→4, 7→5}  — all three survive, contiguous.
    let cblc = build_cblc(&[(5, 7)]);
    let cbdt = build_cbdt();

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(5, 3);
    remap.insert(6, 4);
    remap.insert(7, 5);

    let (new_cblc, _new_cbdt) = rewrite_cbdt_cblc(&cblc, &cbdt, &remap);

    // Check numSizes = 1.
    let num_sizes = u32::from_be_bytes([new_cblc[4], new_cblc[5], new_cblc[6], new_cblc[7]]);
    assert_eq!(num_sizes, 1, "strike should survive");

    // Record starts at byte 8 (after header).
    // startGlyphIndex is at record offset 40 (u16).
    // endGlyphIndex is at record offset 42 (u16).
    let rec_base = 8usize;
    let new_start = u16::from_be_bytes([new_cblc[rec_base + 40], new_cblc[rec_base + 41]]);
    let new_end = u16::from_be_bytes([new_cblc[rec_base + 42], new_cblc[rec_base + 43]]);
    assert_eq!(new_start, 3, "startGlyphIndex should remap to 3");
    assert_eq!(new_end, 5, "endGlyphIndex should remap to 5");
}

/// CBLC with two strikes, one partially surviving, one fully removed.
#[test]
fn test_cblc_keeps_partially_surviving_strike() {
    // Strike A: GIDs 1..=3 — only GID 1 survives (→ new GID 1).
    // Strike B: GIDs 10..=12 — none survive.
    let cblc = build_cblc(&[(1, 3), (10, 12)]);
    let cbdt = build_cbdt();

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1); // only GID 1 survives from strike A

    let (new_cblc, _new_cbdt) = rewrite_cbdt_cblc(&cblc, &cbdt, &remap);

    let num_sizes = u32::from_be_bytes([new_cblc[4], new_cblc[5], new_cblc[6], new_cblc[7]]);
    assert_eq!(
        num_sizes, 1,
        "only strike A (partially surviving) should remain"
    );

    let rec_base = 8usize;
    let new_start = u16::from_be_bytes([new_cblc[rec_base + 40], new_cblc[rec_base + 41]]);
    let new_end = u16::from_be_bytes([new_cblc[rec_base + 42], new_cblc[rec_base + 43]]);
    assert_eq!(new_start, 1, "startGlyphIndex should be 1");
    assert_eq!(new_end, 1, "endGlyphIndex should be 1");
}

/// Empty gid_remap (only .notdef) → all non-notdef strikes dropped.
#[test]
fn test_cblc_all_strikes_dropped() {
    let cblc = build_cblc(&[(5, 10), (20, 30)]);
    let cbdt = build_cbdt();

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0); // only .notdef

    let (new_cblc, _new_cbdt) = rewrite_cbdt_cblc(&cblc, &cbdt, &remap);

    let num_sizes = u32::from_be_bytes([new_cblc[4], new_cblc[5], new_cblc[6], new_cblc[7]]);
    assert_eq!(
        num_sizes, 0,
        "all strikes should be dropped when no GIDs survive"
    );
}

// ---------------------------------------------------------------------------
// MATH tests
// ---------------------------------------------------------------------------

/// A MATH table shorter than 10 bytes must be returned verbatim.
#[test]
fn test_math_passthrough_on_short_input() {
    let table = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // 8 bytes
    let remap: HashMap<u16, u16> = HashMap::new();
    let out = rewrite_math(&table, &remap);
    assert_eq!(out, table, "short MATH table must be returned verbatim");
}

/// MATH with MathGlyphInfoOffset pointing to a minimal MathGlyphInfo that
/// contains a MathItalicsCorrectionInfo with a Coverage format 1 listing GIDs
/// 2, 3, 4.  Removing GID 3 from the remap → output Coverage should list
/// new GIDs 2 and 4 (remapped: 2→1, 3 removed, 4→2).
#[test]
fn test_math_remap_coverage_in_glyph_info() {
    // Layout:
    //   [0..10)  MATH header
    //   [10..18) MathGlyphInfo (4 × u16 sub-offsets)
    //   [18..34) MathItalicsCorrectionInfo:
    //              ItalicsCorrectionCoverageOffset u16 = 4 (→ [22..))
    //              (2 bytes pad)
    //              ItalicsCorrectionCount u16 = 3
    //              ... (3 × MathValueRecord: u16 + u16 = 4 bytes each = 12 bytes)
    //   [22..34) Coverage format 1: GIDs 2, 3, 4
    //   [34..38) MathVariants stub (just big enough for header: 10 bytes)
    //
    // Offsets (relative to MATH start):
    //   MATH header[4] = MathConstantsOffset = 0 (no MathConstants sub-table)
    //   MATH header[6] = MathGlyphInfoOffset = 10
    //   MATH header[8] = MathVariantsOffset = 34
    //
    // MathGlyphInfo (at offset 10):
    //   [0] MathItalicsCorrectionInfoOffset = 8  (→ 10+8=18)
    //   [2] MathTopAccentAttachmentOffset = 0
    //   [4] ExtendedShapeCoverageOffset = 0
    //   [6] MathKernInfoOffset = 0
    //
    // MathItalicsCorrectionInfo (at offset 18):
    //   [0..2] ItalicsCorrectionCoverageOffset = 4  (→ 18+4=22)
    //   [2..4] ItalicsCorrectionCount = 3
    //   [4..16] 3 × MathValueRecord (zero data)
    //
    // Coverage format 1 (at offset 22):
    //   format=1 (u16), count=3 (u16), GIDs: 2, 3, 4
    //   = 4 + 3*2 = 10 bytes → [22..32)
    //
    // MathVariants stub (at offset 34):
    //   MinConnectorOverlap=0 u16
    //   VertGlyphCoverageOffset=0 u16
    //   HorizGlyphCoverageOffset=0 u16
    //   VertGlyphCount=0 u16
    //   HorizGlyphCount=0 u16  → 10 bytes → [34..44)

    let mut table = vec![0u8; 44];

    // MATH header (10 bytes at [0..10)):
    table[0..2].copy_from_slice(&u16be(1)); // majorVersion
    table[2..4].copy_from_slice(&u16be(0)); // minorVersion
    table[4..6].copy_from_slice(&u16be(0)); // MathConstantsOffset = 0 (absent)
    table[6..8].copy_from_slice(&u16be(10)); // MathGlyphInfoOffset = 10
    table[8..10].copy_from_slice(&u16be(34)); // MathVariantsOffset = 34

    // MathGlyphInfo at [10..18):
    table[10..12].copy_from_slice(&u16be(8)); // MathItalicsCorrectionInfoOffset = 8 → abs 18
    table[12..14].copy_from_slice(&u16be(0)); // MathTopAccentAttachmentOffset
    table[14..16].copy_from_slice(&u16be(0)); // ExtendedShapeCoverageOffset
    table[16..18].copy_from_slice(&u16be(0)); // MathKernInfoOffset

    // MathItalicsCorrectionInfo at [18..34):
    // ItalicsCorrectionCoverageOffset = 4 → abs 22
    table[18..20].copy_from_slice(&u16be(4));
    // ItalicsCorrectionCount = 3
    table[20..22].copy_from_slice(&u16be(3));
    // 3 × MathValueRecord (zero; 4 bytes each → 12 bytes at [22..34))
    // But coverage is at [22..32), so MathValueRecords would be at [32..44).
    // Let's put MathValueRecords after the coverage instead.
    // Actually the spec says coverage is pointed to by the offset, and the
    // MathValueRecord array is after ItalicsCorrectionCount (at offset +4 in the sub-table).
    // We already put coverage at sub-table+4 = 22. MathValueRecords are at sub-table+4
    // is typically wrong — they're parallel to coverage GIDs.  For test purposes
    // coverage occupies [22..32) and MathValueRecords can overlap; we only care that
    // the coverage GIDs at [22..32) are rewritten correctly.

    // Coverage format 1 at [22..32): format=1, count=3, GIDs=2,3,4
    table[22..24].copy_from_slice(&u16be(1)); // format 1
    table[24..26].copy_from_slice(&u16be(3)); // count = 3
    table[26..28].copy_from_slice(&u16be(2)); // GID 2
    table[28..30].copy_from_slice(&u16be(3)); // GID 3
    table[30..32].copy_from_slice(&u16be(4)); // GID 4

    // MathVariants stub at [34..44): all zeros → no coverage to remap.
    // (already zeroed)

    // Remap: keep GIDs 0, 2, 4 (remap 2→1, 4→2); drop GID 3.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(2, 1);
    remap.insert(4, 2);

    let out = rewrite_math(&table, &remap);

    // The Coverage at bytes [22..32) should now list new GIDs 1 and 2
    // (remapped from old GIDs 2 and 4; old GID 3 dropped).
    // Coverage format 1, count=2: format=1(u16) count=2(u16) gid=1(u16) gid=2(u16) → 8 bytes.
    let cov_format = u16::from_be_bytes([out[22], out[23]]);
    let cov_count = u16::from_be_bytes([out[24], out[25]]);
    let cov_gid0 = u16::from_be_bytes([out[26], out[27]]);
    let cov_gid1 = u16::from_be_bytes([out[28], out[29]]);

    assert_eq!(cov_format, 1, "coverage format should remain 1");
    assert_eq!(
        cov_count, 2,
        "coverage count should be 2 after removing GID 3"
    );
    assert_eq!(
        cov_gid0, 1,
        "first remapped GID should be 1 (old GID 2 → new GID 1)"
    );
    assert_eq!(
        cov_gid1, 2,
        "second remapped GID should be 2 (old GID 4 → new GID 2)"
    );
}

/// MATH passthrough: if MATH table has zero GlyphInfo and Variants offsets,
/// it should come back unchanged.
#[test]
fn test_math_zero_offsets_passthrough() {
    // Minimal valid MATH: header with all offsets = 0 (no sub-tables).
    let mut table = vec![0u8; 10];
    table[0..2].copy_from_slice(&u16be(1)); // majorVersion
                                            // All offsets zero → nothing to remap.

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1);

    let out = rewrite_math(&table, &remap);
    assert_eq!(out, table, "zero-offset MATH should be returned unchanged");
}
