//! `ItemVariationStore` evaluation, for the `fvar`-without-`gvar` carve-out.
//!
//! A face may vary only its metrics, through `HVAR`, with no `gvar` at all. That
//! is the single case in which the instancer *reads* `HVAR` rather than deleting
//! it unread: there are no phantom points to derive advances from. Everywhere
//! else the phantom points are authoritative, because they and `HVAR` are
//! independently authored and are allowed to disagree.

use crate::tables::{get_i16, get_u16};
use crate::varfont::{read_delta_set_map, read_entry, EntryFormat};

/// One axis's `(startCoord, peakCoord, endCoord)` in F2Dot14.
type RegionAxis = (i16, i16, i16);

/// A parsed `ItemVariationStore`.
pub(crate) struct ItemVariationStore {
    regions: Vec<Vec<RegionAxis>>,
    subtables: Vec<ItemVariationData>,
}

struct ItemVariationData {
    region_indices: Vec<u16>,
    /// `itemCount` rows of `regionIndexCount` deltas.
    rows: Vec<Vec<i32>>,
}

impl ItemVariationStore {
    /// Parse an `ItemVariationStore` at the start of `data`.
    ///
    /// Returns `None` for any structural problem; the caller then treats the
    /// metric deltas as zero rather than failing the whole instance.
    pub(crate) fn parse(data: &[u8]) -> Option<Self> {
        let format = get_u16(data, 0)?;
        if format != 1 {
            return None;
        }
        let region_list_offset = read_u32(data, 2)? as usize;
        let data_count = usize::from(get_u16(data, 6)?);

        let axis_count = usize::from(get_u16(data, region_list_offset)?);
        let region_count = usize::from(get_u16(data, region_list_offset + 2)?);
        let region_bytes = region_count
            .checked_mul(axis_count)?
            .checked_mul(6)?
            .checked_add(region_list_offset + 4)?;
        if region_bytes > data.len() {
            return None;
        }
        let mut regions = Vec::with_capacity(region_count);
        for r in 0..region_count {
            let base = region_list_offset + 4 + r * axis_count * 6;
            let mut axes = Vec::with_capacity(axis_count);
            for a in 0..axis_count {
                let o = base + a * 6;
                axes.push((
                    get_i16(data, o)?,
                    get_i16(data, o + 2)?,
                    get_i16(data, o + 4)?,
                ));
            }
            regions.push(axes);
        }

        let mut subtables = Vec::with_capacity(data_count.min(4096));
        for i in 0..data_count {
            let offset = read_u32(data, 8 + i * 4)? as usize;
            subtables.push(parse_item_variation_data(data, offset)?);
        }

        Some(ItemVariationStore { regions, subtables })
    }

    /// The delta for delta-set `(outer, inner)` at `coords`.
    pub(crate) fn delta(&self, outer: u16, inner: u16, coords: &[i16]) -> f64 {
        let Some(sub) = self.subtables.get(usize::from(outer)) else {
            return 0.0;
        };
        let Some(row) = sub.rows.get(usize::from(inner)) else {
            return 0.0;
        };
        let mut total = 0.0f64;
        for (j, &value) in row.iter().enumerate() {
            let Some(&region_index) = sub.region_indices.get(j) else {
                continue;
            };
            let Some(region) = self.regions.get(usize::from(region_index)) else {
                continue;
            };
            let scalar = region_scalar(region, coords);
            if scalar != 0.0 {
                total += scalar * f64::from(value);
            }
        }
        total
    }
}

fn parse_item_variation_data(data: &[u8], offset: usize) -> Option<ItemVariationData> {
    let item_count = usize::from(get_u16(data, offset)?);
    let word_delta_count = get_u16(data, offset + 2)?;
    let region_index_count = usize::from(get_u16(data, offset + 4)?);

    let long_words = word_delta_count & 0x8000 != 0;
    let word_count = usize::from(word_delta_count & 0x7FFF);
    if word_count > region_index_count {
        return None;
    }

    let mut region_indices = Vec::with_capacity(region_index_count);
    for i in 0..region_index_count {
        region_indices.push(get_u16(data, offset + 6 + i * 2)?);
    }

    let long_size = if long_words { 4 } else { 2 };
    let short_size = if long_words { 2 } else { 1 };
    let row_size = word_count
        .checked_mul(long_size)?
        .checked_add((region_index_count - word_count).checked_mul(short_size)?)?;

    let rows_start = offset.checked_add(6 + region_index_count * 2)?;
    let rows_bytes = item_count.checked_mul(row_size)?;
    if rows_start.checked_add(rows_bytes)? > data.len() {
        return None;
    }

    let mut rows = Vec::with_capacity(item_count);
    for r in 0..item_count {
        let base = rows_start + r * row_size;
        let mut values = Vec::with_capacity(region_index_count);
        let mut pos = base;
        for j in 0..region_index_count {
            let v = if j < word_count {
                if long_words {
                    read_i32(data, pos)?
                } else {
                    i32::from(get_i16(data, pos)?)
                }
            } else if long_words {
                i32::from(get_i16(data, pos)?)
            } else {
                i32::from(*data.get(pos)? as i8)
            };
            pos += if j < word_count {
                long_size
            } else {
                short_size
            };
            values.push(v);
        }
        rows.push(values);
    }

    Some(ItemVariationData {
        region_indices,
        rows,
    })
}

/// The tent scalar of one variation region, evaluated at `coords`.
fn region_scalar(region: &[RegionAxis], coords: &[i16]) -> f64 {
    let mut s = 1.0f64;
    for (a, &(start, peak, end)) in region.iter().enumerate() {
        let p = f64::from(peak) / 16384.0;
        if p == 0.0 {
            continue;
        }
        let c = f64::from(coords.get(a).copied().unwrap_or(0)) / 16384.0;
        if c == p {
            continue;
        }
        let lo = f64::from(start) / 16384.0;
        let hi = f64::from(end) / 16384.0;
        if lo > p || p > hi {
            continue;
        }
        if lo < 0.0 && hi > 0.0 {
            continue;
        }
        if c <= lo || c >= hi {
            return 0.0;
        }
        s *= if c < p {
            (c - lo) / (p - lo)
        } else {
            (hi - c) / (hi - p)
        };
    }
    s
}

/// A `HVAR` / `VVAR` table, parsed for advance-delta lookup.
pub(crate) struct AdvanceVariations {
    store: ItemVariationStore,
    /// `(outerIndex, innerIndex)` per glyph; empty means the implicit mapping
    /// (`outer = 0`, `inner = gid`).
    advance_map: Vec<(u16, u16)>,
}

impl AdvanceVariations {
    /// Parse `HVAR` or `VVAR`.
    ///
    /// Returns `None` for any structural problem, in which case the caller
    /// treats every advance delta as zero.
    pub(crate) fn parse(table: &[u8]) -> Option<Self> {
        let major = get_u16(table, 0)?;
        if major != 1 {
            return None;
        }
        let store_offset = read_u32(table, 4)? as usize;
        let advance_map_offset = read_u32(table, 8)? as usize;
        let store = ItemVariationStore::parse(table.get(store_offset..)?)?;

        let advance_map = if advance_map_offset == 0 {
            Vec::new()
        } else {
            parse_index_map(table.get(advance_map_offset..)?).unwrap_or_default()
        };
        Some(AdvanceVariations { store, advance_map })
    }

    /// The advance delta for `gid` at `coords`.
    pub(crate) fn advance_delta(&self, gid: u16, coords: &[i16]) -> f64 {
        let (outer, inner) = if self.advance_map.is_empty() {
            (0u16, gid)
        } else {
            let idx = usize::from(gid).min(self.advance_map.len() - 1);
            self.advance_map[idx]
        };
        self.store.delta(outer, inner, coords)
    }
}

/// Read a `DeltaSetIndexMap` in either format.
///
/// Format 0's header is `uint16 entryFormat` + `uint16 mapCount`, which
/// [`read_delta_set_map`] already decodes; format 1 widens `mapCount` to
/// `uint32` and is decoded here.
fn parse_index_map(data: &[u8]) -> Option<Vec<(u16, u16)>> {
    let format = *data.first()?;
    if format == 0 {
        return read_delta_set_map(data).map(|m| m.entries);
    }
    if format != 1 {
        return None;
    }
    let entry_format = u16::from(*data.get(1)?);
    let map_count = read_u32(data, 2)? as usize;
    let ef = EntryFormat::from_entry_format(entry_format);
    let entry_size = usize::from(ef.entry_size);
    let needed = map_count.checked_mul(entry_size)?.checked_add(6)?;
    if needed > data.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(map_count);
    for i in 0..map_count {
        let start = 6 + i * entry_size;
        let bytes = data.get(start..start + entry_size)?;
        entries.push(read_entry(bytes, ef.inner_bit_count));
    }
    Some(entries)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    data.get(offset..end)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    read_u32(data, offset).map(|v| v as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One region spanning `0 → 1` on a single axis, one delta set of one row.
    fn minimal_store(delta: i16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&1u16.to_be_bytes()); // format
        out.extend_from_slice(&16u32.to_be_bytes()); // regionListOffset
        out.extend_from_slice(&1u16.to_be_bytes()); // itemVariationDataCount
        out.extend_from_slice(&28u32.to_be_bytes()); // offset[0]
                                                     // 12 bytes so far; pad to the region list at 16.
        while out.len() < 16 {
            out.push(0);
        }
        out.extend_from_slice(&1u16.to_be_bytes()); // axisCount
        out.extend_from_slice(&1u16.to_be_bytes()); // regionCount
        out.extend_from_slice(&0i16.to_be_bytes()); // start
        out.extend_from_slice(&16384i16.to_be_bytes()); // peak
        out.extend_from_slice(&16384i16.to_be_bytes()); // end
                                                        // ItemVariationData at 28.
        while out.len() < 28 {
            out.push(0);
        }
        out.extend_from_slice(&1u16.to_be_bytes()); // itemCount
        out.extend_from_slice(&1u16.to_be_bytes()); // wordDeltaCount (1 word)
        out.extend_from_slice(&1u16.to_be_bytes()); // regionIndexCount
        out.extend_from_slice(&0u16.to_be_bytes()); // regionIndexes[0]
        out.extend_from_slice(&delta.to_be_bytes());
        out
    }

    #[test]
    fn store_interpolates_along_the_region() {
        let data = minimal_store(100);
        let store = ItemVariationStore::parse(&data).expect("store");
        assert_eq!(store.delta(0, 0, &[16384]), 100.0);
        assert_eq!(store.delta(0, 0, &[8192]), 50.0);
        assert_eq!(store.delta(0, 0, &[0]), 0.0);
        // Out-of-range indices are zero, never a panic.
        assert_eq!(store.delta(9, 0, &[16384]), 0.0);
        assert_eq!(store.delta(0, 9, &[16384]), 0.0);
    }

    #[test]
    fn truncating_the_store_never_panics() {
        let data = minimal_store(100);
        for cut in 0..data.len() {
            let _ = ItemVariationStore::parse(&data[..cut]);
        }
    }
}
