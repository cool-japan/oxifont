//! Layout table helpers: Coverage, ClassDef, and GDEF rewriters.
//!
//! These are low-level utilities used by the subsetting pipeline to remap
//! GID references inside OpenType layout tables.
//!
//! # Coverage tables
//!
//! Format 1: explicit sorted GID list.
//! Format 2: sorted ranges `(startGlyphID, endGlyphID, startCoverageIndex)`.
//!
//! # ClassDef tables
//!
//! Format 1: dense array starting at `startGlyphID`.
//! Format 2: ranges `(startGlyphID, endGlyphID, class)`.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helper: big-endian u16 / u32 reads
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

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// Parse a Coverage table, returning a sorted list of covered GIDs.
///
/// Returns an empty `Vec` on any parse failure (safe degradation).
pub fn read_coverage(data: &[u8], offset: usize) -> Vec<u16> {
    let data = match data.get(offset..) {
        Some(s) => s,
        None => return vec![],
    };
    let format = match r_u16(data, 0) {
        Some(f) => f,
        None => return vec![],
    };

    match format {
        1 => {
            let count = match r_u16(data, 2) {
                Some(c) => c as usize,
                None => return vec![],
            };
            if data.len() < 4 + count * 2 {
                return vec![];
            }
            let mut gids = Vec::with_capacity(count);
            for i in 0..count {
                if let Some(g) = r_u16(data, 4 + i * 2) {
                    gids.push(g);
                }
            }
            gids
        }
        2 => {
            let range_count = match r_u16(data, 2) {
                Some(c) => c as usize,
                None => return vec![],
            };
            if data.len() < 4 + range_count * 6 {
                return vec![];
            }
            let mut gids = Vec::new();
            for i in 0..range_count {
                let base = 4 + i * 6;
                let start = match r_u16(data, base) {
                    Some(g) => g,
                    None => return vec![],
                };
                let end = match r_u16(data, base + 2) {
                    Some(g) => g,
                    None => return vec![],
                };
                if end < start {
                    return vec![];
                }
                for g in start..=end {
                    gids.push(g);
                }
            }
            gids
        }
        _ => vec![],
    }
}

/// Count consecutive runs in a sorted GID slice.
fn count_runs(gids: &[u16]) -> usize {
    if gids.is_empty() {
        return 0;
    }
    let mut runs = 1usize;
    for w in gids.windows(2) {
        if w[1] != w[0].saturating_add(1) {
            runs += 1;
        }
    }
    runs
}

/// Write a Coverage table for the given sorted GID slice.
///
/// Chooses format 1 vs format 2 based on which encoding is smaller.
/// Format 1 is always used for empty or single-GID inputs.
pub fn write_coverage(gids: &[u16]) -> Vec<u8> {
    let n = gids.len();
    if n <= 1 {
        // Format 1 is always optimal for 0 or 1 GIDs.
        let mut out = Vec::with_capacity(4 + n * 2);
        w_u16(&mut out, 1);
        w_u16(&mut out, n as u16);
        for &g in gids {
            w_u16(&mut out, g);
        }
        return out;
    }

    let runs = count_runs(gids);
    let size_f1 = 4 + n * 2;
    let size_f2 = 4 + runs * 6;

    if size_f1 <= size_f2 {
        // Format 1.
        let mut out = Vec::with_capacity(size_f1);
        w_u16(&mut out, 1);
        w_u16(&mut out, n as u16);
        for &g in gids {
            w_u16(&mut out, g);
        }
        out
    } else {
        // Format 2.
        let mut out = Vec::with_capacity(size_f2);
        w_u16(&mut out, 2);
        w_u16(&mut out, runs as u16);
        let mut cov_idx: u16 = 0;
        let mut run_start = gids[0];
        let mut run_end = gids[0];
        let mut run_cov = cov_idx;
        for &g in &gids[1..] {
            if g == run_end.saturating_add(1) {
                run_end = g;
            } else {
                // Flush current run.
                w_u16(&mut out, run_start);
                w_u16(&mut out, run_end);
                w_u16(&mut out, run_cov);
                cov_idx = cov_idx.wrapping_add(run_end.wrapping_sub(run_start).wrapping_add(1));
                run_start = g;
                run_end = g;
                run_cov = cov_idx;
            }
        }
        // Flush last run.
        w_u16(&mut out, run_start);
        w_u16(&mut out, run_end);
        w_u16(&mut out, run_cov);
        out
    }
}

/// Read coverage, remap GIDs, write new coverage.
///
/// Returns `(new_coverage_bytes, new_gid_list)`.  `new_gid_list` contains the
/// remapped GIDs in their original coverage order (for parallel-array pruning).
/// GIDs not present in `gid_remap` are dropped.
pub fn remap_coverage(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> (Vec<u8>, Vec<u16>) {
    let old_gids = read_coverage(data, offset);
    // Collect new GIDs in coverage-index order, dropping absent ones.
    let new_gids_ordered: Vec<u16> = old_gids
        .iter()
        .filter_map(|g| gid_remap.get(g).copied())
        .collect();
    // write_coverage requires sorted input.
    let mut sorted = new_gids_ordered.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let bytes = write_coverage(&sorted);
    (bytes, new_gids_ordered)
}

// ---------------------------------------------------------------------------
// ClassDef
// ---------------------------------------------------------------------------

/// Parse a ClassDef table, returning a GID → class map.
///
/// GIDs absent from the table have implicit class 0 per the spec.
pub fn read_classdef(data: &[u8], offset: usize) -> HashMap<u16, u16> {
    let data = match data.get(offset..) {
        Some(s) => s,
        None => return HashMap::new(),
    };
    let format = match r_u16(data, 0) {
        Some(f) => f,
        None => return HashMap::new(),
    };

    match format {
        1 => {
            let start_gid = match r_u16(data, 2) {
                Some(g) => g,
                None => return HashMap::new(),
            };
            let count = match r_u16(data, 4) {
                Some(c) => c as usize,
                None => return HashMap::new(),
            };
            if data.len() < 6 + count * 2 {
                return HashMap::new();
            }
            let mut map = HashMap::with_capacity(count);
            for i in 0..count {
                let class = match r_u16(data, 6 + i * 2) {
                    Some(c) => c,
                    None => return HashMap::new(),
                };
                let gid = start_gid.wrapping_add(i as u16);
                if class != 0 {
                    map.insert(gid, class);
                }
            }
            map
        }
        2 => {
            let range_count = match r_u16(data, 2) {
                Some(c) => c as usize,
                None => return HashMap::new(),
            };
            if data.len() < 4 + range_count * 6 {
                return HashMap::new();
            }
            let mut map = HashMap::new();
            for i in 0..range_count {
                let base = 4 + i * 6;
                let start = match r_u16(data, base) {
                    Some(g) => g,
                    None => return HashMap::new(),
                };
                let end = match r_u16(data, base + 2) {
                    Some(g) => g,
                    None => return HashMap::new(),
                };
                let class = match r_u16(data, base + 4) {
                    Some(c) => c,
                    None => return HashMap::new(),
                };
                if end < start {
                    return HashMap::new();
                }
                for g in start..=end {
                    if class != 0 {
                        map.insert(g, class);
                    }
                }
            }
            map
        }
        _ => HashMap::new(),
    }
}

/// Write a ClassDef table for the given GID → class map.
///
/// Chooses format 1 (dense array) when GIDs form a dense-enough range;
/// always falls back to format 2 (ranges).
pub fn write_classdef(map: &HashMap<u16, u16>) -> Vec<u8> {
    if map.is_empty() {
        // Format 2 with 0 ranges.
        let mut out = Vec::with_capacity(4);
        w_u16(&mut out, 2);
        w_u16(&mut out, 0);
        return out;
    }

    let mut gids: Vec<u16> = map.keys().copied().collect();
    gids.sort_unstable();
    let min_gid = gids[0];
    let max_gid = *gids.last().unwrap_or(&gids[0]);
    let dense_count = (max_gid as usize)
        .checked_sub(min_gid as usize)
        .map(|d| d + 1)
        .unwrap_or(1);

    let size_f1 = 6 + dense_count * 2;

    // Format 2 ranges: group consecutive GIDs with same class.
    let ranges = build_classdef_ranges(&gids, map);
    let size_f2 = 4 + ranges.len() * 6;

    if size_f1 <= size_f2 {
        // Format 1.
        let mut out = Vec::with_capacity(size_f1);
        w_u16(&mut out, 1);
        w_u16(&mut out, min_gid);
        w_u16(&mut out, dense_count as u16);
        for i in 0..dense_count {
            let gid = min_gid.wrapping_add(i as u16);
            let class = map.get(&gid).copied().unwrap_or(0);
            w_u16(&mut out, class);
        }
        out
    } else {
        // Format 2.
        let mut out = Vec::with_capacity(size_f2);
        w_u16(&mut out, 2);
        w_u16(&mut out, ranges.len() as u16);
        for (start, end, class) in &ranges {
            w_u16(&mut out, *start);
            w_u16(&mut out, *end);
            w_u16(&mut out, *class);
        }
        out
    }
}

/// Build (start, end, class) ranges from a sorted GID list, merging consecutive
/// GIDs that share the same class value.
fn build_classdef_ranges(gids: &[u16], map: &HashMap<u16, u16>) -> Vec<(u16, u16, u16)> {
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    if gids.is_empty() {
        return ranges;
    }

    let first_class = map.get(&gids[0]).copied().unwrap_or(0);
    let mut run_start = gids[0];
    let mut run_end = gids[0];
    let mut run_class = first_class;

    for &g in &gids[1..] {
        let c = map.get(&g).copied().unwrap_or(0);
        if g == run_end.saturating_add(1) && c == run_class {
            run_end = g;
        } else {
            ranges.push((run_start, run_end, run_class));
            run_start = g;
            run_end = g;
            run_class = c;
        }
    }
    ranges.push((run_start, run_end, run_class));
    ranges
}

/// Read ClassDef, remap GIDs (keeping class values), write new ClassDef.
///
/// GIDs not present in `gid_remap` are dropped from the output.
pub fn remap_classdef(data: &[u8], offset: usize, gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    let old_map = read_classdef(data, offset);
    let new_map: HashMap<u16, u16> = old_map
        .iter()
        .filter_map(|(&old_gid, &class)| gid_remap.get(&old_gid).map(|&new_gid| (new_gid, class)))
        .collect();
    write_classdef(&new_map)
}

// ---------------------------------------------------------------------------
// GDEF sub-structure helpers
// ---------------------------------------------------------------------------

/// Rewrite an AttachList sub-table.
///
/// AttachList = { coverageOffset: Offset16, glyphCount: u16,
///                glyphCount × AttachPoint offsets (Offset16) }
/// Each AttachPoint = { pointCount: u16, pointCount × pointIndex: u16 }
///
/// We remap coverage and prune the parallel array.
fn rewrite_attach_list(
    data: &[u8],
    base_offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(base_offset..)?;
    let cov_offset = r_u16(sub, 0)? as usize;
    let glyph_count = r_u16(sub, 2)? as usize;
    if sub.len() < 4 + glyph_count * 2 {
        return None;
    }

    // Remap coverage; new_gids_ordered tells us which parallel slots survive.
    let (new_cov_bytes, new_gids_ordered) =
        remap_coverage(data, base_offset + cov_offset, gid_remap);
    let new_count = new_gids_ordered.len();

    // Collect old AttachPoint offsets.
    let old_offsets: Vec<usize> = (0..glyph_count)
        .map(|i| r_u16(sub, 4 + i * 2).map(|o| o as usize))
        .collect::<Option<Vec<_>>>()?;

    // Read old coverage to know which slot maps to which parallel entry.
    let old_gids = read_coverage(data, base_offset + cov_offset);
    if old_gids.len() != glyph_count {
        return None;
    }

    // Build index map: old_gid → old slot index.
    let mut old_gid_to_slot: HashMap<u16, usize> = HashMap::with_capacity(glyph_count);
    for (slot, &g) in old_gids.iter().enumerate() {
        old_gid_to_slot.insert(g, slot);
    }

    // For each surviving new GID (in original order), find old slot and copy
    // its AttachPoint data.
    // new_gids_ordered is the list of remapped GIDs in their original cov order.
    // We need to match each back to its old GID to find the old slot.
    // Build reverse of gid_remap for this purpose.
    let rev: HashMap<u16, u16> = gid_remap.iter().map(|(&o, &n)| (n, o)).collect();

    // Collect surviving (new_gid, attach_point_bytes) in new coverage order
    // (sorted by new_gid).
    let mut survivors: Vec<(u16, Vec<u8>)> = Vec::with_capacity(new_count);
    let mut seen_new: std::collections::HashSet<u16> = std::collections::HashSet::new();
    for &new_gid in &new_gids_ordered {
        if !seen_new.insert(new_gid) {
            continue;
        }
        let old_gid = match rev.get(&new_gid) {
            Some(&o) => o,
            None => continue,
        };
        let slot = match old_gid_to_slot.get(&old_gid) {
            Some(&s) => s,
            None => continue,
        };
        let ap_offset = old_offsets[slot];
        let ap_data = sub.get(ap_offset..)?;
        let point_count = r_u16(ap_data, 0)? as usize;
        if ap_data.len() < 2 + point_count * 2 {
            return None;
        }
        let ap_bytes = ap_data.get(..2 + point_count * 2)?.to_vec();
        survivors.push((new_gid, ap_bytes));
    }
    // Sort survivors by new_gid (coverage order).
    survivors.sort_by_key(|(g, _)| *g);
    let final_count = survivors.len() as u16;

    // Layout:
    // offset 0: coverageOffset (Offset16 = 4 + final_count*2)
    // offset 2: glyphCount (u16)
    // offset 4: glyphCount × AttachPoint offsets (Offset16)
    // then coverage bytes
    // then AttachPoint bytes
    let header_size = 4 + final_count as usize * 2;
    let cov_start_in_sub = header_size as u16;

    let mut out = Vec::new();
    w_u16(&mut out, cov_start_in_sub);
    w_u16(&mut out, final_count);

    // Placeholder offsets — will patch below.
    let offsets_start = out.len();
    for _ in 0..final_count {
        w_u16(&mut out, 0);
    }
    // Coverage data immediately after header.
    out.extend_from_slice(&new_cov_bytes);

    // AttachPoint data; record offsets relative to AttachList start.
    let mut ap_offsets: Vec<u16> = Vec::with_capacity(final_count as usize);
    for (_, ap_bytes) in &survivors {
        ap_offsets.push(out.len() as u16);
        out.extend_from_slice(ap_bytes);
    }
    // Patch offsets.
    for (i, &ap_off) in ap_offsets.iter().enumerate() {
        let pos = offsets_start + i * 2;
        out[pos] = (ap_off >> 8) as u8;
        out[pos + 1] = (ap_off & 0xFF) as u8;
    }

    Some(out)
}

/// Rewrite a LigCaretList sub-table.
///
/// LigCaretList = { coverageOffset: Offset16, ligGlyphCount: u16,
///                  ligGlyphCount × LigGlyph offsets (Offset16) }
/// LigGlyph = { caretCount: u16, caretCount × CaretValue offsets (Offset16) }
/// CaretValue: variable-size; we copy bytes verbatim per entry.
fn rewrite_lig_caret_list(
    data: &[u8],
    base_offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(base_offset..)?;
    let cov_offset = r_u16(sub, 0)? as usize;
    let lig_count = r_u16(sub, 2)? as usize;
    if sub.len() < 4 + lig_count * 2 {
        return None;
    }

    let (new_cov_bytes, new_gids_ordered) =
        remap_coverage(data, base_offset + cov_offset, gid_remap);

    let old_gids = read_coverage(data, base_offset + cov_offset);
    if old_gids.len() != lig_count {
        return None;
    }

    let old_offsets: Vec<usize> = (0..lig_count)
        .map(|i| r_u16(sub, 4 + i * 2).map(|o| o as usize))
        .collect::<Option<Vec<_>>>()?;

    let mut old_gid_to_slot: HashMap<u16, usize> = HashMap::with_capacity(lig_count);
    for (slot, &g) in old_gids.iter().enumerate() {
        old_gid_to_slot.insert(g, slot);
    }

    let rev: HashMap<u16, u16> = gid_remap.iter().map(|(&o, &n)| (n, o)).collect();

    let mut survivors: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut seen_new: std::collections::HashSet<u16> = std::collections::HashSet::new();
    for &new_gid in &new_gids_ordered {
        if !seen_new.insert(new_gid) {
            continue;
        }
        let old_gid = match rev.get(&new_gid) {
            Some(&o) => o,
            None => continue,
        };
        let slot = match old_gid_to_slot.get(&old_gid) {
            Some(&s) => s,
            None => continue,
        };
        let lg_offset = old_offsets[slot];
        let lg_data = sub.get(lg_offset..)?;
        let caret_count = r_u16(lg_data, 0)? as usize;
        if lg_data.len() < 2 + caret_count * 2 {
            return None;
        }
        // Collect per-caret offsets and their data.
        let mut lig_glyph_out: Vec<u8> = Vec::new();
        // CaretValue offsets within LigGlyph — relative to LigGlyph start.
        let cv_offsets: Vec<usize> = (0..caret_count)
            .map(|j| r_u16(lg_data, 2 + j * 2).map(|o| o as usize))
            .collect::<Option<Vec<_>>>()?;
        // Compute size of each CaretValue (must be CaretValueFormat 1/2/3).
        // Format 1/2: 4 bytes. Format 3: 4 + DeviceTable header (variable).
        // Safe approach: determine end of each by taking next offset or end-of-sub.
        let mut cv_bytes_list: Vec<Vec<u8>> = Vec::with_capacity(caret_count);
        for j in 0..caret_count {
            let start = cv_offsets[j];
            let end = if j + 1 < caret_count {
                cv_offsets[j + 1]
            } else {
                // End is next LigGlyph or end of LigCaretList sub.
                // Use remaining bytes — copy up to next LigGlyph or sub end.
                // Conservative: use slot+1's offset if available, else sub.len().
                if slot + 1 < lig_count {
                    old_offsets[slot + 1]
                } else {
                    sub.len()
                }
            };
            if end < start || end > sub.len() {
                return None;
            }
            // Rebase: CaretValue offsets are relative to LigGlyph, which is at lg_offset from sub start.
            let abs_start = lg_offset + start;
            let abs_end = lg_offset + end;
            if abs_end > sub.len() {
                return None;
            }
            cv_bytes_list.push(sub.get(abs_start..abs_end)?.to_vec());
        }

        // Serialize LigGlyph.
        // caretCount, then caret offsets, then caret data.
        w_u16(&mut lig_glyph_out, caret_count as u16);
        let data_start = 2 + caret_count * 2;
        let mut running = data_start;
        for cv in &cv_bytes_list {
            w_u16(&mut lig_glyph_out, running as u16);
            running += cv.len();
        }
        for cv in &cv_bytes_list {
            lig_glyph_out.extend_from_slice(cv);
        }

        survivors.push((new_gid, lig_glyph_out));
    }
    survivors.sort_by_key(|(g, _)| *g);
    let final_count = survivors.len() as u16;

    let header_size = 4 + final_count as usize * 2;
    let cov_start_in_sub = header_size as u16;

    let mut out = Vec::new();
    w_u16(&mut out, cov_start_in_sub);
    w_u16(&mut out, final_count);
    let offsets_start = out.len();
    for _ in 0..final_count {
        w_u16(&mut out, 0);
    }
    out.extend_from_slice(&new_cov_bytes);

    let mut lg_offsets: Vec<u16> = Vec::with_capacity(final_count as usize);
    for (_, lg_bytes) in &survivors {
        lg_offsets.push(out.len() as u16);
        out.extend_from_slice(lg_bytes);
    }
    for (i, &lg_off) in lg_offsets.iter().enumerate() {
        let pos = offsets_start + i * 2;
        out[pos] = (lg_off >> 8) as u8;
        out[pos + 1] = (lg_off & 0xFF) as u8;
    }

    Some(out)
}

/// Rewrite a MarkGlyphSetsDef sub-table.
///
/// MarkGlyphSetsDef = { format: u16, markSetCount: u16,
///                      markSetCount × Offset32 to Coverage tables }
fn rewrite_mark_glyph_sets(
    data: &[u8],
    base_offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(base_offset..)?;
    let _format = r_u16(sub, 0)?; // should be 1
    let mark_set_count = r_u16(sub, 2)? as usize;
    if sub.len() < 4 + mark_set_count * 4 {
        return None;
    }

    // Read each Offset32 to a Coverage table.
    let offsets: Vec<u32> = (0..mark_set_count)
        .map(|i| r_u32(sub, 4 + i * 4))
        .collect::<Option<Vec<_>>>()?;

    // Remap each coverage.
    let mut new_covs: Vec<Vec<u8>> = Vec::with_capacity(mark_set_count);
    for &cov_off in &offsets {
        let (new_cov, _) = remap_coverage(data, base_offset + cov_off as usize, gid_remap);
        new_covs.push(new_cov);
    }

    // Layout: format(2) + markSetCount(2) + markSetCount×Offset32(4 each) + coverage data.
    let header_size = 4 + mark_set_count * 4;
    let mut out = Vec::with_capacity(header_size + new_covs.iter().map(|c| c.len()).sum::<usize>());
    w_u16(&mut out, 1); // format
    w_u16(&mut out, mark_set_count as u16);

    // Placeholder Offset32s.
    let offsets_start = out.len();
    for _ in 0..mark_set_count {
        w_u32(&mut out, 0);
    }

    let mut cov_off32s: Vec<u32> = Vec::with_capacity(mark_set_count);
    for cov in &new_covs {
        cov_off32s.push(out.len() as u32);
        out.extend_from_slice(cov);
    }
    // Patch Offset32s.
    for (i, &off32) in cov_off32s.iter().enumerate() {
        let pos = offsets_start + i * 4;
        let bytes = off32.to_be_bytes();
        out[pos..pos + 4].copy_from_slice(&bytes);
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// GDEF
// ---------------------------------------------------------------------------

/// Rewrite a GDEF table, remapping all GID references.
///
/// On any parse failure, returns the original table verbatim.
pub fn rewrite_gdef(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    match try_rewrite_gdef(table, gid_remap) {
        Some(v) => v,
        None => table.to_vec(),
    }
}

/// Inner fallible implementation; returns `None` on any parse failure.
fn try_rewrite_gdef(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    if table.len() < 12 {
        return None;
    }
    let major = r_u16(table, 0)?;
    let minor = r_u16(table, 2)?;

    if major != 1 {
        return None;
    }

    // Read all Offset16 fields from the header.
    let glyph_class_off = r_u16(table, 4)? as usize;
    let attach_list_off = r_u16(table, 6)? as usize;
    let lig_caret_off = r_u16(table, 8)? as usize;
    let mark_attach_off = r_u16(table, 10)? as usize;

    // v1.2+ has MarkGlyphSetsDef at offset 12 (Offset16).
    let (mark_sets_off, item_var_store_off) = if minor >= 2 {
        if table.len() < 14 {
            return None;
        }
        let mgs = r_u16(table, 12)? as usize;
        // v1.3+ has ItemVarStore at offset 14 (Offset32).
        let ivs = if minor >= 3 {
            if table.len() < 18 {
                return None;
            }
            r_u32(table, 14)? as usize
        } else {
            0
        };
        (mgs, ivs)
    } else {
        (0usize, 0usize)
    };

    // Rewrite GlyphClassDef.
    let new_glyph_class: Option<Vec<u8>> = if glyph_class_off != 0 {
        Some(remap_classdef(table, glyph_class_off, gid_remap))
    } else {
        None
    };

    // Rewrite MarkAttachClassDef.
    let new_mark_attach: Option<Vec<u8>> = if mark_attach_off != 0 {
        Some(remap_classdef(table, mark_attach_off, gid_remap))
    } else {
        None
    };

    // Rewrite AttachList.
    let new_attach_list: Option<Vec<u8>> = if attach_list_off != 0 {
        Some(rewrite_attach_list(table, attach_list_off, gid_remap)?)
    } else {
        None
    };

    // Rewrite LigCaretList.
    let new_lig_caret: Option<Vec<u8>> = if lig_caret_off != 0 {
        Some(rewrite_lig_caret_list(table, lig_caret_off, gid_remap)?)
    } else {
        None
    };

    // Rewrite MarkGlyphSetsDef.
    let new_mark_sets: Option<Vec<u8>> = if minor >= 2 && mark_sets_off != 0 {
        Some(rewrite_mark_glyph_sets(table, mark_sets_off, gid_remap)?)
    } else {
        None
    };

    // ItemVarStore: copy verbatim if present (v1.3+).
    let ivs_bytes: Option<Vec<u8>> = if minor >= 3 && item_var_store_off != 0 {
        let ivs_data = table.get(item_var_store_off..)?;
        Some(ivs_data.to_vec())
    } else {
        None
    };

    // Compute header size.
    let header_size: usize = if minor >= 3 {
        18 // 4 (version) + 4×Offset16 + Offset16 + Offset32
    } else if minor >= 2 {
        14 // 4 + 4×Offset16 + Offset16
    } else {
        12 // 4 + 4×Offset16
    };

    // Build output: header placeholder, then sub-tables.
    let mut out = Vec::new();
    // Version.
    w_u16(&mut out, major);
    w_u16(&mut out, minor);
    // Placeholder Offset16s for GlyphClassDef, AttachList, LigCaretList,
    // MarkAttachClassDef, and optionally MarkGlyphSetsDef and ItemVarStore.
    let header_offsets_start = out.len();
    let num_offset16s = if minor >= 2 { 5 } else { 4 };
    for _ in 0..num_offset16s {
        w_u16(&mut out, 0);
    }
    if minor >= 3 {
        w_u32(&mut out, 0); // ItemVarStore placeholder
    }
    assert_eq!(out.len(), header_size);

    // Helper to write a sub-table and record its offset.
    macro_rules! append_sub16 {
        ($opt:expr) => {{
            if let Some(ref data) = $opt {
                let off = out.len() as u16;
                out.extend_from_slice(data);
                off
            } else {
                0u16
            }
        }};
    }

    let new_glyph_class_off16 = append_sub16!(new_glyph_class);
    let new_attach_off16 = append_sub16!(new_attach_list);
    let new_lig_caret_off16 = append_sub16!(new_lig_caret);
    let new_mark_attach_off16 = append_sub16!(new_mark_attach);
    let new_mark_sets_off16 = if minor >= 2 {
        append_sub16!(new_mark_sets)
    } else {
        0
    };
    let new_ivs_off32: u32 = if minor >= 3 {
        if let Some(ref data) = ivs_bytes {
            let off = out.len() as u32;
            out.extend_from_slice(data);
            off
        } else {
            0
        }
    } else {
        0
    };

    // Patch header offsets.
    let patch = |out: &mut Vec<u8>, pos: usize, val: u16| {
        out[pos] = (val >> 8) as u8;
        out[pos + 1] = (val & 0xFF) as u8;
    };
    let base = header_offsets_start;
    patch(&mut out, base, new_glyph_class_off16);
    patch(&mut out, base + 2, new_attach_off16);
    patch(&mut out, base + 4, new_lig_caret_off16);
    patch(&mut out, base + 6, new_mark_attach_off16);
    if minor >= 2 {
        patch(&mut out, base + 8, new_mark_sets_off16);
    }
    if minor >= 3 {
        let ivs_pos = base + 10;
        let bytes = new_ivs_off32.to_be_bytes();
        out[ivs_pos..ivs_pos + 4].copy_from_slice(&bytes);
    }

    Some(out)
}
