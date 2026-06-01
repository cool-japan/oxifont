//! OpenType Layout (OTL) table rewriters: GSUB and GPOS.
//!
//! Implements the Script/Feature/Lookup (SFL) chain rewriter and GSUB/GPOS subtable
//! handlers. After subsetting the glyph set, all GID references inside GSUB/GPOS are
//! remapped through `gid_remap`; lookups / features / scripts whose GIDs
//! disappear entirely are dropped.
//!
//! # Supported GSUB lookup types
//! - Type 1: SingleSubst (Format 1 and 2)
//! - Type 2: MultipleSubst (Format 1)
//! - Type 3: AlternateSubst (Format 1)
//! - Type 4: LigatureSubst (Format 1)
//! - Type 7: ExtensionSubst (Format 1) — recursively rewrites the inner subtype
//! - Types 5, 6, 8: dropped (return `None`); parent lookup is dropped if all
//!   subtables drop.
//!
//! # GSUB/GPOS v1.1 / FeatureVariations
//! When the input has `minorVersion == 1` the FeatureVariations block references
//! feature indices that would become stale after SFL rewriting. The rebuilt table
//! is emitted with `minorVersion = 0` and the FeatureVariations block is dropped.
//! (Full FeatureVariations rewrite is deferred.)

use crate::layout::{read_coverage, write_coverage};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Internal big-endian helpers (mirror the private ones in layout.rs)
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
// GSUB Subtable handlers
// ---------------------------------------------------------------------------

/// Type 1 — SingleSubst
///
/// Format 1: delta-based. We always convert to Format 2 after remapping
/// because a consistent delta is not guaranteed to survive the GID remap.
///
/// Format 2: explicit (old_gid, substitute_gid) parallel arrays.
fn rewrite_single_subst(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    let cov_offset = r_u16(sub, 2)? as usize;

    match format {
        1 => {
            // Format 1: substitute = (gid + deltaGlyphID) mod 65536
            let delta = r_u16(sub, 4)? as i32;
            let old_gids = read_coverage(data, offset + cov_offset);
            if old_gids.is_empty() {
                return None;
            }

            // Convert to pairs; filter by both gid_remap presence.
            let mut pairs: Vec<(u16, u16)> = Vec::new();
            for &old_gid in &old_gids {
                let new_gid = *gid_remap.get(&old_gid)?;
                let old_subst = ((old_gid as i32 + delta) & 0xFFFF) as u16;
                let new_subst = *gid_remap.get(&old_subst)?;
                pairs.push((new_gid, new_subst));
            }

            if pairs.is_empty() {
                return None;
            }

            // Sort by new_gid for coverage ordering.
            pairs.sort_unstable_by_key(|&(g, _)| g);
            pairs.dedup_by_key(|p| p.0);
            emit_single_subst_f2(&pairs)
        }
        2 => {
            // Format 2: explicit substitute array.
            let glyph_count = r_u16(sub, 4)? as usize;
            if sub.len() < 6 + glyph_count * 2 {
                return None;
            }
            let old_gids = read_coverage(data, offset + cov_offset);
            if old_gids.len() != glyph_count {
                return None;
            }

            // Build (new_gid, new_substitute) pairs.
            let mut pairs: Vec<(u16, u16)> = Vec::new();
            for (i, &old_gid) in old_gids.iter().enumerate() {
                let new_gid = match gid_remap.get(&old_gid) {
                    Some(&g) => g,
                    None => continue,
                };
                let old_subst = r_u16(sub, 6 + i * 2)?;
                let new_subst = match gid_remap.get(&old_subst) {
                    Some(&g) => g,
                    None => continue,
                };
                pairs.push((new_gid, new_subst));
            }

            if pairs.is_empty() {
                return None;
            }

            // Sort by new_gid for coverage ordering.
            pairs.sort_unstable_by_key(|&(g, _)| g);
            pairs.dedup_by_key(|p| p.0);
            emit_single_subst_f2(&pairs)
        }
        _ => None,
    }
}

/// Emit a SingleSubst Format 2 from sorted (new_gid, new_subst) pairs.
///
/// Per spec layout:
/// offset 0: format (2)
/// offset 2: coverageOffset (2) — points to Coverage table
/// offset 4: glyphCount (2)
/// offset 6: substituteGlyphIDs[glyphCount] (2 each)
/// offset 6 + glyphCount*2: Coverage table
fn emit_single_subst_f2(pairs: &[(u16, u16)]) -> Option<Vec<u8>> {
    if pairs.is_empty() {
        return None;
    }
    let glyph_count = pairs.len() as u16;
    let cov_gids: Vec<u16> = pairs.iter().map(|&(g, _)| g).collect();
    let cov_bytes = write_coverage(&cov_gids);

    // Coverage starts after: format(2) + coverageOffset(2) + glyphCount(2) + substituteGlyphIDs(n*2)
    let cov_offset = 6u16 + glyph_count * 2;

    let mut out = Vec::with_capacity(6 + pairs.len() * 2 + cov_bytes.len());
    w_u16(&mut out, 2); // format
    w_u16(&mut out, cov_offset);
    w_u16(&mut out, glyph_count);
    // SubstituteGlyphIDs at offset 6 — aligned with coverage order (pairs already sorted).
    for &(_, subst) in pairs {
        w_u16(&mut out, subst);
    }
    // Coverage table immediately after.
    out.extend_from_slice(&cov_bytes);
    Some(out)
}

/// Type 2 — MultipleSubst Format 1
fn rewrite_multiple_subst(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    if format != 1 {
        return None;
    }
    let cov_offset = r_u16(sub, 2)? as usize;
    let seq_count = r_u16(sub, 4)? as usize;
    if sub.len() < 6 + seq_count * 2 {
        return None;
    }

    let old_gids = read_coverage(data, offset + cov_offset);
    if old_gids.len() != seq_count {
        return None;
    }

    // Collect surviving (new_gid, sequence_bytes) pairs.
    let mut pairs: Vec<(u16, Vec<u16>)> = Vec::new();
    for (i, &old_gid) in old_gids.iter().enumerate() {
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        let seq_off = r_u16(sub, 6 + i * 2)? as usize;
        let seq_data = sub.get(seq_off..)?;
        let glyph_count = r_u16(seq_data, 0)? as usize;
        if seq_data.len() < 2 + glyph_count * 2 {
            return None;
        }

        let mut new_seq: Vec<u16> = Vec::with_capacity(glyph_count);
        let mut all_mapped = true;
        for j in 0..glyph_count {
            let old_subst = r_u16(seq_data, 2 + j * 2)?;
            match gid_remap.get(&old_subst) {
                Some(&g) => new_seq.push(g),
                None => {
                    all_mapped = false;
                    break;
                }
            }
        }
        if all_mapped {
            pairs.push((new_gid, new_seq));
        }
    }

    if pairs.is_empty() {
        return None;
    }

    // Sort by new_gid.
    pairs.sort_unstable_by_key(|&(g, _)| g);
    pairs.dedup_by_key(|p| p.0);

    // Serialize: header + sequence-offset array + coverage + sequence data.
    let new_count = pairs.len();
    let cov_gids: Vec<u16> = pairs.iter().map(|&(g, _)| g).collect();
    let cov_bytes = write_coverage(&cov_gids);

    // Header: format(2) + coverageOffset(2) + sequenceCount(2) +
    //         sequenceOffsets[new_count](2 each)
    let header_size = 6 + new_count * 2;
    let cov_start_off = header_size as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    w_u16(&mut out, cov_start_off);
    w_u16(&mut out, new_count as u16);

    // Placeholder offsets for sequences.
    let offsets_pos = out.len();
    for _ in 0..new_count {
        w_u16(&mut out, 0);
    }
    // Coverage bytes.
    out.extend_from_slice(&cov_bytes);

    // Sequence data.
    let mut seq_offs: Vec<u16> = Vec::with_capacity(new_count);
    for (_, seq) in &pairs {
        seq_offs.push(out.len() as u16);
        w_u16(&mut out, seq.len() as u16);
        for &g in seq {
            w_u16(&mut out, g);
        }
    }

    // Patch offsets.
    for (i, &off) in seq_offs.iter().enumerate() {
        patch_u16(&mut out, offsets_pos + i * 2, off);
    }

    Some(out)
}

/// Type 3 — AlternateSubst Format 1
fn rewrite_alternate_subst(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    if format != 1 {
        return None;
    }
    let cov_offset = r_u16(sub, 2)? as usize;
    let alt_set_count = r_u16(sub, 4)? as usize;
    if sub.len() < 6 + alt_set_count * 2 {
        return None;
    }

    let old_gids = read_coverage(data, offset + cov_offset);
    if old_gids.len() != alt_set_count {
        return None;
    }

    let mut pairs: Vec<(u16, Vec<u16>)> = Vec::new();
    for (i, &old_gid) in old_gids.iter().enumerate() {
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        let alt_off = r_u16(sub, 6 + i * 2)? as usize;
        let alt_data = sub.get(alt_off..)?;
        let glyph_count = r_u16(alt_data, 0)? as usize;
        if alt_data.len() < 2 + glyph_count * 2 {
            return None;
        }

        let mut new_alts: Vec<u16> = Vec::with_capacity(glyph_count);
        let mut all_mapped = true;
        for j in 0..glyph_count {
            let old_alt = r_u16(alt_data, 2 + j * 2)?;
            match gid_remap.get(&old_alt) {
                Some(&g) => new_alts.push(g),
                None => {
                    all_mapped = false;
                    break;
                }
            }
        }
        if all_mapped && !new_alts.is_empty() {
            pairs.push((new_gid, new_alts));
        }
    }

    if pairs.is_empty() {
        return None;
    }

    pairs.sort_unstable_by_key(|&(g, _)| g);
    pairs.dedup_by_key(|p| p.0);

    let new_count = pairs.len();
    let cov_gids: Vec<u16> = pairs.iter().map(|&(g, _)| g).collect();
    let cov_bytes = write_coverage(&cov_gids);

    let header_size = 6 + new_count * 2;
    let cov_start_off = header_size as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    w_u16(&mut out, cov_start_off);
    w_u16(&mut out, new_count as u16);

    let offsets_pos = out.len();
    for _ in 0..new_count {
        w_u16(&mut out, 0);
    }
    out.extend_from_slice(&cov_bytes);

    let mut alt_offs: Vec<u16> = Vec::with_capacity(new_count);
    for (_, alts) in &pairs {
        alt_offs.push(out.len() as u16);
        w_u16(&mut out, alts.len() as u16);
        for &g in alts {
            w_u16(&mut out, g);
        }
    }
    for (i, &off) in alt_offs.iter().enumerate() {
        patch_u16(&mut out, offsets_pos + i * 2, off);
    }

    Some(out)
}

/// A single ligature entry: (output_glyph, additional_component_gids).
type LigatureEntry = (u16, Vec<u16>);

/// Type 4 — LigatureSubst Format 1
fn rewrite_ligature_subst(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    let sub = data.get(offset..)?;
    let format = r_u16(sub, 0)?;
    if format != 1 {
        return None;
    }
    let cov_offset = r_u16(sub, 2)? as usize;
    let lig_set_count = r_u16(sub, 4)? as usize;
    if sub.len() < 6 + lig_set_count * 2 {
        return None;
    }

    let old_gids = read_coverage(data, offset + cov_offset);
    if old_gids.len() != lig_set_count {
        return None;
    }

    // Each surviving entry: (new_first_gid, Vec<LigatureEntry>)
    let mut sets: Vec<(u16, Vec<LigatureEntry>)> = Vec::new();

    for (i, &old_gid) in old_gids.iter().enumerate() {
        let new_first_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        let ls_off = r_u16(sub, 6 + i * 2)? as usize;
        let ls_data = sub.get(ls_off..)?;
        let lig_count = r_u16(ls_data, 0)? as usize;
        if ls_data.len() < 2 + lig_count * 2 {
            return None;
        }

        let mut surviving_ligs: Vec<(u16, Vec<u16>)> = Vec::new();
        for j in 0..lig_count {
            let lig_off = r_u16(ls_data, 2 + j * 2)? as usize;
            let lig_data = ls_data.get(lig_off..)?;
            let lig_glyph = r_u16(lig_data, 0)?;
            let component_count = r_u16(lig_data, 2)? as usize;
            // componentCount includes the first glyph; additional components = componentCount-1
            let extra = component_count.saturating_sub(1);
            if lig_data.len() < 4 + extra * 2 {
                return None;
            }

            let new_lig = match gid_remap.get(&lig_glyph) {
                Some(&g) => g,
                None => continue,
            };

            let mut new_comps: Vec<u16> = Vec::with_capacity(extra);
            let mut all_mapped = true;
            for k in 0..extra {
                let comp = r_u16(lig_data, 4 + k * 2)?;
                match gid_remap.get(&comp) {
                    Some(&g) => new_comps.push(g),
                    None => {
                        all_mapped = false;
                        break;
                    }
                }
            }
            if all_mapped {
                surviving_ligs.push((new_lig, new_comps));
            }
        }

        if !surviving_ligs.is_empty() {
            sets.push((new_first_gid, surviving_ligs));
        }
    }

    if sets.is_empty() {
        return None;
    }

    // Sort by new_first_gid for coverage ordering.
    sets.sort_unstable_by_key(|&(g, _)| g);
    sets.dedup_by_key(|s| s.0);

    let new_count = sets.len();
    let cov_gids: Vec<u16> = sets.iter().map(|&(g, _)| g).collect();
    let cov_bytes = write_coverage(&cov_gids);

    // Layout: format(2) + coverageOffset(2) + ligSetCount(2) +
    //         ligSetOffsets[n](2*n) | coverage | LigatureSet... | Ligature...
    let header_size = 6 + new_count * 2;
    let cov_start_off = header_size as u16;

    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    w_u16(&mut out, cov_start_off);
    w_u16(&mut out, new_count as u16);

    let ls_offsets_pos = out.len();
    for _ in 0..new_count {
        w_u16(&mut out, 0);
    }
    out.extend_from_slice(&cov_bytes);

    let mut ls_offs: Vec<u16> = Vec::with_capacity(new_count);
    for (_, ligs) in &sets {
        ls_offs.push(out.len() as u16);
        // LigatureSet: ligCount(2) + ligOffsets[ligCount](2*each)
        let lc = ligs.len();
        let ls_header_size = 2 + lc * 2;
        let lig_offsets_abs_pos = out.len() + 2; // after the ligCount u16
        w_u16(&mut out, lc as u16);
        // Placeholder lig offsets within LigatureSet — relative to LigatureSet start.
        let ls_start = ls_offs.last().copied().unwrap_or(0) as usize;
        let lig_off_base = out.len();
        for _ in 0..lc {
            w_u16(&mut out, 0);
        }
        // Ligature data.
        let mut lig_offs_rel: Vec<u16> = Vec::with_capacity(lc);
        for (new_lig, comps) in ligs {
            // Offset relative to LigatureSet start.
            lig_offs_rel.push((out.len() - ls_start) as u16);
            w_u16(&mut out, *new_lig);
            // componentCount = extra components + 1
            w_u16(&mut out, (comps.len() + 1) as u16);
            for &comp in comps {
                w_u16(&mut out, comp);
            }
        }
        // Patch lig offsets.
        for (k, &off) in lig_offs_rel.iter().enumerate() {
            patch_u16(&mut out, lig_off_base + k * 2, off);
        }
        // Suppress unused variable warning.
        let _ = (ls_header_size, lig_offsets_abs_pos);
    }

    // Patch LigatureSet offsets.
    for (i, &off) in ls_offs.iter().enumerate() {
        patch_u16(&mut out, ls_offsets_pos + i * 2, off);
    }

    Some(out)
}

/// Type 7 — ExtensionSubst Format 1
///
/// Recursively rewrites the inner subtable for `extensionLookupType`.
/// If `extensionLookupType` is 7 (which the spec forbids), return `None`.
fn rewrite_extension_subst(
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
    if ext_type == 7 {
        // Spec forbids Extension wrapping Extension.
        return None;
    }
    let ext_offset = r_u32(sub, 4)? as usize;

    // Inner subtable absolute offset within `data`.
    let inner_abs = offset + ext_offset;
    let inner_bytes = rewrite_gsub_subtable(data, inner_abs, ext_type, gid_remap)?;

    // Rebuild extension wrapper: format(2) + extLookupType(2) + extOffset(Offset32=8)
    // Inner subtable follows immediately.
    let inner_offset: u32 = 8; // header is 8 bytes
    let mut out = Vec::with_capacity(8 + inner_bytes.len());
    w_u16(&mut out, 1); // format
    w_u16(&mut out, ext_type);
    w_u32(&mut out, inner_offset);
    out.extend_from_slice(&inner_bytes);
    Some(out)
}

/// Dispatch to the correct GSUB subtable handler by `lookup_type`.
///
/// Returns `None` if:
/// - the lookup type is unsupported (5, 6, 8),
/// - the subtable is malformed, or
/// - all GIDs in the subtable were removed from the subset.
pub fn rewrite_gsub_subtable(
    data: &[u8],
    offset: usize,
    lookup_type: u16,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<u8>> {
    match lookup_type {
        1 => rewrite_single_subst(data, offset, gid_remap),
        2 => rewrite_multiple_subst(data, offset, gid_remap),
        3 => rewrite_alternate_subst(data, offset, gid_remap),
        4 => rewrite_ligature_subst(data, offset, gid_remap),
        // Types 5 (ContextSubst), 6 (ChainContextSubst), 8 (ReverseChain) — drop.
        5 | 6 | 8 => None,
        7 => rewrite_extension_subst(data, offset, gid_remap),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// LookupList parsing and rewriting
// ---------------------------------------------------------------------------

/// Representation of a rewritten lookup: the raw bytes and flags for
/// reconstruction.
struct RewrittenLookup {
    lookup_type: u16,
    lookup_flag: u16,
    mark_filtering_set: Option<u16>,
    subtable_bytes: Vec<Vec<u8>>,
}

/// Parse a LookupList, rewrite subtables via `dispatch`, and return:
/// - the bytes for the new LookupList (and embedded lookups/subtables),
/// - the index map `old_lookup_idx -> Option<new_idx>`.
///
/// A lookup is dropped if all its subtables return `None`.
///
/// `dispatch` is called with `(table, abs_subtable_offset, lookup_type, gid_remap)`
/// and should return the rewritten subtable bytes or `None` to drop.
pub(crate) fn rewrite_lookup_list_with<F>(
    table: &[u8],
    ll_offset: usize,
    gid_remap: &HashMap<u16, u16>,
    dispatch: F,
) -> (Vec<u8>, Vec<Option<u16>>)
where
    F: Fn(&[u8], usize, u16, &HashMap<u16, u16>) -> Option<Vec<u8>>,
{
    let ll_data = match table.get(ll_offset..) {
        Some(d) => d,
        None => return (build_empty_lookup_list(), vec![]),
    };

    let lookup_count = match r_u16(ll_data, 0) {
        Some(c) => c as usize,
        None => return (build_empty_lookup_list(), vec![]),
    };

    if ll_data.len() < 2 + lookup_count * 2 {
        return (build_empty_lookup_list(), vec![]);
    }

    let mut index_map: Vec<Option<u16>> = Vec::with_capacity(lookup_count);
    let mut rewritten: Vec<RewrittenLookup> = Vec::new();

    for i in 0..lookup_count {
        let raw_off = match r_u16(ll_data, 2 + i * 2) {
            Some(o) => o as usize,
            None => {
                index_map.push(None);
                continue;
            }
        };
        // Lookup offset is relative to LookupList start.
        let lk_data = match ll_data.get(raw_off..) {
            Some(d) => d,
            None => {
                index_map.push(None);
                continue;
            }
        };

        let lookup_type = match r_u16(lk_data, 0) {
            Some(t) => t,
            None => {
                index_map.push(None);
                continue;
            }
        };
        let lookup_flag = match r_u16(lk_data, 2) {
            Some(f) => f,
            None => {
                index_map.push(None);
                continue;
            }
        };
        let sub_count = match r_u16(lk_data, 4) {
            Some(c) => c as usize,
            None => {
                index_map.push(None);
                continue;
            }
        };

        if lk_data.len() < 6 + sub_count * 2 {
            index_map.push(None);
            continue;
        }

        // UseMarkFilteringSet (bit 4 of high byte = 0x0010).
        const USE_MARK_FILTERING_SET: u16 = 0x0010;
        let mark_filtering_set = if lookup_flag & USE_MARK_FILTERING_SET != 0 {
            r_u16(lk_data, 6 + sub_count * 2)
        } else {
            None
        };

        let mut surviving_subtables: Vec<Vec<u8>> = Vec::new();
        for j in 0..sub_count {
            let st_off = match r_u16(lk_data, 6 + j * 2) {
                Some(o) => o as usize,
                None => continue,
            };
            // Subtable offset is relative to Lookup start; absolute = ll_offset + raw_off + st_off.
            let abs_st_off = ll_offset + raw_off + st_off;
            if let Some(st_bytes) = dispatch(table, abs_st_off, lookup_type, gid_remap) {
                surviving_subtables.push(st_bytes);
            }
        }

        if surviving_subtables.is_empty() {
            index_map.push(None);
        } else {
            let new_idx = rewritten.len() as u16;
            index_map.push(Some(new_idx));
            rewritten.push(RewrittenLookup {
                lookup_type,
                lookup_flag,
                mark_filtering_set,
                subtable_bytes: surviving_subtables,
            });
        }
    }

    let ll_bytes = build_lookup_list_bytes(&rewritten);
    (ll_bytes, index_map)
}

/// Parse the GSUB LookupList, rewrite subtables, and return:
/// - the bytes for the new LookupList (and embedded lookups/subtables),
/// - the index map `old_lookup_idx -> Option<new_idx>`.
///
/// A lookup is dropped if all its subtables return `None`.
fn rewrite_lookup_list(
    table: &[u8],
    ll_offset: usize,
    gid_remap: &HashMap<u16, u16>,
) -> (Vec<u8>, Vec<Option<u16>>) {
    rewrite_lookup_list_with(table, ll_offset, gid_remap, rewrite_gsub_subtable)
}

/// Build an empty LookupList (lookupCount = 0).
fn build_empty_lookup_list() -> Vec<u8> {
    let mut out = Vec::with_capacity(2);
    w_u16(&mut out, 0); // lookupCount
    out
}

/// Serialize a list of rewritten lookups into LookupList bytes.
///
/// Layout:
/// ```text
/// LookupList:
///   lookupCount: u16
///   lookupOffsets: [u16; lookupCount]   (from LookupList start)
///
/// Lookup (immediately after LookupList header):
///   lookupType: u16
///   lookupFlag: u16
///   subTableCount: u16
///   subtableOffsets: [u16; subTableCount]  (from Lookup start)
///   [markFilteringSet: u16]
///   subtable data...
/// ```
fn build_lookup_list_bytes(lookups: &[RewrittenLookup]) -> Vec<u8> {
    let n = lookups.len();
    // LookupList header: 2 + n*2
    let ll_header_size = 2 + n * 2;

    // Phase 1: compute sizes and layout.
    // We'll build all Lookup blobs, then lay out the LookupList.
    let mut lookup_blobs: Vec<Vec<u8>> = Vec::with_capacity(n);
    for lk in lookups {
        lookup_blobs.push(build_lookup_bytes(lk));
    }

    let mut out: Vec<u8> = Vec::new();
    w_u16(&mut out, n as u16); // lookupCount

    // Placeholder lookup offsets (relative to LookupList start).
    let lk_offsets_pos = out.len();
    for _ in 0..n {
        w_u16(&mut out, 0);
    }
    assert_eq!(out.len(), ll_header_size);

    // Append lookup blobs and record their offsets.
    let mut lk_offs: Vec<u16> = Vec::with_capacity(n);
    for blob in &lookup_blobs {
        lk_offs.push(out.len() as u16);
        out.extend_from_slice(blob);
    }

    // Patch lookup offsets.
    for (i, &off) in lk_offs.iter().enumerate() {
        patch_u16(&mut out, lk_offsets_pos + i * 2, off);
    }

    out
}

/// Serialize a single Lookup into bytes.
///
/// The subtable data is embedded directly after the Lookup header so that
/// all offsets (Offset16 from Lookup start) remain ≤ 65535.
fn build_lookup_bytes(lk: &RewrittenLookup) -> Vec<u8> {
    let sub_count = lk.subtable_bytes.len();
    // Header size: lookupType(2) + lookupFlag(2) + subTableCount(2) +
    //              subtableOffsets[n](2*n) + [markFilteringSet(2)]
    let has_mfs = lk.mark_filtering_set.is_some();
    let header_size = 6 + sub_count * 2 + if has_mfs { 2 } else { 0 };

    let mut out = Vec::new();
    w_u16(&mut out, lk.lookup_type);
    w_u16(&mut out, lk.lookup_flag);
    w_u16(&mut out, sub_count as u16);

    let st_offsets_pos = out.len();
    for _ in 0..sub_count {
        w_u16(&mut out, 0);
    }
    if let Some(mfs) = lk.mark_filtering_set {
        w_u16(&mut out, mfs);
    }
    assert_eq!(out.len(), header_size);

    // Append subtable data and record offsets (relative to Lookup start).
    let mut st_offs: Vec<u16> = Vec::with_capacity(sub_count);
    for st in &lk.subtable_bytes {
        st_offs.push(out.len() as u16);
        out.extend_from_slice(st);
    }

    // Patch subtable offsets.
    for (i, &off) in st_offs.iter().enumerate() {
        patch_u16(&mut out, st_offsets_pos + i * 2, off);
    }

    out
}

// ---------------------------------------------------------------------------
// FeatureList rewriting
// ---------------------------------------------------------------------------

/// Rewrite the FeatureList, remapping lookup indices through `lk_index_map`.
///
/// A feature is dropped if it has zero surviving lookup indices AND its
/// `requiredFeatureIndex` is 0xFFFF (no required feature).
///
/// Returns `(new_feature_list_bytes, feature_index_map)`.
pub(crate) fn rewrite_feature_list(
    table: &[u8],
    fl_offset: usize,
    lk_index_map: &[Option<u16>],
) -> (Vec<u8>, Vec<Option<u16>>) {
    let fl_data = match table.get(fl_offset..) {
        Some(d) => d,
        None => return (build_empty_feature_list(), vec![]),
    };

    let feat_count = match r_u16(fl_data, 0) {
        Some(c) => c as usize,
        None => return (build_empty_feature_list(), vec![]),
    };

    if fl_data.len() < 2 + feat_count * 6 {
        return (build_empty_feature_list(), vec![]);
    }

    let mut feat_index_map: Vec<Option<u16>> = Vec::with_capacity(feat_count);
    // (tag, featureParamsOffset=0, new_lookup_indices)
    let mut new_features: Vec<([u8; 4], Vec<u16>)> = Vec::new();

    for i in 0..feat_count {
        let base = 2 + i * 6;
        let tag: [u8; 4] = match fl_data.get(base..base + 4) {
            Some(t) => [t[0], t[1], t[2], t[3]],
            None => {
                feat_index_map.push(None);
                continue;
            }
        };
        let feat_off = match r_u16(fl_data, base + 4) {
            Some(o) => o as usize,
            None => {
                feat_index_map.push(None);
                continue;
            }
        };

        let feat_data = match fl_data.get(feat_off..) {
            Some(d) => d,
            None => {
                feat_index_map.push(None);
                continue;
            }
        };

        // featureParamsOffset (we ignore it — emit 0 for rebuilt features;
        // preserving requires per-tag length knowledge which is out of scope here).
        let lk_index_count = match r_u16(feat_data, 2) {
            Some(c) => c as usize,
            None => {
                feat_index_map.push(None);
                continue;
            }
        };

        if feat_data.len() < 4 + lk_index_count * 2 {
            feat_index_map.push(None);
            continue;
        }

        // Remap lookup indices, keeping survivors.
        let mut new_lk_indices: Vec<u16> = Vec::new();
        for j in 0..lk_index_count {
            let old_lk_idx = match r_u16(feat_data, 4 + j * 2) {
                Some(k) => k as usize,
                None => continue,
            };
            if let Some(Some(new_idx)) = lk_index_map.get(old_lk_idx) {
                new_lk_indices.push(*new_idx);
            }
        }

        if new_lk_indices.is_empty() {
            feat_index_map.push(None);
        } else {
            let new_feat_idx = new_features.len() as u16;
            feat_index_map.push(Some(new_feat_idx));
            new_features.push((tag, new_lk_indices));
        }
    }

    let fl_bytes = build_feature_list_bytes(&new_features);
    (fl_bytes, feat_index_map)
}

/// Build empty FeatureList bytes.
fn build_empty_feature_list() -> Vec<u8> {
    let mut out = Vec::with_capacity(2);
    w_u16(&mut out, 0); // featureCount
    out
}

/// Serialize a FeatureList.
///
/// Layout:
/// ```text
/// featureCount: u16
/// featureRecords: [{tag: [u8;4], featureOffset: u16}; n]  (from FeatureList start)
/// Feature: featureParamsOffset(0) + lookupIndexCount + lookupListIndices...
/// ```
fn build_feature_list_bytes(features: &[([u8; 4], Vec<u16>)]) -> Vec<u8> {
    let n = features.len();
    let fl_header_size = 2 + n * 6;

    let mut out = Vec::new();
    w_u16(&mut out, n as u16); // featureCount

    let feat_rec_offsets_pos = out.len();
    // Placeholder feature offsets (in featureRecord, last 2 bytes).
    for (tag, _) in features {
        out.extend_from_slice(tag);
        w_u16(&mut out, 0); // placeholder featureOffset
    }
    assert_eq!(out.len(), fl_header_size);

    // Append Feature data and record offsets.
    let mut feat_offs: Vec<u16> = Vec::with_capacity(n);
    for (_, lk_indices) in features {
        feat_offs.push(out.len() as u16);
        w_u16(&mut out, 0); // featureParamsOffset = 0
        w_u16(&mut out, lk_indices.len() as u16);
        for &idx in lk_indices {
            w_u16(&mut out, idx);
        }
    }

    // Patch featureOffset in each featureRecord.
    for (i, &off) in feat_offs.iter().enumerate() {
        let pos = feat_rec_offsets_pos + i * 6 + 4; // skip 4-byte tag
        patch_u16(&mut out, pos, off);
    }

    out
}

// ---------------------------------------------------------------------------
// ScriptList rewriting
// ---------------------------------------------------------------------------

/// A rebuilt LangSys.
struct NewLangSys {
    req_feat_idx: u16, // 0xFFFF if none
    feat_indices: Vec<u16>,
}

/// Rewrite the ScriptList, remapping feature indices through `feat_index_map`.
pub(crate) fn rewrite_script_list(
    table: &[u8],
    sl_offset: usize,
    feat_index_map: &[Option<u16>],
) -> Vec<u8> {
    let sl_data = match table.get(sl_offset..) {
        Some(d) => d,
        None => return build_empty_script_list(),
    };

    let script_count = match r_u16(sl_data, 0) {
        Some(c) => c as usize,
        None => return build_empty_script_list(),
    };

    if sl_data.len() < 2 + script_count * 6 {
        return build_empty_script_list();
    }

    // Collect surviving scripts.
    let mut new_scripts: Vec<([u8; 4], Vec<u8>)> = Vec::new(); // (tag, script_bytes)

    for i in 0..script_count {
        let base = 2 + i * 6;
        let tag: [u8; 4] = match sl_data.get(base..base + 4) {
            Some(t) => [t[0], t[1], t[2], t[3]],
            None => continue,
        };
        let sc_off = match r_u16(sl_data, base + 4) {
            Some(o) => o as usize,
            None => continue,
        };
        let sc_data = match sl_data.get(sc_off..) {
            Some(d) => d,
            None => continue,
        };

        let default_ls_off = match r_u16(sc_data, 0) {
            Some(o) => o as usize,
            None => continue,
        };
        let langsys_count = match r_u16(sc_data, 2) {
            Some(c) => c as usize,
            None => continue,
        };
        if sc_data.len() < 4 + langsys_count * 6 {
            continue;
        }

        // Rewrite DefaultLangSys.
        let new_default_ls: Option<NewLangSys> = if default_ls_off != 0 {
            parse_and_remap_langsys(sc_data, default_ls_off, feat_index_map)
        } else {
            None
        };

        // Rewrite named LangSys records.
        let mut new_langsys: Vec<([u8; 4], NewLangSys)> = Vec::new();
        for j in 0..langsys_count {
            let ls_base = 4 + j * 6;
            let ls_tag: [u8; 4] = match sc_data.get(ls_base..ls_base + 4) {
                Some(t) => [t[0], t[1], t[2], t[3]],
                None => continue,
            };
            let ls_off = match r_u16(sc_data, ls_base + 4) {
                Some(o) => o as usize,
                None => continue,
            };
            if let Some(nls) = parse_and_remap_langsys(sc_data, ls_off, feat_index_map) {
                new_langsys.push((ls_tag, nls));
            }
        }

        // Keep script if any LangSys survives.
        let has_default = new_default_ls.is_some();
        let has_named = !new_langsys.is_empty();
        if has_default || has_named {
            let sc_bytes = build_script_bytes(new_default_ls, &new_langsys);
            new_scripts.push((tag, sc_bytes));
        }
    }

    build_script_list_bytes(&new_scripts)
}

/// Parse a LangSys at `ls_off` within `sc_data` and remap feature indices.
/// Returns `None` if no feature indices (and no requiredFeatureIndex) survive.
fn parse_and_remap_langsys(
    sc_data: &[u8],
    ls_off: usize,
    feat_index_map: &[Option<u16>],
) -> Option<NewLangSys> {
    let ls_data = sc_data.get(ls_off..)?;
    // lookupOrderOffset: u16 (reserved)
    let req_feat = r_u16(ls_data, 2)?;
    let feat_count = r_u16(ls_data, 4)? as usize;
    if ls_data.len() < 6 + feat_count * 2 {
        return None;
    }

    // Remap requiredFeatureIndex.
    let new_req = if req_feat == 0xFFFF {
        0xFFFF
    } else {
        let old_idx = req_feat as usize;
        // Required feature dropped → mark as "no required feature" (0xFFFF).
        feat_index_map
            .get(old_idx)
            .copied()
            .flatten()
            .unwrap_or(0xFFFF)
    };

    // Remap regular feature indices.
    let mut new_feat_indices: Vec<u16> = Vec::new();
    for j in 0..feat_count {
        let old_idx = r_u16(ls_data, 6 + j * 2)? as usize;
        if let Some(Some(new_idx)) = feat_index_map.get(old_idx) {
            new_feat_indices.push(*new_idx);
        }
    }

    if new_req == 0xFFFF && new_feat_indices.is_empty() {
        return None;
    }

    Some(NewLangSys {
        req_feat_idx: new_req,
        feat_indices: new_feat_indices,
    })
}

/// Serialize a Script table from a rebuilt DefaultLangSys + named LangSys list.
fn build_script_bytes(
    default_ls: Option<NewLangSys>,
    named_ls: &[([u8; 4], NewLangSys)],
) -> Vec<u8> {
    let lc = named_ls.len();
    // Header: defaultLangSysOffset(2) + langSysCount(2) + langSysRecords[lc](6*lc)
    let header_size = 4 + lc * 6;

    let mut out = Vec::new();
    // Placeholders.
    let dls_off_pos = out.len();
    w_u16(&mut out, 0); // defaultLangSysOffset placeholder
    w_u16(&mut out, lc as u16);

    let ls_rec_offsets_pos = out.len();
    for (tag, _) in named_ls {
        out.extend_from_slice(tag);
        w_u16(&mut out, 0); // placeholder langSysOffset
    }
    assert_eq!(out.len(), header_size);

    // DefaultLangSys.
    let dls_off: u16 = if let Some(ref dls) = default_ls {
        let off = out.len() as u16;
        out.extend_from_slice(&build_langsys_bytes(dls));
        off
    } else {
        0
    };
    patch_u16(&mut out, dls_off_pos, dls_off);

    // Named LangSys.
    let mut ls_offs: Vec<u16> = Vec::with_capacity(lc);
    for (_, nls) in named_ls {
        ls_offs.push(out.len() as u16);
        out.extend_from_slice(&build_langsys_bytes(nls));
    }
    for (i, &off) in ls_offs.iter().enumerate() {
        let pos = ls_rec_offsets_pos + i * 6 + 4; // after 4-byte tag
        patch_u16(&mut out, pos, off);
    }

    out
}

/// Serialize a LangSys.
fn build_langsys_bytes(nls: &NewLangSys) -> Vec<u8> {
    let mut out = Vec::new();
    w_u16(&mut out, 0); // lookupOrderOffset (reserved)
    w_u16(&mut out, nls.req_feat_idx);
    w_u16(&mut out, nls.feat_indices.len() as u16);
    for &idx in &nls.feat_indices {
        w_u16(&mut out, idx);
    }
    out
}

/// Build an empty ScriptList.
fn build_empty_script_list() -> Vec<u8> {
    let mut out = Vec::with_capacity(2);
    w_u16(&mut out, 0); // scriptCount
    out
}

/// Serialize a ScriptList from rebuilt scripts.
fn build_script_list_bytes(scripts: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let n = scripts.len();
    let sl_header_size = 2 + n * 6;

    let mut out = Vec::new();
    w_u16(&mut out, n as u16); // scriptCount

    let sc_rec_offsets_pos = out.len();
    for (tag, _) in scripts {
        out.extend_from_slice(tag);
        w_u16(&mut out, 0); // placeholder scriptOffset
    }
    assert_eq!(out.len(), sl_header_size);

    let mut sc_offs: Vec<u16> = Vec::with_capacity(n);
    for (_, sc_bytes) in scripts {
        sc_offs.push(out.len() as u16);
        out.extend_from_slice(sc_bytes);
    }
    for (i, &off) in sc_offs.iter().enumerate() {
        let pos = sc_rec_offsets_pos + i * 6 + 4; // after 4-byte tag
        patch_u16(&mut out, pos, off);
    }

    out
}

// ---------------------------------------------------------------------------
// Public entry point: rewrite_gsub
// ---------------------------------------------------------------------------

/// Rewrite a GSUB table, remapping all GID references through `gid_remap`.
///
/// On parse failure or input < 10 bytes, returns the original bytes verbatim.
///
/// GSUB v1.1 FeatureVariations are dropped; the rebuilt table uses v1.0.
pub fn rewrite_gsub(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    if table.len() < 10 {
        return table.to_vec();
    }
    match try_rewrite_gsub(table, gid_remap) {
        Some(v) => v,
        None => table.to_vec(),
    }
}

fn try_rewrite_gsub(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    let major = r_u16(table, 0)?;
    if major != 1 {
        return None;
    }
    // minorVersion: 0 or 1 (v1.1 has FeatureVariations).
    // We always output v1.0 (dropping FeatureVariations if present).

    let sl_offset = r_u16(table, 4)? as usize;
    let fl_offset = r_u16(table, 6)? as usize;
    let ll_offset = r_u16(table, 8)? as usize;

    // ---- Step 1: Rewrite LookupList ----
    let (new_ll_bytes, lk_index_map) = rewrite_lookup_list(table, ll_offset, gid_remap);

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
