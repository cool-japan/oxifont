//! OpenType Layout GPOS table rewriter.
//!
//! Implements GPOS-specific subtable handlers on top of the shared SFL
//! infrastructure from `otl.rs`.
//!
//! # Supported GPOS lookup types
//! - Type 1: SinglePos (Format 1 and 2)
//! - Type 2: PairPos (Format 1 and 2)
//! - Type 4: MarkBasePos (Format 1)
//! - Type 6: MarkMarkPos (Format 1)
//! - Type 9: ExtensionPos (Format 1) — recursively rewrites the inner subtype
//! - Types 3, 5, 7, 8: dropped (return `None`)
//!
//! # ValueRecord
//! ValueRecords contain no GID references — they are copied verbatim.
//! The size is determined by the `valueFormat` bitmask (2 bytes per set bit,
//! bits 0–7 only).

use crate::layout::{read_coverage, remap_classdef, remap_coverage};
use crate::otl::{rewrite_feature_list, rewrite_lookup_list_with, rewrite_script_list};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Internal big-endian helpers (shared pattern from otl.rs)
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
fn w_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn w_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn patch_u16(out: &mut [u8], offset: usize, v: u16) {
    if offset + 2 <= out.len() {
        out[offset] = (v >> 8) as u8;
        out[offset + 1] = (v & 0xFF) as u8;
    }
}

// ---------------------------------------------------------------------------
// ValueRecord helpers
// ---------------------------------------------------------------------------

/// Return the byte size of a ValueRecord for the given `valueFormat`.
///
/// Each set bit in bits 0–7 of `valueFormat` contributes one i16 field (2 bytes).
#[inline]
fn value_record_size(value_format: u16) -> usize {
    (value_format & 0x00FF).count_ones() as usize * 2
}

// ---------------------------------------------------------------------------
// GPOS Type 1: SinglePos
// ---------------------------------------------------------------------------

/// Read and copy one ValueRecord of the given size.
fn copy_value_record(data: &[u8], offset: usize, size: usize) -> Option<Vec<u8>> {
    data.get(offset..offset + size).map(|b| b.to_vec())
}

/// GPOS Type 1, Format 1: single ValueRecord for all covered glyphs.
fn rewrite_single_pos_f1(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let cov_offset = r_u16(sub, 2)? as usize;
    let value_format = r_u16(sub, 4)?;
    let vr_size = value_record_size(value_format);

    if sub.len() < 6 + vr_size {
        return None;
    }

    let vr_bytes = copy_value_record(sub, 6, vr_size)?;

    // Remap coverage; if no glyphs survive, drop the subtable.
    let (new_cov_bytes, new_gids) = remap_coverage(data, offset + cov_offset, gid_remap);
    if new_gids.is_empty() {
        return None;
    }

    // Layout: format(2) + coverageOffset(2) + valueFormat(2) + valueRecord(vr_size)
    //         + coverage data
    let cov_start = (6 + vr_size) as u16;
    let mut out = Vec::with_capacity(6 + vr_size + new_cov_bytes.len());
    w_u16(&mut out, 1); // format
    w_u16(&mut out, cov_start);
    w_u16(&mut out, value_format);
    out.extend_from_slice(&vr_bytes);
    out.extend_from_slice(&new_cov_bytes);
    Some(out)
}

/// GPOS Type 1, Format 2: one ValueRecord per coverage entry.
fn rewrite_single_pos_f2(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let cov_offset = r_u16(sub, 2)? as usize;
    let value_format = r_u16(sub, 4)?;
    let value_count = r_u16(sub, 6)? as usize;
    let vr_size = value_record_size(value_format);

    if sub.len() < 8 + value_count * vr_size {
        return None;
    }

    // Read old coverage GIDs (in coverage order).
    let old_gids = read_coverage(data, offset + cov_offset);
    if old_gids.len() != value_count {
        return None;
    }

    // Build (new_gid, value_record_bytes) pairs, preserving coverage order.
    let mut pairs: Vec<(u16, Vec<u8>)> = Vec::new();
    for (i, &old_gid) in old_gids.iter().enumerate() {
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        let vr = copy_value_record(sub, 8 + i * vr_size, vr_size)?;
        pairs.push((new_gid, vr));
    }

    if pairs.is_empty() {
        return None;
    }

    // Sort by new_gid for coverage ordering.
    pairs.sort_unstable_by_key(|&(g, _)| g);
    pairs.dedup_by_key(|p| p.0);

    let new_count = pairs.len();
    let cov_gids: Vec<u16> = pairs.iter().map(|&(g, _)| g).collect();
    let new_cov_bytes = crate::layout::write_coverage(&cov_gids);

    // Layout: format(2) + coverageOffset(2) + valueFormat(2) + valueCount(2)
    //         + valueRecords(count * vr_size) + coverage
    let header_end = 8 + new_count * vr_size;
    let cov_start = header_end as u16;

    let mut out = Vec::with_capacity(header_end + new_cov_bytes.len());
    w_u16(&mut out, 2); // format
    w_u16(&mut out, cov_start);
    w_u16(&mut out, value_format);
    w_u16(&mut out, new_count as u16);
    for (_, vr) in &pairs {
        out.extend_from_slice(vr);
    }
    out.extend_from_slice(&new_cov_bytes);
    Some(out)
}

fn rewrite_single_pos(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    match format {
        1 => rewrite_single_pos_f1(data, offset, gid_remap),
        2 => rewrite_single_pos_f2(data, offset, gid_remap),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// GPOS Type 2: PairPos
// ---------------------------------------------------------------------------

/// A single pair adjustment record: (second_glyph, value_record1_bytes, value_record2_bytes).
type PairRecord = (u16, Vec<u8>, Vec<u8>);

/// GPOS Type 2, Format 1: explicit PairSets per first-glyph.
fn rewrite_pair_pos_f1(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let cov_offset = r_u16(sub, 2)? as usize;
    let value_format1 = r_u16(sub, 4)?;
    let value_format2 = r_u16(sub, 6)?;
    let pair_set_count = r_u16(sub, 8)? as usize;

    if sub.len() < 10 + pair_set_count * 2 {
        return None;
    }

    let vr1_size = value_record_size(value_format1);
    let vr2_size = value_record_size(value_format2);
    let pair_record_size = 2 + vr1_size + vr2_size;

    let old_gids = read_coverage(data, offset + cov_offset);
    if old_gids.len() != pair_set_count {
        return None;
    }

    // Collect surviving (new_first_gid, Vec<PairRecord>) entries.
    let mut sets: Vec<(u16, Vec<PairRecord>)> = Vec::new();

    for (i, &old_first) in old_gids.iter().enumerate() {
        let new_first = match gid_remap.get(&old_first) {
            Some(&g) => g,
            None => continue,
        };

        let ps_off = r_u16(sub, 10 + i * 2)? as usize;
        let ps_data = sub.get(ps_off..)?;
        let pair_value_count = r_u16(ps_data, 0)? as usize;
        if ps_data.len() < 2 + pair_value_count * pair_record_size {
            return None;
        }

        let mut surviving_pairs: Vec<PairRecord> = Vec::new();
        for j in 0..pair_value_count {
            let rec_off = 2 + j * pair_record_size;
            let second_glyph = r_u16(ps_data, rec_off)?;
            let new_second = match gid_remap.get(&second_glyph) {
                Some(&g) => g,
                None => continue,
            };
            let vr1 = ps_data.get(rec_off + 2..rec_off + 2 + vr1_size)?.to_vec();
            let vr2 = ps_data
                .get(rec_off + 2 + vr1_size..rec_off + 2 + vr1_size + vr2_size)?
                .to_vec();
            surviving_pairs.push((new_second, vr1, vr2));
        }

        if !surviving_pairs.is_empty() {
            sets.push((new_first, surviving_pairs));
        }
    }

    if sets.is_empty() {
        return None;
    }

    sets.sort_unstable_by_key(|&(g, _)| g);
    sets.dedup_by_key(|s| s.0);

    let new_count = sets.len();
    let cov_gids: Vec<u16> = sets.iter().map(|&(g, _)| g).collect();
    let new_cov_bytes = crate::layout::write_coverage(&cov_gids);

    // Layout: format(2) + coverageOffset(2) + valueFormat1(2) + valueFormat2(2)
    //         + pairSetCount(2) + pairSetOffsets[n](2*n) + coverage + PairSets...
    let header_end = 10 + new_count * 2;
    let cov_start = header_end as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    w_u16(&mut out, cov_start);
    w_u16(&mut out, value_format1);
    w_u16(&mut out, value_format2);
    w_u16(&mut out, new_count as u16);

    let ps_offsets_pos = out.len();
    for _ in 0..new_count {
        w_u16(&mut out, 0); // placeholder
    }
    out.extend_from_slice(&new_cov_bytes);

    let mut ps_offs: Vec<u16> = Vec::with_capacity(new_count);
    for (_, pairs) in &sets {
        ps_offs.push(out.len() as u16);
        w_u16(&mut out, pairs.len() as u16);
        for (new_second, vr1, vr2) in pairs {
            w_u16(&mut out, *new_second);
            out.extend_from_slice(vr1);
            out.extend_from_slice(vr2);
        }
    }
    for (i, &off) in ps_offs.iter().enumerate() {
        patch_u16(&mut out, ps_offsets_pos + i * 2, off);
    }

    Some(out)
}

/// GPOS Type 2, Format 2: class-based pair adjustment.
fn rewrite_pair_pos_f2(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let cov_offset = r_u16(sub, 2)? as usize;
    let value_format1 = r_u16(sub, 4)?;
    let value_format2 = r_u16(sub, 6)?;
    let classdef1_offset = r_u16(sub, 8)? as usize;
    let classdef2_offset = r_u16(sub, 10)? as usize;
    let class1_count = r_u16(sub, 12)? as usize;
    let class2_count = r_u16(sub, 14)? as usize;

    let vr1_size = value_record_size(value_format1);
    let vr2_size = value_record_size(value_format2);
    let class2_record_size = vr1_size + vr2_size;
    let class1_record_size = class2_count * class2_record_size;
    let matrix_size = class1_count * class1_record_size;

    if sub.len() < 16 + matrix_size {
        return None;
    }

    // Remap coverage: if zero first-glyphs survive, drop.
    let (new_cov_bytes, new_first_gids) = remap_coverage(data, offset + cov_offset, gid_remap);
    if new_first_gids.is_empty() {
        return None;
    }

    // Remap ClassDef1 and ClassDef2.
    let new_classdef1 = remap_classdef(data, offset + classdef1_offset, gid_remap);
    let new_classdef2 = remap_classdef(data, offset + classdef2_offset, gid_remap);

    // Copy the matrix verbatim — no GID references inside it.
    let matrix_bytes = sub.get(16..16 + matrix_size)?.to_vec();

    // Layout: format(2) + coverageOffset(2) + valueFormat1(2) + valueFormat2(2)
    //         + classDef1Offset(2) + classDef2Offset(2)
    //         + class1Count(2) + class2Count(2) = 16 bytes header
    //         + coverage + classDef1 + classDef2 + matrix
    let cov_start = 16u16;
    let classdef1_start = (16 + new_cov_bytes.len()) as u16;
    let classdef2_start = classdef1_start + new_classdef1.len() as u16;
    // Matrix follows after the header, immediately after classDef2.
    let matrix_start = classdef2_start + new_classdef2.len() as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 2); // format
    w_u16(&mut out, cov_start);
    w_u16(&mut out, value_format1);
    w_u16(&mut out, value_format2);
    w_u16(&mut out, classdef1_start);
    w_u16(&mut out, classdef2_start);
    w_u16(&mut out, class1_count as u16);
    w_u16(&mut out, class2_count as u16);
    out.extend_from_slice(&new_cov_bytes);
    out.extend_from_slice(&new_classdef1);
    out.extend_from_slice(&new_classdef2);
    // Pad to matrix_start if needed (should not be needed in a well-formed rebuild).
    while out.len() < matrix_start as usize {
        out.push(0);
    }
    out.extend_from_slice(&matrix_bytes);
    Some(out)
}

fn rewrite_pair_pos(data: &[u8], offset: usize, gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    match format {
        1 => rewrite_pair_pos_f1(data, offset, gid_remap),
        2 => rewrite_pair_pos_f2(data, offset, gid_remap),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Anchor helpers for MarkBasePos / MarkMarkPos
// ---------------------------------------------------------------------------

/// Return the byte size of an Anchor record at `anchor_offset` within `data`.
///
/// Format 1: 6 bytes (format + XCoordinate + YCoordinate)
/// Format 2: 8 bytes (format + X + Y + AnchorPoint)
/// Format 3: 10 bytes (format + X + Y + XDeviceOffset + YDeviceOffset)
///           Device tables are referenced by offset but we treat the anchor as
///           10 bytes and copy device data separately via sorted-offsets strategy.
///
/// Returns `None` on parse failure or unknown format.
fn anchor_fixed_size(data: &[u8], anchor_offset: usize) -> Option<usize> {
    let format = r_u16(data, anchor_offset)?;
    match format {
        1 => Some(6),
        2 => Some(8),
        3 => Some(10),
        _ => None,
    }
}

/// Parse MarkArray at `mark_array_offset` within `data` (absolute).
///
/// Returns a Vec of (markClass, anchor_blob_bytes) per mark glyph.
fn parse_mark_array(data: &[u8], mark_array_offset: usize) -> Option<Vec<(u16, Vec<u8>)>> {
    let ma = data.get(mark_array_offset..)?;
    let mark_count = r_u16(ma, 0)? as usize;
    if ma.len() < 2 + mark_count * 4 {
        return None;
    }

    // Collect all (markClass, anchorOffset-relative-to-MarkArray) pairs first.
    let mut entries: Vec<(u16, usize)> = Vec::with_capacity(mark_count);
    for i in 0..mark_count {
        let mark_class = r_u16(ma, 2 + i * 4)?;
        let anc_off = r_u16(ma, 2 + i * 4 + 2)? as usize;
        entries.push((mark_class, anc_off));
    }

    // Sort anchor offsets to determine region for each anchor.
    let mut sorted_anc_offsets: Vec<usize> = entries.iter().map(|&(_, o)| o).collect();
    sorted_anc_offsets.sort_unstable();
    sorted_anc_offsets.dedup();

    let mut result: Vec<(u16, Vec<u8>)> = Vec::with_capacity(mark_count);
    for (mark_class, anc_off) in entries {
        let abs_anc = mark_array_offset + anc_off;
        let fixed_sz = anchor_fixed_size(data, abs_anc).unwrap_or(6);
        // For format 3 anchors, try to include device table data by reading
        // until the next anchor offset or end of region.
        let next_off = sorted_anc_offsets
            .iter()
            .find(|&&o| o > anc_off)
            .copied()
            .unwrap_or(anc_off + fixed_sz);
        let copy_sz = next_off - anc_off;
        let anc_bytes = data
            .get(abs_anc..abs_anc + copy_sz)
            .unwrap_or_else(|| data.get(abs_anc..abs_anc + fixed_sz).unwrap_or(&[]))
            .to_vec();
        result.push((mark_class, anc_bytes));
    }

    Some(result)
}

/// Parse BaseArray at `base_array_offset` (absolute), with `mark_class_count`
/// anchor offsets per base record.
///
/// Returns a Vec of anchor-blob-Vecs (one Vec<Vec<u8>> per base glyph).
fn parse_base_array(
    data: &[u8],
    base_array_offset: usize,
    mark_class_count: usize,
) -> Option<Vec<Vec<Vec<u8>>>> {
    let ba = data.get(base_array_offset..)?;
    let base_count = r_u16(ba, 0)? as usize;
    if mark_class_count == 0 {
        // No mark classes → no anchor data per base record.
        return Some(vec![vec![]; base_count]);
    }
    if ba.len() < 2 + base_count * mark_class_count * 2 {
        return None;
    }

    // Collect all anchor offsets across all base records.
    let mut all_anc_offsets: Vec<usize> = Vec::new();
    for i in 0..base_count {
        for j in 0..mark_class_count {
            let anc_off = r_u16(ba, 2 + (i * mark_class_count + j) * 2)? as usize;
            all_anc_offsets.push(anc_off);
        }
    }

    let mut sorted_anc: Vec<usize> = all_anc_offsets.clone();
    sorted_anc.sort_unstable();
    sorted_anc.dedup();

    let mut result: Vec<Vec<Vec<u8>>> = Vec::with_capacity(base_count);
    let mut idx = 0usize;
    for _ in 0..base_count {
        let mut rec: Vec<Vec<u8>> = Vec::with_capacity(mark_class_count);
        for _ in 0..mark_class_count {
            let anc_off = all_anc_offsets[idx];
            idx += 1;
            if anc_off == 0 {
                // NULL anchor.
                rec.push(vec![0u8; 6]); // emit a format-1 zero anchor
                continue;
            }
            let abs_anc = base_array_offset + anc_off;
            let fixed_sz = anchor_fixed_size(data, abs_anc).unwrap_or(6);
            let next_off = sorted_anc
                .iter()
                .find(|&&o| o > anc_off)
                .copied()
                .unwrap_or(anc_off + fixed_sz);
            let copy_sz = next_off - anc_off;
            let anc_bytes = data
                .get(abs_anc..abs_anc + copy_sz)
                .unwrap_or_else(|| data.get(abs_anc..abs_anc + fixed_sz).unwrap_or(&[]))
                .to_vec();
            rec.push(anc_bytes);
        }
        result.push(rec);
    }

    Some(result)
}

// ---------------------------------------------------------------------------
// GPOS Type 4: MarkBasePos / Type 6: MarkMarkPos
// ---------------------------------------------------------------------------

/// Core implementation shared by MarkBasePos (type 4) and MarkMarkPos (type 6).
///
/// Both use format 1 with identical structure — only the names differ.
fn rewrite_mark_pair_pos_f1(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    if format != 1 {
        return None;
    }

    let mark_cov_offset = r_u16(sub, 2)? as usize;
    let base_cov_offset = r_u16(sub, 4)? as usize;
    let mark_class_count = r_u16(sub, 6)? as usize;
    let mark_array_offset = r_u16(sub, 8)? as usize;
    let base_array_offset = r_u16(sub, 10)? as usize;

    // Remap mark coverage.
    let old_mark_gids = read_coverage(data, offset + mark_cov_offset);
    let mut new_mark_entries: Vec<(u16, u16, Vec<u8>)> = Vec::new(); // (new_gid, markClass, anchor)

    // Parse the full MarkArray.
    let mark_arr = parse_mark_array(data, offset + mark_array_offset)?;
    if mark_arr.len() != old_mark_gids.len() {
        return None;
    }

    for (i, &old_gid) in old_mark_gids.iter().enumerate() {
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        let (mark_class, ref anc) = mark_arr[i];
        new_mark_entries.push((new_gid, mark_class, anc.clone()));
    }

    if new_mark_entries.is_empty() {
        return None;
    }

    // Sort by new_gid for coverage.
    new_mark_entries.sort_unstable_by_key(|&(g, _, _)| g);
    new_mark_entries.dedup_by_key(|e| e.0);

    // Remap base coverage.
    let old_base_gids = read_coverage(data, offset + base_cov_offset);
    let base_arr = parse_base_array(data, offset + base_array_offset, mark_class_count)?;
    if base_arr.len() != old_base_gids.len() {
        return None;
    }

    let mut new_base_entries: Vec<(u16, Vec<Vec<u8>>)> = Vec::new(); // (new_gid, anchors_per_class)
    for (i, &old_gid) in old_base_gids.iter().enumerate() {
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        new_base_entries.push((new_gid, base_arr[i].clone()));
    }

    if new_base_entries.is_empty() {
        return None;
    }

    new_base_entries.sort_unstable_by_key(|&(g, _)| g);
    new_base_entries.dedup_by_key(|e| e.0);

    // Build new coverage tables.
    let mark_cov_gids: Vec<u16> = new_mark_entries.iter().map(|&(g, _, _)| g).collect();
    let base_cov_gids: Vec<u16> = new_base_entries.iter().map(|&(g, _)| g).collect();
    let new_mark_cov = crate::layout::write_coverage(&mark_cov_gids);
    let new_base_cov = crate::layout::write_coverage(&base_cov_gids);

    // Build new MarkArray.
    // MarkArray: markCount(2) + markRecords[n]×(markClass(2) + markAnchorOffset(2))
    //            + anchor data
    let n_marks = new_mark_entries.len();
    let mark_array_header_size = 2 + n_marks * 4;
    let mut mark_array_out: Vec<u8> = Vec::new();
    w_u16(&mut mark_array_out, n_marks as u16);
    // Placeholder record offsets.
    let mark_rec_offsets_pos = mark_array_out.len();
    for _ in 0..n_marks {
        w_u16(&mut mark_array_out, 0); // markClass placeholder
        w_u16(&mut mark_array_out, 0); // anchorOffset placeholder
    }
    // Anchor data.
    let mut mark_anc_offsets: Vec<u16> = Vec::with_capacity(n_marks);
    for (_, mark_class, anc) in &new_mark_entries {
        mark_anc_offsets.push(mark_array_out.len() as u16);
        // Patch markClass and anchorOffset into the header slot.
        let _ = mark_class; // will patch below
        mark_array_out.extend_from_slice(anc);
    }
    // Patch records.
    for (i, (_, mark_class, _)) in new_mark_entries.iter().enumerate() {
        let pos = mark_rec_offsets_pos + i * 4;
        patch_u16(&mut mark_array_out, pos, *mark_class);
        patch_u16(&mut mark_array_out, pos + 2, mark_anc_offsets[i]);
    }
    let _ = mark_array_header_size;

    // Build new BaseArray.
    // BaseArray: baseCount(2) + baseRecords[n]×(markClassCount × anchorOffset(2))
    //            + anchor data
    let n_bases = new_base_entries.len();
    let base_array_header_size = 2 + n_bases * mark_class_count * 2;
    let mut base_array_out: Vec<u8> = Vec::new();
    w_u16(&mut base_array_out, n_bases as u16);
    // Placeholder anchor offsets.
    let base_anc_offset_pos = base_array_out.len();
    for _ in 0..(n_bases * mark_class_count) {
        w_u16(&mut base_array_out, 0);
    }
    // Anchor data.
    let mut base_anc_offsets: Vec<u16> = Vec::new();
    for (_, ancs) in &new_base_entries {
        for anc in ancs {
            base_anc_offsets.push(base_array_out.len() as u16);
            base_array_out.extend_from_slice(anc);
        }
    }
    // Patch base anchor offsets.
    for (idx, &off) in base_anc_offsets.iter().enumerate() {
        patch_u16(&mut base_array_out, base_anc_offset_pos + idx * 2, off);
    }
    let _ = base_array_header_size;

    // Assemble full subtable.
    // Header: format(2) + markCovOff(2) + baseCovOff(2) + markClassCount(2)
    //         + markArrayOff(2) + baseArrayOff(2) = 12 bytes
    let header_size = 12usize;
    let mark_cov_start = header_size as u16;
    let base_cov_start = mark_cov_start + new_mark_cov.len() as u16;
    let mark_array_start = base_cov_start + new_base_cov.len() as u16;
    let base_array_start = mark_array_start + mark_array_out.len() as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    w_u16(&mut out, mark_cov_start);
    w_u16(&mut out, base_cov_start);
    w_u16(&mut out, mark_class_count as u16);
    w_u16(&mut out, mark_array_start);
    w_u16(&mut out, base_array_start);
    out.extend_from_slice(&new_mark_cov);
    out.extend_from_slice(&new_base_cov);
    out.extend_from_slice(&mark_array_out);
    out.extend_from_slice(&base_array_out);

    Some(out)
}

// ---------------------------------------------------------------------------
// GPOS Type 9: ExtensionPos
// ---------------------------------------------------------------------------

fn rewrite_extension_pos(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    if format != 1 {
        return None;
    }
    let ext_type = r_u16(sub, 2)?;
    if ext_type == 9 {
        // Spec forbids Extension wrapping Extension.
        return None;
    }
    let ext_offset = r_u32(sub, 4)? as usize;
    let inner_abs = offset + ext_offset;
    let inner_bytes = rewrite_gpos_subtable(data, inner_abs, ext_type, gid_remap)?;

    // Rebuild extension wrapper: format(2) + extLookupType(2) + extOffset(Offset32)
    let inner_offset: u32 = 8;
    let mut out = Vec::with_capacity(8 + inner_bytes.len());
    w_u16(&mut out, 1);
    w_u16(&mut out, ext_type);
    w_u32(&mut out, inner_offset);
    out.extend_from_slice(&inner_bytes);
    Some(out)
}

// ---------------------------------------------------------------------------
// GPOS subtable dispatcher
// ---------------------------------------------------------------------------

/// Dispatch to the correct GPOS subtable handler by `lookup_type`.
///
/// Returns `None` if:
/// - the lookup type is unsupported (3, 5, 7, 8),
/// - the subtable is malformed, or
/// - all GIDs in the subtable were removed from the subset.
pub fn rewrite_gpos_subtable(
    data: &[u8],
    offset: usize,
    lookup_type: u16,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    match lookup_type {
        1 => rewrite_single_pos(data, offset, gid_remap),
        2 => rewrite_pair_pos(data, offset, gid_remap),
        // Type 3 (CursivePos), 5 (MarkLigPos), 7 (ContextPos), 8 (ChainContextPos) — drop.
        3 | 5 | 7 | 8 => None,
        4 => rewrite_mark_pair_pos_f1(data, offset, gid_remap),
        6 => rewrite_mark_pair_pos_f1(data, offset, gid_remap),
        9 => rewrite_extension_pos(data, offset, gid_remap),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Public entry point: rewrite_gpos
// ---------------------------------------------------------------------------

/// Rewrite a GPOS table, remapping all GID references through `gid_remap`.
///
/// On parse failure or input < 10 bytes, returns the original bytes verbatim.
///
/// GPOS v1.1 FeatureVariations are dropped; the rebuilt table uses v1.0.
pub fn rewrite_gpos(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    if table.len() < 10 {
        return table.to_vec();
    }
    match try_rewrite_gpos(table, gid_remap) {
        Some(v) => v,
        None => table.to_vec(),
    }
}

fn try_rewrite_gpos(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    let major = r_u16(table, 0)?;
    if major != 1 {
        return None;
    }

    let sl_offset = r_u16(table, 4)? as usize;
    let fl_offset = r_u16(table, 6)? as usize;
    let ll_offset = r_u16(table, 8)? as usize;

    // ---- Step 1: Rewrite LookupList ----
    let (new_ll_bytes, lk_index_map) =
        rewrite_lookup_list_with(table, ll_offset, gid_remap, rewrite_gpos_subtable);

    // ---- Step 2: Rewrite FeatureList ----
    let (new_fl_bytes, feat_index_map) = rewrite_feature_list(table, fl_offset, &lk_index_map);

    // ---- Step 3: Rewrite ScriptList ----
    let new_sl_bytes = rewrite_script_list(table, sl_offset, &feat_index_map);

    // ---- Step 4: Assemble ----
    // Header (v1.0): majorVersion(2) + minorVersion(2) + scriptListOffset(2) +
    //                featureListOffset(2) + lookupListOffset(2) = 10 bytes.
    let header_size: u16 = 10;
    let sl_off16 = header_size;
    let fl_off16 = sl_off16 + new_sl_bytes.len() as u16;
    let ll_off16 = fl_off16 + new_fl_bytes.len() as u16;

    let mut out = Vec::with_capacity(
        header_size as usize + new_sl_bytes.len() + new_fl_bytes.len() + new_ll_bytes.len(),
    );
    w_u16(&mut out, 1); // majorVersion
    w_u16(&mut out, 0); // minorVersion (always 0 in rebuilt table)
    w_u16(&mut out, sl_off16);
    w_u16(&mut out, fl_off16);
    w_u16(&mut out, ll_off16);
    out.extend_from_slice(&new_sl_bytes);
    out.extend_from_slice(&new_fl_bytes);
    out.extend_from_slice(&new_ll_bytes);

    Some(out)
}
