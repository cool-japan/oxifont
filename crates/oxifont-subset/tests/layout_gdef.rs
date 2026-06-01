//! Integration tests for Coverage, ClassDef, and GDEF layout helpers.

use std::collections::HashMap;

use oxifont_subset::layout::{
    read_classdef, read_coverage, remap_classdef, remap_coverage, rewrite_gdef, write_classdef,
    write_coverage,
};

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// Build a Coverage format 1 byte buffer manually.
fn make_cov_f1(gids: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&1u16.to_be_bytes()); // format 1
    v.extend_from_slice(&(gids.len() as u16).to_be_bytes());
    for &g in gids {
        v.extend_from_slice(&g.to_be_bytes());
    }
    v
}

/// Build a Coverage format 2 byte buffer manually.
fn make_cov_f2(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&2u16.to_be_bytes()); // format 2
    v.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
    for &(start, end, cov_idx) in ranges {
        v.extend_from_slice(&start.to_be_bytes());
        v.extend_from_slice(&end.to_be_bytes());
        v.extend_from_slice(&cov_idx.to_be_bytes());
    }
    v
}

#[test]
fn test_coverage_format1_roundtrip() {
    let gids = [10u16, 20, 30, 40, 50];
    let data = make_cov_f1(&gids);
    let parsed = read_coverage(&data, 0);
    assert_eq!(parsed, gids.as_slice());

    // write_coverage should round-trip: sparse GIDs → format 1 or 2.
    let rewritten = write_coverage(&parsed);
    let reparsed = read_coverage(&rewritten, 0);
    assert_eq!(reparsed, gids.as_slice());
}

#[test]
fn test_coverage_format2_roundtrip() {
    // Three ranges: [1,3], [10,12], [20,20].
    let ranges = [(1u16, 3u16, 0u16), (10, 12, 3), (20, 20, 6)];
    let data = make_cov_f2(&ranges);
    let parsed = read_coverage(&data, 0);
    assert_eq!(parsed, [1, 2, 3, 10, 11, 12, 20]);

    // Write back and verify identical GIDs.
    let rewritten = write_coverage(&parsed);
    let reparsed = read_coverage(&rewritten, 0);
    assert_eq!(reparsed, [1, 2, 3, 10, 11, 12, 20]);
}

#[test]
fn test_coverage_remap_drops_removed() {
    // GIDs 1, 2, 3 in coverage; remap 1→10, 3→20 (drop 2).
    let gids = [1u16, 2, 3];
    let data = make_cov_f1(&gids);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(1, 10);
    remap.insert(3, 20);

    let (new_cov_bytes, new_gids_ordered) = remap_coverage(&data, 0, &remap);
    // Ordered list should have 10 and 20 (in original slot order: slot0→1→10, slot2→3→20).
    assert_eq!(new_gids_ordered, [10, 20]);

    // New coverage should contain sorted [10, 20].
    let reparsed = read_coverage(&new_cov_bytes, 0);
    assert_eq!(reparsed, [10, 20]);
}

#[test]
fn test_coverage_empty() {
    let bytes = write_coverage(&[]);
    let parsed = read_coverage(&bytes, 0);
    assert_eq!(parsed, [] as [u16; 0]);
}

#[test]
fn test_coverage_single() {
    let bytes = write_coverage(&[42u16]);
    let parsed = read_coverage(&bytes, 0);
    assert_eq!(parsed, [42u16]);
}

// ---------------------------------------------------------------------------
// ClassDef
// ---------------------------------------------------------------------------

/// Build a ClassDef format 1 buffer.
fn make_classdef_f1(start_gid: u16, classes: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&1u16.to_be_bytes()); // format 1
    v.extend_from_slice(&start_gid.to_be_bytes());
    v.extend_from_slice(&(classes.len() as u16).to_be_bytes());
    for &c in classes {
        v.extend_from_slice(&c.to_be_bytes());
    }
    v
}

/// Build a ClassDef format 2 buffer.
fn make_classdef_f2(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&2u16.to_be_bytes()); // format 2
    v.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
    for &(start, end, class) in ranges {
        v.extend_from_slice(&start.to_be_bytes());
        v.extend_from_slice(&end.to_be_bytes());
        v.extend_from_slice(&class.to_be_bytes());
    }
    v
}

#[test]
fn test_classdef_format1_roundtrip() {
    // start_gid=5, classes=[1,2,3] → GIDs 5→1, 6→2, 7→3
    let data = make_classdef_f1(5, &[1, 2, 3]);
    let map = read_classdef(&data, 0);
    assert_eq!(map.get(&5), Some(&1));
    assert_eq!(map.get(&6), Some(&2));
    assert_eq!(map.get(&7), Some(&3));
    assert_eq!(map.get(&8), None);

    // Write back and re-parse.
    let rewritten = write_classdef(&map);
    let remap2 = read_classdef(&rewritten, 0);
    assert_eq!(remap2.get(&5), Some(&1));
    assert_eq!(remap2.get(&6), Some(&2));
    assert_eq!(remap2.get(&7), Some(&3));
}

#[test]
fn test_classdef_format2_roundtrip() {
    // GIDs 1–3 → class 1, GIDs 10–12 → class 2.
    let ranges = [(1u16, 3u16, 1u16), (10, 12, 2)];
    let data = make_classdef_f2(&ranges);
    let map = read_classdef(&data, 0);
    assert_eq!(map.get(&1), Some(&1));
    assert_eq!(map.get(&3), Some(&1));
    assert_eq!(map.get(&10), Some(&2));
    assert_eq!(map.get(&12), Some(&2));
    assert_eq!(map.get(&5), None);

    let rewritten = write_classdef(&map);
    let remap2 = read_classdef(&rewritten, 0);
    assert_eq!(remap2.get(&1), Some(&1));
    assert_eq!(remap2.get(&10), Some(&2));
}

#[test]
fn test_classdef_remap() {
    // GIDs 1→class 1, 2→class 2, 3→class 1 (format 1).
    let data = make_classdef_f1(1, &[1, 2, 1]);
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(1, 100);
    remap.insert(3, 300);
    // GID 2 is dropped.

    let result = remap_classdef(&data, 0, &remap);
    let out_map = read_classdef(&result, 0);
    assert_eq!(out_map.get(&100), Some(&1));
    assert_eq!(out_map.get(&300), Some(&1));
    assert_eq!(out_map.get(&2), None); // was dropped
    assert_eq!(out_map.get(&1), None); // old GIDs not present
}

// ---------------------------------------------------------------------------
// GDEF
// ---------------------------------------------------------------------------

/// Build a minimal GDEF v1.0 table with only GlyphClassDef and
/// MarkAttachClassDef populated; AttachList and LigCaretList are 0 (absent).
///
/// Layout:
///   offset  0: majorVersion = 1
///   offset  2: minorVersion = 0
///   offset  4: GlyphClassDef  (Offset16 → ClassDef_f1 for GIDs 1–3)
///   offset  6: AttachList     = 0
///   offset  8: LigCaretList   = 0
///   offset 10: MarkAttachClassDef (Offset16 → ClassDef_f2 for GIDs 10–12)
///   offset 12: ClassDef_f1 data (glyph_class)
///              ClassDef_f2 data (mark_attach)
fn build_synthetic_gdef() -> Vec<u8> {
    let mut glyph_class = make_classdef_f1(1, &[1, 2, 3]); // 6 + 3*2 = 12 bytes
    let mut mark_attach = make_classdef_f2(&[(10, 12, 3)]); // 4 + 6 = 10 bytes

    // Ensure both are properly formatted.
    assert_eq!(glyph_class.len(), 12);
    assert_eq!(mark_attach.len(), 10);

    let glyph_class_off: u16 = 12; // right after the 12-byte header
    let mark_attach_off: u16 = 12 + glyph_class.len() as u16;

    let mut out = Vec::new();
    out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&glyph_class_off.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // AttachList absent
    out.extend_from_slice(&0u16.to_be_bytes()); // LigCaretList absent
    out.extend_from_slice(&mark_attach_off.to_be_bytes());
    out.append(&mut glyph_class);
    out.append(&mut mark_attach);
    out
}

#[test]
fn test_rewrite_gdef_synthetic() {
    let gdef = build_synthetic_gdef();

    // Remap: keep GIDs 1 and 3 (drop 2), remap 10 (drop 11, 12).
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1);
    remap.insert(3, 2);
    remap.insert(10, 3);

    let result = rewrite_gdef(&gdef, &remap);

    // Must be valid GDEF: version 1.0 at offset 0.
    assert!(result.len() >= 12);
    let major = u16::from_be_bytes([result[0], result[1]]);
    let minor = u16::from_be_bytes([result[2], result[3]]);
    assert_eq!(major, 1);
    assert_eq!(minor, 0);

    // Parse GlyphClassDef from the result.
    let gc_off = u16::from_be_bytes([result[4], result[5]]) as usize;
    assert_ne!(gc_off, 0, "GlyphClassDef offset should be non-zero");
    let gc_map = read_classdef(&result, gc_off);
    // GID 1 (new) should have class 1, GID 2 (new, remapped from old 3) should have class 3.
    assert_eq!(gc_map.get(&1), Some(&1));
    assert_eq!(gc_map.get(&2), Some(&3));
    // Old GID 2 was dropped; new map shouldn't have old GID keys.

    // Parse MarkAttachClassDef.
    let ma_off = u16::from_be_bytes([result[10], result[11]]) as usize;
    assert_ne!(ma_off, 0, "MarkAttachClassDef offset should be non-zero");
    let ma_map = read_classdef(&result, ma_off);
    // New GID 3 (remapped from old 10) should have class 3.
    assert_eq!(ma_map.get(&3), Some(&3));
}

#[test]
fn test_rewrite_gdef_verbatim_on_failure() {
    let bad_data = b"short";
    let gid_remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gdef(bad_data, &gid_remap);
    assert_eq!(result, bad_data.as_slice());
}

#[test]
fn test_rewrite_gdef_verbatim_on_bad_version() {
    // Build a GDEF with major version 2 (unknown → verbatim).
    let mut data = vec![0u8; 12];
    data[0] = 0;
    data[1] = 2; // major = 2
    let gid_remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gdef(&data, &gid_remap);
    assert_eq!(result, data.as_slice());
}

#[test]
fn test_rewrite_gdef_all_absent() {
    // GDEF with all offsets zero (all sub-tables absent).
    let mut data = vec![0u8; 12];
    data[0] = 0;
    data[1] = 1; // major = 1
                 // minor = 0, all offsets = 0.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    let result = rewrite_gdef(&data, &remap);
    // Should produce a valid GDEF v1.0 with all zero offsets.
    assert!(result.len() >= 12);
    assert_eq!(u16::from_be_bytes([result[0], result[1]]), 1);
    assert_eq!(u16::from_be_bytes([result[2], result[3]]), 0);
    // All offset fields should remain 0.
    assert_eq!(u16::from_be_bytes([result[4], result[5]]), 0);
    assert_eq!(u16::from_be_bytes([result[6], result[7]]), 0);
    assert_eq!(u16::from_be_bytes([result[8], result[9]]), 0);
    assert_eq!(u16::from_be_bytes([result[10], result[11]]), 0);
}
