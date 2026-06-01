//! Tests for HVAR / VVAR delta-set index map rewriting.
//!
//! These tests use synthetic HVAR tables constructed byte-by-byte to verify
//! that `rewrite_hvar_vvar` correctly reads `advanceWidthMappingOffset` from
//! bytes [8..12] (per the OpenType spec) and patches that same field — not the
//! `itemVariationStoreOffset` at bytes [4..8].

use std::collections::HashMap;

use oxifont_subset::varfont::rewrite_hvar_vvar;

/// Build a minimal synthetic HVAR table with:
///
/// ```text
/// bytes 0-1:   majorVersion = 1  (u16 BE)
/// bytes 2-3:   minorVersion = 0  (u16 BE)
/// bytes 4-7:   itemVariationStoreOffset = 0x00000064  (u32 BE)
/// bytes 8-11:  advanceWidthMappingOffset = 20  (u32 BE) → points to DeltaSetIndexMap
/// bytes 12-15: lsbMappingOffset = 0  (absent, u32 BE)
/// bytes 16-19: rsbMappingOffset = 0  (absent, u32 BE)
/// bytes 20-21: DeltaSetIndexMap.entryFormat = 0x0001  (entry_size=1 byte, inner_bits=2)
/// bytes 22-23: DeltaSetIndexMap.mapCount   = 3
/// bytes 24:    entry GID 0 = 0x00
/// bytes 25:    entry GID 1 = 0x01
/// bytes 26:    entry GID 2 = 0x02
/// ```
fn synthetic_hvar() -> Vec<u8> {
    let mut table = Vec::with_capacity(27);

    // Header
    table.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    table.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    table.extend_from_slice(&0x00000064u32.to_be_bytes()); // itemVariationStoreOffset
    table.extend_from_slice(&20u32.to_be_bytes()); // advanceWidthMappingOffset = 20
    table.extend_from_slice(&0u32.to_be_bytes()); // lsbMappingOffset = absent
    table.extend_from_slice(&0u32.to_be_bytes()); // rsbMappingOffset = absent

    // DeltaSetIndexMap at offset 20
    // entryFormat: bits 0-3 = inner_bit_count - 1 = 1 (inner=2 bits)
    //              bits 4-7 = entry_size - 1 = 0 (1 byte per entry)
    // So entryFormat = 0x0001
    table.extend_from_slice(&0x0001u16.to_be_bytes()); // entryFormat
    table.extend_from_slice(&3u16.to_be_bytes()); // mapCount = 3
    table.push(0x00); // GID 0 entry
    table.push(0x01); // GID 1 entry
    table.push(0x02); // GID 2 entry

    table
}

/// Verify that:
/// 1. `result[4..8]` (itemVariationStoreOffset) is unchanged (= 0x00000064).
/// 2. `result[8..12]` (advanceWidthMappingOffset) is updated to point to the
///    new AdvWidthMap that was appended.
/// 3. The new AdvWidthMap at that offset contains exactly 2 entries (GIDs 0 and 1
///    in the new space, remapped from old GIDs 0 and 2).
#[test]
fn hvar_rewrite_patches_correct_offset_field() {
    let table = synthetic_hvar();

    // Keep GID 0 → new GID 0, GID 2 → new GID 1.  Drop GID 1.
    let mut gid_remap: HashMap<u16, u16> = HashMap::new();
    gid_remap.insert(0, 0);
    gid_remap.insert(2, 1);

    let result = rewrite_hvar_vvar(&table, &gid_remap, 2).expect("rewrite_hvar_vvar must succeed");

    // --- Check 1: itemVariationStoreOffset at bytes [4..8] must be untouched ---
    let ivs_offset = u32::from_be_bytes(result[4..8].try_into().unwrap());
    assert_eq!(
        ivs_offset, 0x0000_0064,
        "itemVariationStoreOffset at bytes [4..8] must not be overwritten; got {ivs_offset:#010x}"
    );

    // --- Check 2: advanceWidthMappingOffset at bytes [8..12] must point into the output ---
    let adv_offset = u32::from_be_bytes(result[8..12].try_into().unwrap()) as usize;
    assert!(
        adv_offset + 4 <= result.len(),
        "advanceWidthMappingOffset ({adv_offset}) must be within the output (len={})",
        result.len()
    );

    // --- Check 3: the DeltaSetIndexMap at that offset must have 2 entries ---
    let map_data = &result[adv_offset..];
    assert!(
        map_data.len() >= 4,
        "AdvWidthMap at new offset must have at least 4 bytes"
    );
    let map_count = u16::from_be_bytes([map_data[2], map_data[3]]);
    assert_eq!(
        map_count, 2,
        "New AdvWidthMap must contain exactly 2 entries (one per new GID), got {map_count}"
    );
}

/// Verify that a table too small to be valid HVAR is returned verbatim.
#[test]
fn hvar_rewrite_too_small_returns_verbatim() {
    let tiny = vec![0u8; 8];
    let gid_remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_hvar_vvar(&tiny, &gid_remap, 1).expect("must not error");
    assert_eq!(result, tiny, "too-small table must be returned verbatim");
}

/// Verify that an unknown-version table is returned verbatim.
#[test]
fn hvar_rewrite_unknown_version_returns_verbatim() {
    let mut table = synthetic_hvar();
    // Overwrite majorVersion with 2 (unknown).
    table[0] = 0;
    table[1] = 2;
    let gid_remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_hvar_vvar(&table, &gid_remap, 1).expect("must not error");
    assert_eq!(
        result, table,
        "unknown-version table must be returned verbatim"
    );
}
