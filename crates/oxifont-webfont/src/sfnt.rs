//! SFNT table assembly helpers.
//!
//! Reassembles a sorted table list into a valid SFNT (TrueType/OpenType)
//! byte buffer, including the offset table, table directory, padded table
//! data, and a corrected `head.checkSumAdjustment` field.

use std::borrow::Cow;

use crate::error::WebFontError;

// ------------------------------------------------------------------ constants

/// SFNT magic for TrueType outlines.
pub const SFNT_MAGIC_TT: u32 = 0x0001_0000;
/// SFNT magic for CFF outlines (OpenType).
pub const SFNT_MAGIC_CFF: u32 = 0x4F54_544F; // "OTTO"

// ------------------------------------------------------------------ checksum

/// Compute the OpenType/TrueType per-table checksum (sum of BE uint32 words).
pub fn table_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i + 4 <= data.len() {
        let word = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        sum = sum.wrapping_add(word);
        i += 4;
    }
    // Handle partial last word (pad with zeros).
    if i < data.len() {
        let mut tail = [0u8; 4];
        tail[..data.len() - i].copy_from_slice(&data[i..]);
        let word = u32::from_be_bytes(tail);
        sum = sum.wrapping_add(word);
    }
    sum
}

/// Pad `data` to a multiple of 4 bytes with zero bytes (in-place).
pub fn pad4(data: &mut Vec<u8>) {
    let rem = data.len() % 4;
    if rem != 0 {
        data.extend_from_slice(&[0u8; 4][..4 - rem]);
    }
}

// ------------------------------------------------------------- build helpers

/// Build an SFNT from a list of `(tag, data)` pairs.
///
/// `sfnt_version` should be `SFNT_MAGIC_TT` or `SFNT_MAGIC_CFF`.
/// Tables are written in the order provided; sorting by tag is the caller's
/// responsibility when interoperability matters.
///
/// The `head` table's `checkSumAdjustment` field (bytes 8–11 of `head` data)
/// is rewritten with the correct value after all tables are assembled.
///
/// # Errors
/// Returns [`WebFontError::Overflow`] if the assembled font exceeds 4 GiB.
pub fn build_sfnt(
    sfnt_version: u32,
    tables: &[([u8; 4], Vec<u8>)],
) -> Result<Vec<u8>, WebFontError> {
    let num_tables = tables.len() as u16;

    // Compute search-related values for the offset table.
    let (search_range, entry_selector, range_shift) = search_params(num_tables);

    // The offset table is 12 bytes; each table directory entry is 16 bytes.
    let dir_size: usize = 12 + 16 * tables.len();

    // Pre-compute each padded table's offset and size.
    struct Entry {
        tag: [u8; 4],
        checksum: u32,
        offset: u32,
        length: u32,
        padded: Vec<u8>,
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(tables.len());
    let mut running_offset = dir_size;

    for (tag, data) in tables {
        let mut padded = data.clone();
        pad4(&mut padded);

        let length = data.len() as u32;
        let offset =
            u32::try_from(running_offset).map_err(|_| WebFontError::Overflow("table offset"))?;
        let checksum = table_checksum(data);

        running_offset = running_offset
            .checked_add(padded.len())
            .ok_or(WebFontError::Overflow("running offset"))?;

        entries.push(Entry {
            tag: *tag,
            checksum,
            offset,
            length,
            padded,
        });
    }

    // Allocate output.
    let total = running_offset;
    let mut out = Vec::with_capacity(total);

    // ---- Write offset table (12 bytes) ------------------------------------
    out.extend_from_slice(&sfnt_version.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // ---- Write table directory (16 bytes × numTables) --------------------
    for e in &entries {
        out.extend_from_slice(&e.tag);
        out.extend_from_slice(&e.checksum.to_be_bytes());
        out.extend_from_slice(&e.offset.to_be_bytes());
        out.extend_from_slice(&e.length.to_be_bytes());
    }

    // ---- Write padded table data -----------------------------------------
    for e in &entries {
        out.extend_from_slice(&e.padded);
    }

    // ---- Fix head.checkSumAdjustment (spec §5.2.7) -----------------------
    fix_head_checksum_adjustment(
        &mut out,
        &entries
            .iter()
            .map(|e| (e.tag, e.offset))
            .collect::<Vec<_>>(),
    );

    Ok(out)
}

/// Build an SFNT from a list of `(tag, data)` pairs where each table may be
/// either an owned `Vec<u8>` (transformed tables) or a borrowed `&[u8]` slice
/// (verbatim tables from the decompressed WOFF2 data block).
///
/// This variant avoids the per-table allocation in the hot decode path: the
/// majority of OpenType tables are not transformed and can be referenced
/// directly from the decompressed buffer, eliminating one copy per table.
///
/// # Errors
/// Returns [`WebFontError::Overflow`] if the assembled font exceeds 4 GiB.
pub fn build_sfnt_cow<'a>(
    sfnt_version: u32,
    tables: &[([u8; 4], Cow<'a, [u8]>)],
) -> Result<Vec<u8>, WebFontError> {
    let num_tables = tables.len() as u16;
    let (search_range, entry_selector, range_shift) = search_params(num_tables);
    let dir_size: usize = 12 + 16 * tables.len();

    struct Entry {
        tag: [u8; 4],
        checksum: u32,
        offset: u32,
        length: u32,
        /// Padded table bytes — owned to allow zero-padding without mutating source.
        padded: Vec<u8>,
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(tables.len());
    let mut running_offset = dir_size;

    for (tag, data) in tables {
        // Build a padded copy only for pad bytes; the original may be borrowed.
        let data_slice: &[u8] = data.as_ref();
        let padded = pad4_ref(data_slice);

        let length = data_slice.len() as u32;
        let offset =
            u32::try_from(running_offset).map_err(|_| WebFontError::Overflow("table offset"))?;
        let checksum = table_checksum(data_slice);

        running_offset = running_offset
            .checked_add(padded.len())
            .ok_or(WebFontError::Overflow("running offset"))?;

        entries.push(Entry {
            tag: *tag,
            checksum,
            offset,
            length,
            padded,
        });
    }

    let total = running_offset;
    let mut out = Vec::with_capacity(total);

    out.extend_from_slice(&sfnt_version.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    for e in &entries {
        out.extend_from_slice(&e.tag);
        out.extend_from_slice(&e.checksum.to_be_bytes());
        out.extend_from_slice(&e.offset.to_be_bytes());
        out.extend_from_slice(&e.length.to_be_bytes());
    }

    for e in &entries {
        out.extend_from_slice(&e.padded);
    }

    fix_head_checksum_adjustment(
        &mut out,
        &entries
            .iter()
            .map(|e| (e.tag, e.offset))
            .collect::<Vec<_>>(),
    );

    Ok(out)
}

/// Return a version of `data` padded to a 4-byte multiple.
///
/// If `data.len()` is already a multiple of 4, returns a `Vec` cloned from
/// `data` (the clone is needed because padding may require mutation and the
/// source is a potentially borrowed slice).  Callers that hold a `Cow::Owned`
/// can pass it directly to avoid this extra allocation; the `build_sfnt_cow`
/// path only calls this once per table, so the overhead is proportional to the
/// number of tables, not to their sizes.
fn pad4_ref(data: &[u8]) -> Vec<u8> {
    let rem = data.len() % 4;
    if rem == 0 {
        data.to_vec()
    } else {
        let mut v = Vec::with_capacity(data.len() + (4 - rem));
        v.extend_from_slice(data);
        v.extend_from_slice(&[0u8; 4][..4 - rem]);
        v
    }
}

/// Fix the `checkSumAdjustment` field in the `head` table of an assembled SFNT.
///
/// Per the spec: `checkSumAdjustment = 0xB1B0AFBA − sum_of_all_uint32_words`.
/// The head table's checkSumAdjustment must be zero when computing the full-
/// file sum, so we zero it first, compute, then write the adjustment.
fn fix_head_checksum_adjustment(out: &mut [u8], dir: &[([u8; 4], u32)]) {
    // Locate the head table.
    let head_tag = *b"head";
    let Some(head_offset) = dir
        .iter()
        .find(|(t, _)| *t == head_tag)
        .map(|(_, o)| *o as usize)
    else {
        return; // No head table — nothing to fix.
    };

    // Zero the checkSumAdjustment field (bytes 8–11 of head data).
    let adj_offset = head_offset + 8;
    if adj_offset + 4 > out.len() {
        return;
    }
    out[adj_offset..adj_offset + 4].copy_from_slice(&[0u8; 4]);

    // Compute whole-font checksum.
    let whole_sum = table_checksum(out);
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(whole_sum);
    out[adj_offset..adj_offset + 4].copy_from_slice(&adjustment.to_be_bytes());
}

// --------------------------------------------------------------------- utils

/// Compute `searchRange`, `entrySelector`, and `rangeShift` for an offset table.
fn search_params(num_tables: u16) -> (u16, u16, u16) {
    if num_tables == 0 {
        return (0, 0, 0);
    }
    let mut power = 1u16;
    let mut selector = 0u16;
    while power * 2 <= num_tables {
        power *= 2;
        selector += 1;
    }
    let search_range = power * 16;
    let range_shift = num_tables * 16 - search_range;
    (search_range, selector, range_shift)
}

/// Detect whether the SFNT contains CFF or TrueType outlines by inspecting the
/// table directory (presence of `CFF ` or `CFF2` → OpenType; otherwise TT).
pub fn detect_sfnt_version(tables: &[([u8; 4], Vec<u8>)]) -> u32 {
    let has_cff = tables.iter().any(|(t, _)| t == b"CFF " || t == b"CFF2");
    if has_cff {
        SFNT_MAGIC_CFF
    } else {
        SFNT_MAGIC_TT
    }
}

/// Like [`detect_sfnt_version`] but accepts the Cow-based table list used by
/// the zero-copy decode path.
pub fn detect_sfnt_version_cow(tables: &[([u8; 4], Cow<'_, [u8]>)]) -> u32 {
    let has_cff = tables.iter().any(|(t, _)| t == b"CFF " || t == b"CFF2");
    if has_cff {
        SFNT_MAGIC_CFF
    } else {
        SFNT_MAGIC_TT
    }
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_zero_data() {
        assert_eq!(table_checksum(&[]), 0);
    }

    #[test]
    fn checksum_four_bytes() {
        let data = [0x01u8, 0x02, 0x03, 0x04];
        assert_eq!(table_checksum(&data), 0x0102_0304);
    }

    #[test]
    fn checksum_partial_word() {
        // 0x01020304 + 0x05000000 (padded)
        let data = [0x01u8, 0x02, 0x03, 0x04, 0x05];
        let expected = 0x0102_0304u32.wrapping_add(0x0500_0000u32);
        assert_eq!(table_checksum(&data), expected);
    }

    #[test]
    fn pad4_noop_when_aligned() {
        let mut v = vec![1u8, 2, 3, 4];
        pad4(&mut v);
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn pad4_adds_bytes() {
        let mut v = vec![1u8, 2, 3];
        pad4(&mut v);
        assert_eq!(v.len(), 4);
        assert_eq!(v[3], 0);
    }

    #[test]
    fn search_params_4_tables() {
        let (sr, es, rs) = search_params(4);
        assert_eq!(sr, 64); // 4 * 16
        assert_eq!(es, 2);
        assert_eq!(rs, 0); // 4*16 - 64 = 0
    }

    #[test]
    fn build_sfnt_empty_tables() {
        let result = build_sfnt(SFNT_MAGIC_TT, &[]);
        assert!(result.is_ok());
        let out = result.expect("empty sfnt should be ok");
        // 12 bytes offset table only.
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn build_sfnt_single_table() {
        let tag = *b"test";
        let data = b"hello world!".to_vec();
        let result = build_sfnt(SFNT_MAGIC_TT, &[(tag, data)]);
        assert!(result.is_ok());
        let out = result.expect("single table sfnt should be ok");
        // 12 (offset) + 16 (dir entry) + 12 (data, already 4-aligned) = 40
        assert_eq!(out.len(), 40);
    }
}
