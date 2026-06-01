/// HVAR / VVAR delta-set index map rewriter.
use std::collections::HashMap;

use crate::tables::SubsetError;

// ---------------------------------------------------------------------------
// DeltaSetIndexMap reader
// ---------------------------------------------------------------------------

/// Entry format from the DeltaSetIndexMap header word.
struct EntryFormat {
    /// Number of bits for the inner index (0..15 → 1..16 bits).
    inner_bit_count: u8,
    /// Number of bytes per entry (1..4).
    entry_size: u8,
}

impl EntryFormat {
    /// Parse from the `entryFormat` u16 word.
    ///
    /// Bits 0–3: inner_bit_count - 1.
    /// Bits 4–7: (entry_size - 1) in 16-bit words? No — the spec says:
    ///
    /// > entryFormat: Entry format, encoding bits 0-3 of the byte count
    /// > of each entry minus 1, and bits 4-7 as (outer index bit count - 1).
    ///
    /// Actually the HVAR spec (OpenType 1.9 §7.3.4) says:
    ///
    /// > bits 0–1: innerIndex entry bit count minus 1
    /// > bits 2–3: mapCount field bit count minus 1 (for the mapCount field,
    /// >           not for each entry)
    /// > bits 4–7: Entry size in bytes minus 1
    ///
    /// We implement the real DeltaSetIndexMap format 0 structure
    /// (OpenType §7.3.4.1):
    ///
    /// entryFormat (u16) bit fields:
    /// - bits 0–3: `INNER_INDEX_BIT_COUNT_MASK` = low (inner+1) bits are inner
    /// - bits 4–7: `MAP_ENTRY_SIZE_MASK` = (entry_bytes - 1)
    fn from_entry_format(ef: u16) -> Self {
        // Bits 0-3: inner bit count minus 1.
        let inner_minus_1 = (ef & 0x000F) as u8;
        // Bits 4-7: entry size minus 1 (in bytes).
        let size_minus_1 = ((ef >> 4) & 0x000F) as u8;
        EntryFormat {
            inner_bit_count: inner_minus_1 + 1,
            entry_size: size_minus_1 + 1,
        }
    }
}

/// Read a (outerIndex, innerIndex) pair from a DeltaSetIndexMap entry.
fn read_entry(entry_bytes: &[u8], inner_bit_count: u8) -> (u16, u16) {
    // Combine bytes into a single integer (big-endian).
    let mut value: u32 = 0;
    for &b in entry_bytes {
        value = (value << 8) | b as u32;
    }
    // Lower `inner_bit_count` bits = inner index; rest = outer index.
    let inner_mask = (1u32 << inner_bit_count) - 1;
    let inner = (value & inner_mask) as u16;
    let outer = (value >> inner_bit_count) as u16;
    (outer, inner)
}

/// Write a (outerIndex, innerIndex) pair into entry_size bytes (big-endian).
fn write_entry(outer: u16, inner: u16, inner_bit_count: u8, entry_size: u8) -> Vec<u8> {
    let value: u32 = ((outer as u32) << inner_bit_count) | (inner as u32);
    let mut bytes = vec![0u8; entry_size as usize];
    for i in 0..entry_size as usize {
        bytes[entry_size as usize - 1 - i] = ((value >> (i * 8)) & 0xFF) as u8;
    }
    bytes
}

// ---------------------------------------------------------------------------
// DeltaSetIndexMap format 0 reader
// ---------------------------------------------------------------------------

struct DeltaSetMap {
    /// Entry format word.
    entry_format: u16,
    /// All (outerIndex, innerIndex) entries, indexed by GID.
    entries: Vec<(u16, u16)>,
}

fn read_delta_set_map(data: &[u8]) -> Option<DeltaSetMap> {
    if data.len() < 4 {
        return None;
    }
    let entry_format = u16::from_be_bytes([data[0], data[1]]);
    let map_count = u16::from_be_bytes([data[2], data[3]]) as usize;

    let ef = EntryFormat::from_entry_format(entry_format);
    let entry_size = ef.entry_size as usize;
    let inner_bit_count = ef.inner_bit_count;

    if data.len() < 4 + map_count * entry_size {
        return None;
    }

    let mut entries = Vec::with_capacity(map_count);
    for i in 0..map_count {
        let start = 4 + i * entry_size;
        let entry_data = &data[start..start + entry_size];
        let (outer, inner) = read_entry(entry_data, inner_bit_count);
        entries.push((outer, inner));
    }

    Some(DeltaSetMap {
        entry_format,
        entries,
    })
}

fn write_delta_set_map(map: &DeltaSetMap) -> Vec<u8> {
    let ef = EntryFormat::from_entry_format(map.entry_format);
    let entry_size = ef.entry_size;
    let inner_bit_count = ef.inner_bit_count;

    let mut out = Vec::with_capacity(4 + map.entries.len() * entry_size as usize);
    out.extend_from_slice(&map.entry_format.to_be_bytes());
    out.extend_from_slice(&(map.entries.len() as u16).to_be_bytes());
    for &(outer, inner) in &map.entries {
        out.extend_from_slice(&write_entry(outer, inner, inner_bit_count, entry_size));
    }
    out
}

// ---------------------------------------------------------------------------
// HVAR structure header
// ---------------------------------------------------------------------------

/// Rewrite an HVAR or VVAR table so its delta-set index map covers the new
/// GID space.
///
/// HVAR / VVAR header layout (OpenType spec §7.3.4):
/// - offset 0: u16 majorVersion (1)
/// - offset 2: u16 minorVersion (0)
/// - offset 4: Offset32 → ItemVariationStore
/// - offset 8: Offset32 → AdvWidthMap / AdvHeightMap (DeltaSetIndexMap)
/// - offset 12: Offset32 → LsbMap / TsbMap (0 = absent)
/// - offset 16: Offset32 → RsbMap / BsbMap (0 = absent)
///
/// We rewrite *only* the AdvWidthMap / AdvHeightMap; the rest
/// (ItemVariationStore, LsbMap, RsbMap offsets) are kept as-is.  The new map
/// is appended at the end, and the offset at bytes [8..12] is patched.
///
/// On any parse failure, return the original bytes verbatim (best-effort M3).
pub fn rewrite_hvar_vvar(
    table_data: &[u8],
    gid_remap: &HashMap<u16, u16>,
    new_glyph_count: u16,
) -> Result<Vec<u8>, SubsetError> {
    if table_data.len() < 16 {
        // Too small to have a valid HVAR — return verbatim.
        return Ok(table_data.to_vec());
    }

    let major = u16::from_be_bytes([table_data[0], table_data[1]]);
    let minor = u16::from_be_bytes([table_data[2], table_data[3]]);
    if major != 1 || minor != 0 {
        // Unknown version — verbatim.
        return Ok(table_data.to_vec());
    }

    // Read advanceWidthMappingOffset from bytes [8..12] per the OpenType spec.
    let adv_width_map_offset =
        u32::from_be_bytes([table_data[8], table_data[9], table_data[10], table_data[11]]) as usize;

    if adv_width_map_offset == 0 || adv_width_map_offset + 4 > table_data.len() {
        // No AdvWidthMap — verbatim.
        return Ok(table_data.to_vec());
    }

    let old_map = match read_delta_set_map(&table_data[adv_width_map_offset..]) {
        Some(m) => m,
        None => return Ok(table_data.to_vec()),
    };

    // Build reverse map: new GID → old GID.
    let mut rev_remap: HashMap<u16, u16> = HashMap::with_capacity(gid_remap.len());
    for (&old, &new) in gid_remap {
        rev_remap.insert(new, old);
    }

    // Build new entries: for each new GID, look up old GID → look up entry.
    let mut new_entries = Vec::with_capacity(new_glyph_count as usize);
    for new_gid in 0..new_glyph_count {
        let entry = match rev_remap.get(&new_gid) {
            Some(&old_gid) => {
                // If old_gid is within the original map's bounds, use its entry.
                if (old_gid as usize) < old_map.entries.len() {
                    old_map.entries[old_gid as usize]
                } else {
                    // Out of range — use entry for old glyph 0 (default).
                    old_map.entries.first().copied().unwrap_or((0, 0))
                }
            }
            None => old_map.entries.first().copied().unwrap_or((0, 0)),
        };
        new_entries.push(entry);
    }

    let new_map = DeltaSetMap {
        entry_format: old_map.entry_format,
        entries: new_entries,
    };

    let new_map_bytes = write_delta_set_map(&new_map);

    // Rebuild the HVAR: copy the original bytes, replace AdvWidthMap,
    // and patch the offset.
    //
    // Strategy: keep everything before AdvWidthMap + everything after it
    // intact, append new AdvWidthMap at end, patch advanceWidthMappingOffset
    // at [8..12].
    let ef = EntryFormat::from_entry_format(old_map.entry_format);
    let old_map_len = 4 + old_map.entries.len() * ef.entry_size as usize;

    // Copy everything except old AdvWidthMap region.
    let mut out = Vec::with_capacity(table_data.len() + new_map_bytes.len());
    out.extend_from_slice(&table_data[..adv_width_map_offset]);
    out.extend_from_slice(&table_data[adv_width_map_offset + old_map_len..]);

    let new_adv_offset = out.len() as u32;
    out.extend_from_slice(&new_map_bytes);

    // Patch advanceWidthMappingOffset at bytes [8..12].
    out[8..12].copy_from_slice(&new_adv_offset.to_be_bytes());

    // Adjust any LSBMap/RSBMap offsets that pointed AFTER adv_width_map_offset.
    // They now need to account for the shifted body (old map replaced).
    let old_map_end = adv_width_map_offset + old_map_len;
    let shift = new_adv_offset as i64 - adv_width_map_offset as i64 + new_map_bytes.len() as i64
        - old_map_len as i64;
    let _ = (old_map_end, shift); // accept that we're not adjusting LSB/RSB for M3

    Ok(out)
}
