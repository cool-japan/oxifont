/// SVG table subsetting: remove SVG document index entries for removed GIDs.
///
/// SVG table format:
/// - Header (10 bytes): version(u16), offsetToSVGDocumentList(u32), reserved(u32).
/// - SVGDocumentList (at header offset):
///   - numEntries(u16)
///   - numEntries × SVGDocumentIndexEntry (12 bytes each):
///     startGlyphID(u16), endGlyphID(u16), svgDocOffset(u32), svgDocLength(u32)
///   - Offsets within entries are relative to start of SVGDocumentList.
///
/// Each entry covers a GID range [startGlyphID, endGlyphID] (inclusive) and
/// points to an opaque SVG document blob.
///
/// This subsetter uses a conservative strategy: an entry is retained only when
/// BOTH its startGlyphID AND endGlyphID survive in `gid_remap`.  Entries where
/// the start or end was removed (even if interior GIDs survive) are dropped.
/// This avoids producing entries with inconsistent start/end GID boundaries.
/// SVG document blobs are preserved verbatim (internal `id="glyphN"` references
/// would require XML parsing to remap, which is out of scope).
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

// ─── internal structures ──────────────────────────────────────────────────────

struct SvgIndexEntry {
    start_gid: u16,
    end_gid: u16,
    doc_offset: u32,
    doc_length: u32,
}

// ─── public API ───────────────────────────────────────────────────────────────

/// Rewrite an SVG table to reflect the new GID space.
///
/// Entries where startGlyphID or endGlyphID are no longer in `gid_remap` are
/// dropped.  SVG document blobs are kept verbatim and re-packed; their internal
/// `id="glyphN"` attributes are not modified.
///
/// On any parse failure: returned verbatim.
pub fn rewrite_svg(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    rewrite_svg_inner(table, gid_remap).unwrap_or_else(|| table.to_vec())
}

fn rewrite_svg_inner(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    // Minimum header: 10 bytes.
    if table.len() < 10 {
        return None;
    }

    let version = r_u16(table, 0)?;
    if version != 0 {
        // Unknown SVG table version — preserve verbatim.
        return Some(table.to_vec());
    }

    let list_offset = r_u32(table, 2)? as usize;
    // reserved at bytes 6-9 (ignored).

    // SVGDocumentList starts at list_offset.
    if list_offset + 2 > table.len() {
        return None;
    }

    let num_entries = r_u16(table, list_offset)? as usize;
    let entries_start = list_offset + 2;

    if entries_start + num_entries * 12 > table.len() {
        return None;
    }

    // Parse all entries.
    let mut entries: Vec<SvgIndexEntry> = Vec::with_capacity(num_entries);
    for i in 0..num_entries {
        let off = entries_start + i * 12;
        entries.push(SvgIndexEntry {
            start_gid: r_u16(table, off)?,
            end_gid: r_u16(table, off + 2)?,
            doc_offset: r_u32(table, off + 4)?,
            doc_length: r_u32(table, off + 8)?,
        });
    }

    // ─── Select surviving entries ─────────────────────────────────────────────
    //
    // Conservative strategy: keep an entry only when BOTH startGlyphID and
    // endGlyphID survive.  Remap them to their new GIDs.

    struct SurvivingEntry<'a> {
        new_start: u16,
        new_end: u16,
        blob: &'a [u8],
    }

    let mut survivors: Vec<SurvivingEntry<'_>> = Vec::new();

    for entry in &entries {
        let new_start = match gid_remap.get(&entry.start_gid) {
            Some(&g) => g,
            None => continue,
        };
        let new_end = match gid_remap.get(&entry.end_gid) {
            Some(&g) => g,
            None => continue,
        };

        // Validate SVG doc blob offset/length (relative to SVGDocumentList start).
        let doc_abs_start = list_offset.checked_add(entry.doc_offset as usize)?;
        let doc_abs_end = doc_abs_start.checked_add(entry.doc_length as usize)?;
        if doc_abs_end > table.len() {
            return None;
        }

        let blob = &table[doc_abs_start..doc_abs_end];
        survivors.push(SurvivingEntry {
            new_start,
            new_end,
            blob,
        });
    }

    // Sort surviving entries by new startGlyphID (required by spec).
    survivors.sort_unstable_by_key(|e| e.new_start);

    // ─── Serialise ────────────────────────────────────────────────────────────
    //
    // Layout:
    //   SVG header (10 bytes)
    //   SVGDocumentList:
    //     numEntries (2 bytes)
    //     entries × 12 bytes
    //     SVG blobs (packed sequentially)

    let new_num = survivors.len();
    // offsetToSVGDocumentList is always 10 (immediately after the 10-byte header).
    let new_list_offset: u32 = 10;
    // SVG blobs start after numEntries + entries.
    let blobs_start_in_list: u32 = 2 + (new_num as u32) * 12;

    // Compute per-entry offsets in the new document list (relative to list start).
    let mut blob_cursor: u32 = blobs_start_in_list;
    let mut entry_offsets: Vec<u32> = Vec::with_capacity(new_num);
    for s in &survivors {
        entry_offsets.push(blob_cursor);
        blob_cursor = blob_cursor.checked_add(s.blob.len() as u32)?;
    }

    let total_blobs: usize = survivors.iter().map(|s| s.blob.len()).sum();
    let total_size = 10 + 2 + new_num * 12 + total_blobs;
    let mut out: Vec<u8> = Vec::with_capacity(total_size);

    // SVG header
    out.extend_from_slice(&0u16.to_be_bytes()); // version
    out.extend_from_slice(&new_list_offset.to_be_bytes()); // offsetToSVGDocumentList
    out.extend_from_slice(&0u32.to_be_bytes()); // reserved

    // SVGDocumentList header
    out.extend_from_slice(&(new_num as u16).to_be_bytes());

    // Index entries
    for (i, s) in survivors.iter().enumerate() {
        out.extend_from_slice(&s.new_start.to_be_bytes());
        out.extend_from_slice(&s.new_end.to_be_bytes());
        out.extend_from_slice(&entry_offsets[i].to_be_bytes());
        out.extend_from_slice(&(s.blob.len() as u32).to_be_bytes());
    }

    // SVG document blobs
    for s in &survivors {
        out.extend_from_slice(s.blob);
    }

    Some(out)
}
