//! CBDT/CBLC table subsetting.
//!
//! `CBLC` is the Color Bitmap Location table (index structure, analogous to
//! `loca`). `CBDT` is the Color Bitmap Data table (actual pixel data).
//!
//! # Conservative approach
//!
//! Fully rebuilding IndexSubTable offset arrays (Format 1/3/5) would require
//! a deep parse of every per-GID bitmap record, which is disproportionately
//! complex for a pure-subsetter. Instead we apply the following safe strategy:
//!
//! 1. Walk `CBLC` header's `BitmapSizeRecord` array.
//! 2. For each record, compute the subset of surviving GIDs in its range.
//! 3. If no survivors → drop the entire record (including its IndexSubTables).
//! 4. If survivors exist → remap `startGlyphIndex`/`endGlyphIndex` to the min
//!    and max mapped new-GID values from the surviving set.  The
//!    `IndexSubTable` offset arrays are left verbatim (they still reference
//!    valid `CBDT` offsets).
//!
//! **Known limitation (A):** when the surviving GIDs are not contiguous in the
//! original range the remapped IndexSubTable entries may resolve to wrong
//! bitmaps for the gap positions.  The font remains structurally valid — no
//! out-of-bounds reads — but a renderer may display incorrect bitmaps for
//! those gap GIDs.  Fixing this fully would require per-format IndexSubTable
//! rebuilding and is out of scope.
//!
//! `CBDT` data is kept verbatim; only the `CBLC` index is rewritten.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Big-endian integer helpers
// ---------------------------------------------------------------------------

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

#[inline]
fn w_u16(out: &mut [u8], offset: usize, v: u16) {
    if offset + 2 <= out.len() {
        out[offset] = (v >> 8) as u8;
        out[offset + 1] = (v & 0xFF) as u8;
    }
}

#[inline]
fn w_u32(out: &mut [u8], offset: usize, v: u32) {
    if offset + 4 <= out.len() {
        out[offset] = (v >> 24) as u8;
        out[offset + 1] = (v >> 16) as u8;
        out[offset + 2] = (v >> 8) as u8;
        out[offset + 3] = v as u8;
    }
}

// ---------------------------------------------------------------------------
// BitmapSizeRecord layout constants
// ---------------------------------------------------------------------------

// Each BitmapSizeRecord is 48 bytes (per OpenType spec).
const BITMAP_SIZE_RECORD_LEN: usize = 48;
// CBLC header before the record array: 8 bytes (u16 major, u16 minor, u32 numSizes).
const CBLC_HEADER_LEN: usize = 8;

// Offsets within a BitmapSizeRecord:
const OFF_INDEX_SUB_TABLE_ARRAY_OFFSET: usize = 0; // u32
                                                   // indexTablesSize (u32) at offset 4 — not read (kept verbatim).
                                                   // numberOfIndexSubTables (u32) at offset 8 — not read (kept verbatim).
                                                   // colorRef: u32 at 12
                                                   // hori SbitLineMetrics: 12 bytes at 16
                                                   // vert SbitLineMetrics: 12 bytes at 28
const OFF_START_GLYPH_INDEX: usize = 40; // u16
const OFF_END_GLYPH_INDEX: usize = 42; // u16
                                       // ppemX u8 at 44, ppemY u8 at 45, bitDepth u8 at 46, flags u8 at 47

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Rewrite `CBLC` to cover only the surviving GIDs in `gid_remap`; keep
/// `CBDT` verbatim.
///
/// Returns `(new_cblc, new_cbdt)`.  On any parse failure both tables are
/// returned verbatim (safe fallback).
pub fn rewrite_cbdt_cblc(
    cblc: &[u8],
    cbdt: &[u8],
    gid_remap: &HashMap<u16, u16>,
) -> (Vec<u8>, Vec<u8>) {
    match try_rewrite_cblc(cblc, gid_remap) {
        Some(new_cblc) => (new_cblc, cbdt.to_vec()),
        None => (cblc.to_vec(), cbdt.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

/// Returns `None` on any parse error (caller will fall back to verbatim).
fn try_rewrite_cblc(cblc: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    if cblc.len() < CBLC_HEADER_LEN {
        return None;
    }

    let major_version = r_u16(cblc, 0)?;
    let minor_version = r_u16(cblc, 2)?;
    let num_sizes = r_u32(cblc, 4)? as usize;

    // Validate header + records fit in the table.
    let records_start = CBLC_HEADER_LEN;
    let records_end = records_start + num_sizes * BITMAP_SIZE_RECORD_LEN;
    if records_end > cblc.len() {
        return None;
    }

    // Collect surviving records.
    let mut surviving_records: Vec<BitmapSizeRecordInfo> = Vec::new();

    for i in 0..num_sizes {
        let rec_base = records_start + i * BITMAP_SIZE_RECORD_LEN;
        let rec = &cblc[rec_base..rec_base + BITMAP_SIZE_RECORD_LEN];

        let start_gid = r_u16(rec, OFF_START_GLYPH_INDEX)?;
        let end_gid = r_u16(rec, OFF_END_GLYPH_INDEX)?;

        // Find min/max new GID among survivors in [start_gid, end_gid].
        let mut new_start: Option<u16> = None;
        let mut new_end: Option<u16> = None;
        for old_gid in start_gid..=end_gid {
            if let Some(&new_gid) = gid_remap.get(&old_gid) {
                new_start = Some(match new_start {
                    None => new_gid,
                    Some(s) => s.min(new_gid),
                });
                new_end = Some(match new_end {
                    None => new_gid,
                    Some(e) => e.max(new_gid),
                });
            }
        }

        // No survivors → drop this record.
        let (new_start, new_end) = match (new_start, new_end) {
            (Some(s), Some(e)) => (s, e),
            _ => continue,
        };

        surviving_records.push(BitmapSizeRecordInfo {
            raw: rec.to_vec(),
            new_start_gid: new_start,
            new_end_gid: new_end,
        });
    }

    // Build the new CBLC.
    let new_num_sizes = surviving_records.len() as u32;

    // We keep the same IndexSubTable data as before.  The records reference
    // offsets from the start of CBLC; we keep them unchanged so the CBDT
    // lookups still resolve correctly.
    let mut out = Vec::with_capacity(cblc.len());

    // Header.
    out.extend_from_slice(&major_version.to_be_bytes());
    out.extend_from_slice(&minor_version.to_be_bytes());
    out.extend_from_slice(&new_num_sizes.to_be_bytes());

    // Surviving records.
    for info in &surviving_records {
        let mut rec = info.raw.clone();
        w_u16(&mut rec, OFF_START_GLYPH_INDEX, info.new_start_gid);
        w_u16(&mut rec, OFF_END_GLYPH_INDEX, info.new_end_gid);
        out.extend_from_slice(&rec);
    }

    // Append all data after the record array (IndexSubTable arrays).
    // These are still referenced by the surviving records with their original
    // `indexSubTableArrayOffset` values (relative to start of CBLC).
    // Since we didn't shift any offsets, we must keep the data in place.
    // The simplest way: copy everything after the original record array.
    // But the surviving records' offsets point into the ORIGINAL cblc body;
    // we need to re-emit the full body after the header unchanged, and
    // then patch only numSizes + record list.
    //
    // Easiest correct approach: emit the original table verbatim except
    // for the header numSizes and the record array (replacing dropped records
    // with nothing, but leaving body intact at original offsets).
    //
    // To keep offsets valid we simply:
    // 1. Emit new header (8 bytes) + surviving records.
    // 2. Emit the body region (everything from records_end onwards) verbatim.
    //
    // The surviving records already had their raw bytes copied from the
    // original cblc, so their `indexSubTableArrayOffset` values still point
    // into the original body region correctly — but now the body is shifted
    // because the record array is shorter.
    //
    // FIX: we need to adjust all indexSubTableArrayOffset values by the
    // delta (original_records_end - new_records_end).
    let orig_records_end = records_end;
    let new_records_end = CBLC_HEADER_LEN + surviving_records.len() * BITMAP_SIZE_RECORD_LEN;
    let delta = orig_records_end.saturating_sub(new_records_end) as u32;

    // If delta != 0 we need to subtract delta from each record's
    // indexSubTableArrayOffset so they still point at the same body data.
    if delta != 0 {
        // Patch each surviving record already pushed into `out`.
        for j in 0..surviving_records.len() {
            let rec_start = CBLC_HEADER_LEN + j * BITMAP_SIZE_RECORD_LEN;
            if let Some(orig_offset) = r_u32(&out, rec_start + OFF_INDEX_SUB_TABLE_ARRAY_OFFSET) {
                let new_offset = orig_offset.saturating_sub(delta);
                w_u32(
                    &mut out,
                    rec_start + OFF_INDEX_SUB_TABLE_ARRAY_OFFSET,
                    new_offset,
                );
            }
        }
    }

    // Append body (IndexSubTable data) verbatim.
    if orig_records_end <= cblc.len() {
        out.extend_from_slice(&cblc[orig_records_end..]);
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// Internal data structures
// ---------------------------------------------------------------------------

struct BitmapSizeRecordInfo {
    /// Raw bytes of the original BitmapSizeRecord (48 bytes).
    raw: Vec<u8>,
    /// New startGlyphIndex (min mapped survivor).
    new_start_gid: u16,
    /// New endGlyphIndex (max mapped survivor).
    new_end_gid: u16,
}
