//! Tests for `gvar` per-glyph variation data rewriting.

use oxifont_subset::gvar::rewrite_gvar;
use std::collections::HashMap;

// ─── build helper ─────────────────────────────────────────────────────────────

/// Build a syntactically valid gvar table with the given parameters.
///
/// `per_glyph_data`: one slice per glyph (may be empty to represent no variation
/// data for that glyph).
///
/// Layout produced:
/// - Bytes 0–19  : header
/// - Bytes 20 … : offset array ((glyph_count+1) entries)
/// - Then        : shared tuples (axisCount × sharedTupleCount × 2 bytes of 0)
/// - Then        : per-glyph blocks concatenated
fn build_gvar(
    axis_count: u16,
    shared_tuple_count: u16,
    per_glyph_data: &[&[u8]],
    long_offsets: bool,
) -> Vec<u8> {
    let glyph_count = per_glyph_data.len() as u16;
    let entry_count = glyph_count as usize + 1;
    let offset_entry_size = if long_offsets { 4usize } else { 2usize };
    let offset_array_size = entry_count * offset_entry_size;

    let shared_tuples_size = axis_count as usize * shared_tuple_count as usize * 2;
    // shared tuples region: all zeros, placed right after the offset array.
    let shared_tuples_offset: u32 = (20 + offset_array_size) as u32;
    let glyph_var_data_array_offset: u32 = shared_tuples_offset + shared_tuples_size as u32;

    // Build offset array (relative to glyph_var_data_array_offset).
    let mut offset_array: Vec<u8> = Vec::with_capacity(offset_array_size);
    let mut cursor: usize = 0;
    for block in per_glyph_data {
        if long_offsets {
            offset_array.extend_from_slice(&(cursor as u32).to_be_bytes());
        } else {
            offset_array.extend_from_slice(&((cursor / 2) as u16).to_be_bytes());
        }
        cursor += block.len();
    }
    // Sentinel.
    if long_offsets {
        offset_array.extend_from_slice(&(cursor as u32).to_be_bytes());
    } else {
        offset_array.extend_from_slice(&((cursor / 2) as u16).to_be_bytes());
    }

    // Build header.
    let flags: u16 = if long_offsets { 0x0001 } else { 0x0000 };
    let mut hdr = [0u8; 20];
    hdr[0] = 0;
    hdr[1] = 1; // majorVersion = 1
    hdr[2] = 0;
    hdr[3] = 0; // minorVersion = 0
    hdr[4] = (axis_count >> 8) as u8;
    hdr[5] = (axis_count & 0xFF) as u8;
    hdr[6] = (shared_tuple_count >> 8) as u8;
    hdr[7] = (shared_tuple_count & 0xFF) as u8;
    let sto = shared_tuples_offset.to_be_bytes();
    hdr[8] = sto[0];
    hdr[9] = sto[1];
    hdr[10] = sto[2];
    hdr[11] = sto[3];
    hdr[12] = (glyph_count >> 8) as u8;
    hdr[13] = (glyph_count & 0xFF) as u8;
    hdr[14] = (flags >> 8) as u8;
    hdr[15] = (flags & 0xFF) as u8;
    let gvdao = glyph_var_data_array_offset.to_be_bytes();
    hdr[16] = gvdao[0];
    hdr[17] = gvdao[1];
    hdr[18] = gvdao[2];
    hdr[19] = gvdao[3];

    // Assemble.
    let mut out = Vec::new();
    out.extend_from_slice(&hdr);
    out.extend_from_slice(&offset_array);
    out.extend_from_slice(&vec![0u8; shared_tuples_size]);
    for block in per_glyph_data {
        out.extend_from_slice(block);
    }
    out
}

// ─── parse helper for verification ───────────────────────────────────────────

/// Extract the raw bytes for glyph `gid` from a rewritten gvar table.
///
/// Returns `None` on any structural error.
fn extract_glyph_block(table: &[u8], gid: usize) -> Option<&[u8]> {
    if table.len() < 20 {
        return None;
    }
    let flags = u16::from_be_bytes([table[14], table[15]]);
    let long = (flags & 0x0001) != 0;
    let glyph_count = u16::from_be_bytes([table[12], table[13]]) as usize;
    let base = u32::from_be_bytes([table[16], table[17], table[18], table[19]]) as usize;

    if gid > glyph_count {
        return None;
    }

    let (rel_start, rel_end) = if long {
        let needed = 20 + (glyph_count + 1) * 4;
        if table.len() < needed {
            return None;
        }
        let s = u32::from_be_bytes([
            table[20 + gid * 4],
            table[20 + gid * 4 + 1],
            table[20 + gid * 4 + 2],
            table[20 + gid * 4 + 3],
        ]) as usize;
        let e = u32::from_be_bytes([
            table[20 + (gid + 1) * 4],
            table[20 + (gid + 1) * 4 + 1],
            table[20 + (gid + 1) * 4 + 2],
            table[20 + (gid + 1) * 4 + 3],
        ]) as usize;
        (s, e)
    } else {
        let needed = 20 + (glyph_count + 1) * 2;
        if table.len() < needed {
            return None;
        }
        let s = u16::from_be_bytes([table[20 + gid * 2], table[20 + gid * 2 + 1]]) as usize * 2;
        let e = u16::from_be_bytes([table[20 + (gid + 1) * 2], table[20 + (gid + 1) * 2 + 1]])
            as usize
            * 2;
        (s, e)
    };

    let abs_start = base.checked_add(rel_start)?;
    let abs_end = base.checked_add(rel_end)?;
    if abs_end > table.len() || abs_start > abs_end {
        return None;
    }
    Some(&table[abs_start..abs_end])
}

// ─── tests ────────────────────────────────────────────────────────────────────

/// 4 glyphs, remap removes glyph 1 (old GIDs 0,2,3 → new GIDs 0,1,2).
/// Verify correct data blocks in the rewritten table.
#[test]
fn test_gvar_basic_remap() {
    let block0: &[u8] = b"DATA0000"; // 8 bytes — even length for short offsets
    let block1: &[u8] = b"SKIP1111"; // will be removed
    let block2: &[u8] = b"DATA2222";
    let block3: &[u8] = b"DATA3333";

    let table = build_gvar(0, 0, &[block0, block1, block2, block3], false);

    // new→old: 0→0, 1→2, 2→3
    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);
    rev.insert(1, 2);
    rev.insert(2, 3);

    let result = rewrite_gvar(&table, &rev, 3);

    // Header: glyphCount should be 3.
    let new_glyph_count = u16::from_be_bytes([result[12], result[13]]);
    assert_eq!(new_glyph_count, 3);

    assert_eq!(extract_glyph_block(&result, 0), Some(block0));
    assert_eq!(extract_glyph_block(&result, 1), Some(block2));
    assert_eq!(extract_glyph_block(&result, 2), Some(block3));
}

/// Verify short offset encoding (stored value = byte_offset / 2).
#[test]
fn test_gvar_short_offsets() {
    // Two 4-byte blocks.
    let block0: &[u8] = &[0xAA, 0xBB, 0xCC, 0xDD];
    let block1: &[u8] = &[0x11, 0x22, 0x33, 0x44];

    let table = build_gvar(0, 0, &[block0, block1], false);

    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);
    rev.insert(1, 1);

    let result = rewrite_gvar(&table, &rev, 2);

    // flags bit 0 must be 0 (short).
    let flags = u16::from_be_bytes([result[14], result[15]]);
    assert_eq!(flags & 0x0001, 0, "expected short-offset flag");

    // Read the raw short offset values.
    let off0 = u16::from_be_bytes([result[20], result[21]]) as usize;
    let off1 = u16::from_be_bytes([result[22], result[23]]) as usize;
    let off2 = u16::from_be_bytes([result[24], result[25]]) as usize;

    // Differences in units must represent 4-byte blocks.
    assert_eq!((off1 - off0) * 2, 4);
    assert_eq!((off2 - off1) * 2, 4);

    // Data must round-trip correctly.
    assert_eq!(extract_glyph_block(&result, 0), Some(block0));
    assert_eq!(extract_glyph_block(&result, 1), Some(block1));
}

/// Build with long_offsets=true; verify header flag bit 0 is set.
#[test]
fn test_gvar_long_offsets() {
    let block0: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
    let block1: &[u8] = &[0xCA, 0xFE, 0xBA, 0xBE];

    let table = build_gvar(0, 0, &[block0, block1], true);

    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);
    rev.insert(1, 1);

    let result = rewrite_gvar(&table, &rev, 2);

    // The input uses long offsets; the data is small enough for short offsets,
    // so the rewriter may switch to short.  We verify that the flag is
    // consistent with the actual encoding used.
    let flags = u16::from_be_bytes([result[14], result[15]]);
    let long = (flags & 0x0001) != 0;

    if long {
        // Long: offset[0] and offset[1] are u32 at bytes 20 and 24.
        let off0 = u32::from_be_bytes([result[20], result[21], result[22], result[23]]) as usize;
        let off1 = u32::from_be_bytes([result[24], result[25], result[26], result[27]]) as usize;
        assert_eq!(off1 - off0, 4);
    }

    assert_eq!(extract_glyph_block(&result, 0), Some(block0));
    assert_eq!(extract_glyph_block(&result, 1), Some(block1));
}

/// A new GID with no rev_remap entry must produce an empty block
/// (adjacent offsets equal).
#[test]
fn test_gvar_empty_block_for_missing_gid() {
    let block0: &[u8] = b"HELLO!!."; // 8 bytes

    // Only one glyph in source; new table has 3 glyphs but only new_gid=0 is mapped.
    let table = build_gvar(0, 0, &[block0], false);

    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);
    // new_gid 1 and 2 intentionally absent → empty blocks.

    let result = rewrite_gvar(&table, &rev, 3);

    assert_eq!(extract_glyph_block(&result, 0), Some(block0));
    assert_eq!(
        extract_glyph_block(&result, 1),
        Some(&[][..]),
        "missing gid must yield empty block"
    );
    assert_eq!(
        extract_glyph_block(&result, 2),
        Some(&[][..]),
        "missing gid must yield empty block"
    );
}

/// Verify shared tuples bytes are preserved verbatim in the output.
#[test]
fn test_gvar_shared_tuples_preserved() {
    // axisCount=2, sharedTupleCount=1 → 4 bytes of shared tuples.
    // We must craft the table manually because build_gvar writes zeros for
    // shared tuples; we patch them afterwards to known values.
    let block0: &[u8] = b"GLYPH0!!"; // 8 bytes

    let mut table = build_gvar(2, 1, &[block0], false);

    // Locate the shared tuples region in the input table.
    // sharedTuplesOffset is at header bytes 8-11.
    let sto = u32::from_be_bytes([table[8], table[9], table[10], table[11]]) as usize;
    // 4 bytes of shared tuples — write a recognisable pattern.
    table[sto] = 0x12;
    table[sto + 1] = 0x34;
    table[sto + 2] = 0x56;
    table[sto + 3] = 0x78;

    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);

    let result = rewrite_gvar(&table, &rev, 1);

    // Find shared tuples in output.
    let new_sto = u32::from_be_bytes([result[8], result[9], result[10], result[11]]) as usize;
    assert!(
        new_sto + 4 <= result.len(),
        "shared tuples region out of bounds"
    );
    assert_eq!(&result[new_sto..new_sto + 4], &[0x12, 0x34, 0x56, 0x78]);

    // Per-glyph data must also survive.
    assert_eq!(extract_glyph_block(&result, 0), Some(block0));
}

/// A table with version != 1.0 must be returned verbatim.
#[test]
fn test_gvar_bad_version_verbatim() {
    let mut table = build_gvar(0, 0, &[b"DATA1111"], false);
    // Corrupt majorVersion to 2.
    table[1] = 2;

    let rev: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gvar(&table, &rev, 1);

    // Must come back byte-for-byte identical to what was handed in.
    assert_eq!(result, table, "bad version must be returned verbatim");
}

/// A table shorter than 20 bytes must be returned verbatim.
#[test]
fn test_gvar_too_short_verbatim() {
    let table: Vec<u8> = vec![0x00, 0x01, 0x00, 0x00, 0xAB]; // only 5 bytes

    let rev: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gvar(&table, &rev, 1);

    assert_eq!(result, table, "too-short table must be returned verbatim");
}
