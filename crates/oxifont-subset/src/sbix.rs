/// sbix table subsetting: rebuild per-glyph bitmap data arrays for the new GID space.
///
/// sbix table format:
/// - Header (8 bytes): version(u16), flags(u16), numStrikes(u32).
/// - StrikeOffset array: numStrikes × u32 offsets relative to table start.
/// - Each Strike (at its offset):
///   - ppem(u16), ppi(u16)
///   - glyphDataOffsets[(numGlyphs + 1) × u32] — relative to strike start.
///     Adjacent pair [i], [i+1] gives the byte range for glyph i within the strike.
///   - Glyph data: originOffsetX(i16), originOffsetY(i16), graphicType(u32), pixel data.
///
/// `old_glyph_count` comes from maxp in the original font.
/// `new_glyph_count` is the number of glyphs after subsetting.
/// `rev_remap`: new_gid → old_gid mapping.
///
/// On any parse failure: returned verbatim.
use std::collections::HashMap;

// ─── parse helpers ────────────────────────────────────────────────────────────

#[inline]
fn r_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

#[inline]
fn r_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

// ─── public API ───────────────────────────────────────────────────────────────

/// Rewrite an sbix table to reflect the new GID space.
///
/// For each strike, glyph data blocks are reordered to match new GID numbering.
/// Glyphs not in `rev_remap` (no old GID) get empty blocks.
/// Glyphs whose old block was empty remain empty.
///
/// `old_glyph_count`: total glyphs in original font (from maxp).
/// `new_glyph_count`: total glyphs after subsetting.
/// `rev_remap`: maps new GID → old GID.
///
/// On any parse failure: returned verbatim.
pub fn rewrite_sbix(
    table: &[u8],
    rev_remap: &HashMap<u16, u16>,
    old_glyph_count: u16,
    new_glyph_count: u16,
) -> Vec<u8> {
    rewrite_sbix_inner(table, rev_remap, old_glyph_count, new_glyph_count)
        .unwrap_or_else(|| table.to_vec())
}

fn rewrite_sbix_inner(
    table: &[u8],
    rev_remap: &HashMap<u16, u16>,
    old_glyph_count: u16,
    new_glyph_count: u16,
) -> Option<Vec<u8>> {
    // Minimum header is 8 bytes.
    if table.len() < 8 {
        return None;
    }

    let version = r_u16(table, 0)?;
    let flags = r_u16(table, 2)?;
    let num_strikes = r_u32(table, 4)? as usize;

    // Strike offset array immediately follows the 8-byte header.
    let strike_offsets_start = 8usize;
    if strike_offsets_start + num_strikes * 4 > table.len() {
        return None;
    }

    // Read old strike absolute offsets.
    let mut old_strike_offsets: Vec<usize> = Vec::with_capacity(num_strikes);
    for i in 0..num_strikes {
        let off = r_u32(table, strike_offsets_start + i * 4)? as usize;
        if off > table.len() {
            return None;
        }
        old_strike_offsets.push(off);
    }

    // ─── Per-strike processing ────────────────────────────────────────────────

    // We'll collect rewritten strike blobs and then assemble the final table.
    let mut new_strike_blobs: Vec<Vec<u8>> = Vec::with_capacity(num_strikes);

    let old_gc = old_glyph_count as usize;
    let new_gc = new_glyph_count as usize;

    for &strike_start in &old_strike_offsets {
        // Each strike starts with ppem(u16) + ppi(u16) = 4 bytes,
        // followed by (old_glyph_count + 1) × u32 offset array.
        let strike_header_size = 4usize;
        let old_offset_array_size = (old_gc + 1) * 4;

        if strike_start + strike_header_size + old_offset_array_size > table.len() {
            return None;
        }

        let ppem = r_u16(table, strike_start)?;
        let ppi = r_u16(table, strike_start + 2)?;

        // Read old per-glyph offsets (relative to strike start).
        let old_offsets_base = strike_start + strike_header_size;
        let mut old_glyph_offsets: Vec<usize> = Vec::with_capacity(old_gc + 1);
        for i in 0..=old_gc {
            let rel = r_u32(table, old_offsets_base + i * 4)? as usize;
            old_glyph_offsets.push(rel);
        }

        // ─── Collect glyph data blocks in new GID order ───────────────────────

        // Blocks are slices into `table` (absolute offsets).
        let mut new_blocks: Vec<&[u8]> = Vec::with_capacity(new_gc);

        for new_gid in 0..new_glyph_count {
            let old_gid = match rev_remap.get(&new_gid) {
                Some(&g) => g as usize,
                None => {
                    new_blocks.push(&[]);
                    continue;
                }
            };

            if old_gid >= old_gc {
                // old_gid out of range for this font → empty block.
                new_blocks.push(&[]);
                continue;
            }

            let rel_start = old_glyph_offsets[old_gid];
            let rel_end = old_glyph_offsets[old_gid + 1];

            if rel_start == rel_end {
                // Empty block.
                new_blocks.push(&[]);
                continue;
            }

            // Offsets are relative to strike start.
            let abs_start = strike_start.checked_add(rel_start)?;
            let abs_end = strike_start.checked_add(rel_end)?;

            if abs_start > abs_end || abs_end > table.len() {
                return None;
            }

            new_blocks.push(&table[abs_start..abs_end]);
        }

        // ─── Build new strike blob ────────────────────────────────────────────
        //
        // Layout: ppem(u16) ppi(u16) offsets[(new_gc+1)×u32] glyph_data...
        // All offsets are relative to the strike blob start.

        let new_offset_array_size = (new_gc + 1) * 4;
        let new_data_start = strike_header_size + new_offset_array_size;
        let total_data: usize = new_blocks.iter().map(|b| b.len()).sum();
        let blob_size = new_data_start + total_data;

        let mut blob: Vec<u8> = Vec::with_capacity(blob_size);
        blob.extend_from_slice(&ppem.to_be_bytes());
        blob.extend_from_slice(&ppi.to_be_bytes());

        // Write per-glyph offsets (relative to strike blob start).
        let mut cursor = new_data_start as u32;
        for blk in &new_blocks {
            blob.extend_from_slice(&cursor.to_be_bytes());
            cursor = cursor.checked_add(blk.len() as u32)?;
        }
        // Sentinel offset.
        blob.extend_from_slice(&cursor.to_be_bytes());

        // Write glyph data.
        for blk in &new_blocks {
            blob.extend_from_slice(blk);
        }

        new_strike_blobs.push(blob);
    }

    // ─── Assemble output table ────────────────────────────────────────────────
    //
    // Layout:
    //   header (8 bytes)
    //   strike offsets (num_strikes × 4 bytes)
    //   strike blobs (packed sequentially)

    let strike_offsets_section = num_strikes * 4;
    // Each strike blob starts after the header + offset array.
    let first_strike_start = 8usize + strike_offsets_section;

    let mut new_strike_abs_offsets: Vec<u32> = Vec::with_capacity(num_strikes);
    let mut blob_cursor = first_strike_start as u32;
    for blob in &new_strike_blobs {
        new_strike_abs_offsets.push(blob_cursor);
        blob_cursor = blob_cursor.checked_add(blob.len() as u32)?;
    }

    let total_size = blob_cursor as usize;
    let mut out: Vec<u8> = Vec::with_capacity(total_size);

    // Header
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&(num_strikes as u32).to_be_bytes());

    // Strike offset array
    for &abs_off in &new_strike_abs_offsets {
        out.extend_from_slice(&abs_off.to_be_bytes());
    }

    // Strike blobs
    for blob in &new_strike_blobs {
        out.extend_from_slice(blob);
    }

    Some(out)
}
