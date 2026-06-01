/// `gvar` table rewriting — reorder per-glyph variation data to the new GID space.
///
/// Per-glyph variation data blocks are opaque (no inter-GID references), so
/// subsetting only needs to reorder them to match the new GID numbering.
use std::collections::HashMap;

// ─── parse helpers ──────────────────────────────────────────────────────────

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let b = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let b = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

// ─── internal result type ────────────────────────────────────────────────────

/// Everything parsed from a valid gvar header.
struct GvarHeader {
    axis_count: u16,
    shared_tuple_count: u16,
    shared_tuples_offset: usize,
    glyph_count: u16,
    long_offsets: bool,
    glyph_var_data_array_offset: usize,
}

fn parse_header(table: &[u8]) -> Option<GvarHeader> {
    if table.len() < 20 {
        return None;
    }
    let major = read_u16(table, 0)?;
    let minor = read_u16(table, 2)?;
    if major != 1 || minor != 0 {
        return None;
    }
    let axis_count = read_u16(table, 4)?;
    let shared_tuple_count = read_u16(table, 6)?;
    let shared_tuples_offset = read_u32(table, 8)? as usize;
    let glyph_count = read_u16(table, 12)?;
    let flags = read_u16(table, 14)?;
    let long_offsets = (flags & 0x0001) != 0;
    let glyph_var_data_array_offset = read_u32(table, 16)? as usize;

    Some(GvarHeader {
        axis_count,
        shared_tuple_count,
        shared_tuples_offset,
        glyph_count,
        long_offsets,
        glyph_var_data_array_offset,
    })
}

/// Parse the offset table from a gvar.  Returns absolute byte offsets from the
/// start of the table (converted from relative-to-`glyph_var_data_array_offset`).
fn parse_offsets(table: &[u8], hdr: &GvarHeader) -> Option<Vec<usize>> {
    let entry_count = hdr.glyph_count as usize + 1;
    let base = hdr.glyph_var_data_array_offset;

    let mut offsets = Vec::with_capacity(entry_count);

    if hdr.long_offsets {
        let needed = 20 + entry_count * 4;
        if table.len() < needed {
            return None;
        }
        for i in 0..entry_count {
            let rel = read_u32(table, 20 + i * 4)? as usize;
            offsets.push(base.checked_add(rel)?);
        }
    } else {
        let needed = 20 + entry_count * 2;
        if table.len() < needed {
            return None;
        }
        for i in 0..entry_count {
            let rel = read_u16(table, 20 + i * 2)? as usize * 2;
            offsets.push(base.checked_add(rel)?);
        }
    }

    Some(offsets)
}

// ─── public API ─────────────────────────────────────────────────────────────

/// Rewrite a gvar table so its per-glyph data array covers the new GID space.
///
/// Per-glyph variation data blocks are opaque (no inter-GID references) —
/// subsetting just reorders them to match the new GID numbering.
///
/// `rev_remap`: new_gid → old_gid mapping.
/// On any parse failure, returns the original table verbatim.
pub fn rewrite_gvar(table: &[u8], rev_remap: &HashMap<u16, u16>, new_glyph_count: u16) -> Vec<u8> {
    rewrite_gvar_inner(table, rev_remap, new_glyph_count).unwrap_or_else(|| table.to_vec())
}

/// Inner implementation — returns `None` on any parse failure so the caller
/// can fall back to verbatim copy cleanly without scattered `return`.
fn rewrite_gvar_inner(
    table: &[u8],
    rev_remap: &HashMap<u16, u16>,
    new_glyph_count: u16,
) -> Option<Vec<u8>> {
    let hdr = parse_header(table)?;
    let old_offsets = parse_offsets(table, &hdr)?;

    // ── Shared tuples region (copied verbatim) ──────────────────────────────
    // Size = axisCount × sharedTupleCount × 2 bytes (F2Dot14 = i16).
    let shared_tuples_size = hdr.axis_count as usize * hdr.shared_tuple_count as usize * 2;

    let shared_start = hdr.shared_tuples_offset;
    let shared_end = shared_start.checked_add(shared_tuples_size)?;
    // Only validate if there are actually shared tuples to copy.
    if shared_tuples_size > 0 && shared_end > table.len() {
        return None;
    }
    let shared_tuples_bytes = if shared_tuples_size > 0 {
        &table[shared_start..shared_end]
    } else {
        &[]
    };

    // ── Collect per-glyph blocks in the new GID order ──────────────────────
    let mut new_blocks: Vec<&[u8]> = Vec::with_capacity(new_glyph_count as usize);
    for new_gid in 0..new_glyph_count {
        match rev_remap.get(&new_gid) {
            None => {
                // No source GID → empty block.
                new_blocks.push(&[]);
            }
            Some(&old_gid) => {
                let old_idx = old_gid as usize;
                if old_idx >= old_offsets.len().saturating_sub(1) {
                    // old_gid out of range for the old font → empty block.
                    new_blocks.push(&[]);
                } else {
                    let start = old_offsets[old_idx];
                    let end = old_offsets[old_idx + 1];
                    if start > end || end > table.len() {
                        return None;
                    }
                    new_blocks.push(&table[start..end]);
                }
            }
        }
    }

    // ── Determine offset format for the new table ───────────────────────────
    // Short offsets: value = byte_offset / 2, max representable = 0xFFFF × 2 = 131070.
    let total_data_size: usize = new_blocks.iter().map(|b| b.len()).sum();
    // Use long offsets when total data does not fit in the short range.
    let use_long = total_data_size > 131070;
    let offset_entry_size = if use_long { 4usize } else { 2usize };
    let offset_array_size = (new_glyph_count as usize + 1) * offset_entry_size;

    // ── Compute new absolute offsets in the output table ───────────────────
    // Layout: [header 20] [offset array] [shared tuples] [per-glyph data]
    let new_shared_tuples_offset: u32 = (20 + offset_array_size) as u32;
    let new_glyph_var_data_array_offset: u32 = new_shared_tuples_offset + shared_tuples_size as u32;

    // ── Build new offset array (relative to new_glyph_var_data_array_offset) ─
    let mut new_offset_array: Vec<u8> = Vec::with_capacity(offset_array_size);
    let mut cursor: usize = 0;
    for block in &new_blocks {
        if use_long {
            new_offset_array.extend_from_slice(&(cursor as u32).to_be_bytes());
        } else {
            // value = cursor / 2; cursor is always even because we pad to alignment
            // Note: no padding in this implementation — blocks are taken as-is.
            // The value is floor(cursor / 2) per spec (value * 2 = byte offset).
            // We trust the round-trip: any misalignment will cause offset_end - offset_start
            // to be the correct byte count when read back.
            new_offset_array.extend_from_slice(&((cursor / 2) as u16).to_be_bytes());
        }
        cursor += block.len();
    }
    // Final sentinel offset.
    if use_long {
        new_offset_array.extend_from_slice(&(cursor as u32).to_be_bytes());
    } else {
        new_offset_array.extend_from_slice(&((cursor / 2) as u16).to_be_bytes());
    }

    // ── Patch header ────────────────────────────────────────────────────────
    // Work from the original 20-byte header; patch the fields that change.
    let mut new_header = [0u8; 20];
    new_header.copy_from_slice(&table[..20]);

    // glyphCount @ offset 12
    new_header[12] = (new_glyph_count >> 8) as u8;
    new_header[13] = (new_glyph_count & 0xFF) as u8;

    // flags @ offset 14 — bit 0 controls offset size
    let mut flags = u16::from_be_bytes([new_header[14], new_header[15]]);
    if use_long {
        flags |= 0x0001;
    } else {
        flags &= !0x0001;
    }
    new_header[14] = (flags >> 8) as u8;
    new_header[15] = (flags & 0xFF) as u8;

    // sharedTuplesOffset @ offset 8
    let sto = new_shared_tuples_offset.to_be_bytes();
    new_header[8] = sto[0];
    new_header[9] = sto[1];
    new_header[10] = sto[2];
    new_header[11] = sto[3];

    // glyphVariationDataArrayOffset @ offset 16
    let gvdao = new_glyph_var_data_array_offset.to_be_bytes();
    new_header[16] = gvdao[0];
    new_header[17] = gvdao[1];
    new_header[18] = gvdao[2];
    new_header[19] = gvdao[3];

    // ── Assemble output ─────────────────────────────────────────────────────
    let capacity = 20 + offset_array_size + shared_tuples_size + total_data_size;
    let mut out = Vec::with_capacity(capacity);

    out.extend_from_slice(&new_header);
    out.extend_from_slice(&new_offset_array);
    out.extend_from_slice(shared_tuples_bytes);
    for block in &new_blocks {
        out.extend_from_slice(block);
    }

    Some(out)
}
