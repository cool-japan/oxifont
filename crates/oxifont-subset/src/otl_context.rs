//! Contextual and chaining-contextual lookup subtables.
//!
//! `SequenceContext` (GSUB lookup type 5 / GPOS type 7) and
//! `ChainedSequenceContext` (GSUB type 6 / GPOS type 8) share one binary
//! layout across both tables, so a single implementation serves all four
//! lookup types. All three formats are supported:
//!
//! | format | keyed on                    | rewrite strategy                       |
//! |--------|-----------------------------|----------------------------------------|
//! | 1      | glyph IDs (rule sets)       | coverage + rule glyph sequences remapped |
//! | 2      | class values (rule sets)    | coverage + ClassDefs remapped, class values preserved |
//! | 3      | coverage tables per position| every coverage remapped                |
//!
//! # Two-phase lookup-index fixup
//!
//! Every contextual subtable embeds `SequenceLookupRecord`s whose
//! `lookupListIndex` points into the *LookupList* — indices that the subsetter
//! renumbers as lookups are dropped. Those indices are therefore **not**
//! resolved while parsing: [`parse_context_subtable`] keeps the original
//! values in an in-memory IR ([`ContextSubtable`]) and
//! [`ContextSubtable::serialize`] applies the final old→new index map when the
//! LookupList is written, after every lookup's fate is known.
//!
//! Keeping the IR (rather than serialising provisional bytes and re-parsing
//! them) means there is no code path that can emit a *stale* `lookupListIndex`:
//! a subtable is either serialised with the final map or not serialised at all.

use crate::layout::{read_coverage, remap_classdef, remap_coverage, write_coverage};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Byte helpers (mirrors of the private helpers in `otl` / `otl_gpos`)
// ---------------------------------------------------------------------------

#[inline]
fn r_u16(data: &[u8], offset: usize) -> Option<u16> {
    let b = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
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
    let b = v.to_be_bytes();
    out[offset] = b[0];
    out[offset + 1] = b[1];
}

/// Offsets in these tables are `Offset16`; a rule set that would land beyond
/// this bound cannot be referenced and is emitted as NULL instead.
const OFFSET16_MAX: usize = u16::MAX as usize;

// ---------------------------------------------------------------------------
// Intermediate representation
// ---------------------------------------------------------------------------

/// One `SequenceRule` / `ChainedSequenceRule` / class-based equivalent.
///
/// `input` holds the sequence **excluding** the first position (which is
/// matched by the subtable coverage), exactly as stored on disk. For format 1
/// the values are glyph IDs (already remapped); for format 2 they are class
/// values (left untouched by subsetting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextRule {
    /// Backtrack sequence, in the spec's reverse (closest-first) order.
    pub(crate) back: Vec<u16>,
    /// Input sequence from the *second* position onwards.
    pub(crate) input: Vec<u16>,
    /// Lookahead sequence.
    pub(crate) ahead: Vec<u16>,
    /// `(sequenceIndex, lookupListIndex)` pairs, indices still in the
    /// *original* lookup numbering until [`ContextSubtable::serialize`] runs.
    pub(crate) records: Vec<(u16, u16)>,
}

/// Format-specific payload of a contextual subtable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextBody {
    /// Format 1 — glyph-keyed rule sets, parallel to `coverage`.
    Format1 {
        /// New (subset) glyph IDs, ascending.
        coverage: Vec<u16>,
        /// Rule sets, one per `coverage` entry.
        rule_sets: Vec<Vec<ContextRule>>,
    },
    /// Format 2 — class-keyed rule sets, indexed by input class value.
    Format2 {
        /// Serialised, already-remapped Coverage table.
        coverage: Vec<u8>,
        /// Serialised backtrack ClassDef (empty when not chained).
        back_class: Vec<u8>,
        /// Serialised input ClassDef.
        input_class: Vec<u8>,
        /// Serialised lookahead ClassDef (empty when not chained).
        ahead_class: Vec<u8>,
        /// Rule sets indexed by input class value; entries may be empty.
        rule_sets: Vec<Vec<ContextRule>>,
    },
    /// Format 3 — one Coverage table per sequence position.
    Format3 {
        /// Serialised backtrack coverages (empty when not chained).
        back: Vec<Vec<u8>>,
        /// Serialised input coverages, one per input position.
        input: Vec<Vec<u8>>,
        /// Serialised lookahead coverages (empty when not chained).
        ahead: Vec<Vec<u8>>,
        /// `(sequenceIndex, lookupListIndex)` pairs, original numbering.
        records: Vec<(u16, u16)>,
    },
}

/// A parsed, GID-remapped contextual subtable awaiting lookup-index fixup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextSubtable {
    /// `true` for `ChainedSequenceContext` (GSUB 6 / GPOS 8).
    pub(crate) chained: bool,
    /// Format-specific payload.
    pub(crate) body: ContextBody,
}

// ---------------------------------------------------------------------------
// Dispatcher output
// ---------------------------------------------------------------------------

/// What a subtable rewriter produced.
///
/// Most lookup types serialise immediately to bytes; contextual ones must wait
/// for the final lookup index map, and Extension wrappers simply forward the
/// distinction to their inner subtable.
#[derive(Debug, Clone)]
pub(crate) enum SubtableOut {
    /// Fully rewritten subtable bytes; nothing left to fix up.
    Bytes(Vec<u8>),
    /// A contextual subtable whose `lookupListIndex` values are still original.
    Context(ContextSubtable),
    /// `ExtensionSubst` (GSUB 7) / `ExtensionPos` (GPOS 9) wrapper.
    Extension {
        /// `extensionLookupType` of the wrapped subtable.
        ext_type: u16,
        /// The wrapped subtable.
        inner: Box<SubtableOut>,
    },
}

impl SubtableOut {
    /// Serialise to final subtable bytes.
    ///
    /// `index_map` maps *original* lookup index → `Some(new index)` or `None`
    /// when the lookup was dropped from the subset. Pass `None` to keep the
    /// original indices (used by the single-subtable public helpers, which have
    /// no LookupList context).
    pub(crate) fn serialize(&self, index_map: Option<&[Option<u16>]>) -> Vec<u8> {
        match self {
            SubtableOut::Bytes(b) => b.clone(),
            SubtableOut::Context(c) => c.serialize(index_map),
            SubtableOut::Extension { ext_type, inner } => {
                let inner_bytes = inner.serialize(index_map);
                // format(2) + extensionLookupType(2) + extensionOffset(4) = 8
                let mut out = Vec::with_capacity(8 + inner_bytes.len());
                w_u16(&mut out, 1);
                w_u16(&mut out, *ext_type);
                w_u32(&mut out, 8);
                out.extend_from_slice(&inner_bytes);
                out
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse and GID-remap the (chained) context subtable at `offset`.
///
/// Returns `None` when the subtable is malformed, uses an unknown format, or
/// can no longer match anything under the subset (empty coverage, or — for
/// format 1 — no surviving rule).
pub(crate) fn parse_context_subtable(
    data: &[u8],
    offset: usize,
    gid_remap: &HashMap<u16, u16>,
    chained: bool,
) -> Option<ContextSubtable> {
    let sub = data.get(offset..)?;
    let body = match r_u16(sub, 0)? {
        1 => parse_format1(data, offset, sub, gid_remap, chained)?,
        2 => parse_format2(data, offset, sub, gid_remap, chained)?,
        3 => parse_format3(data, offset, sub, gid_remap, chained)?,
        _ => return None,
    };
    Some(ContextSubtable { chained, body })
}

/// Format 1 — coverage-parallel glyph rule sets.
fn parse_format1(
    data: &[u8],
    offset: usize,
    sub: &[u8],
    gid_remap: &HashMap<u16, u16>,
    chained: bool,
) -> Option<ContextBody> {
    let cov_off = r_u16(sub, 2)? as usize;
    if cov_off == 0 {
        return None;
    }
    let set_count = r_u16(sub, 4)? as usize;
    let old_gids = read_coverage(data, offset + cov_off);
    if old_gids.len() != set_count {
        // Malformed: the rule-set array must be parallel to coverage.
        return None;
    }

    let mut entries: Vec<(u16, Vec<ContextRule>)> = Vec::new();
    for (i, &old_gid) in old_gids.iter().enumerate() {
        let rs_off = r_u16(sub, 6 + i * 2)? as usize;
        let new_gid = match gid_remap.get(&old_gid) {
            Some(&g) => g,
            None => continue,
        };
        if rs_off == 0 {
            continue;
        }
        let raw = parse_rule_set(data, offset + rs_off, chained)?;
        // Rules referencing a glyph that left the subset can never match.
        let rules: Vec<ContextRule> = raw
            .into_iter()
            .filter_map(|r| remap_rule_glyphs(&r, gid_remap))
            .collect();
        if rules.is_empty() {
            continue;
        }
        entries.push((new_gid, rules));
    }

    if entries.is_empty() {
        return None;
    }
    entries.sort_unstable_by_key(|(g, _)| *g);
    entries.dedup_by_key(|e| e.0);
    // The coverage sits immediately after the header, so the header itself must
    // be addressable by an Offset16.
    if 6 + entries.len() * 2 > OFFSET16_MAX {
        return None;
    }

    let coverage: Vec<u16> = entries.iter().map(|(g, _)| *g).collect();
    let rule_sets: Vec<Vec<ContextRule>> = entries.into_iter().map(|(_, r)| r).collect();
    Some(ContextBody::Format1 {
        coverage,
        rule_sets,
    })
}

/// Format 2 — class-keyed rule sets.
fn parse_format2(
    data: &[u8],
    offset: usize,
    sub: &[u8],
    gid_remap: &HashMap<u16, u16>,
    chained: bool,
) -> Option<ContextBody> {
    let cov_off = r_u16(sub, 2)? as usize;
    if cov_off == 0 {
        return None;
    }
    let (coverage, new_gids) = remap_coverage(data, offset + cov_off, gid_remap);
    if new_gids.is_empty() {
        return None;
    }

    // ClassDef offsets and the rule-set array start differ between the plain
    // and chained layouts.
    let (back_class, input_class, ahead_class, set_count, sets_pos) = if chained {
        let back_off = r_u16(sub, 4)? as usize;
        let input_off = r_u16(sub, 6)? as usize;
        let ahead_off = r_u16(sub, 8)? as usize;
        if input_off == 0 {
            return None;
        }
        let back = if back_off == 0 {
            write_classdef_empty()
        } else {
            remap_classdef(data, offset + back_off, gid_remap)
        };
        let ahead = if ahead_off == 0 {
            write_classdef_empty()
        } else {
            remap_classdef(data, offset + ahead_off, gid_remap)
        };
        let input = remap_classdef(data, offset + input_off, gid_remap);
        (back, input, ahead, r_u16(sub, 10)? as usize, 12usize)
    } else {
        let cd_off = r_u16(sub, 4)? as usize;
        if cd_off == 0 {
            return None;
        }
        let input = remap_classdef(data, offset + cd_off, gid_remap);
        (
            Vec::new(),
            input,
            Vec::new(),
            r_u16(sub, 6)? as usize,
            8usize,
        )
    };

    // Coverage and ClassDefs follow the header, so the header must be
    // addressable by an Offset16.
    if sets_pos + set_count * 2 > OFFSET16_MAX {
        return None;
    }

    let mut rule_sets: Vec<Vec<ContextRule>> = Vec::with_capacity(set_count);
    let mut any = false;
    for i in 0..set_count {
        let rs_off = r_u16(sub, sets_pos + i * 2)? as usize;
        if rs_off == 0 {
            rule_sets.push(Vec::new());
            continue;
        }
        // Class values are unaffected by GID remapping, so rules survive as-is.
        let rules = parse_rule_set(data, offset + rs_off, chained)?;
        any |= !rules.is_empty();
        rule_sets.push(rules);
    }
    if !any {
        return None;
    }

    Some(ContextBody::Format2 {
        coverage,
        back_class,
        input_class,
        ahead_class,
        rule_sets,
    })
}

/// Format 3 — one coverage per position.
fn parse_format3(
    data: &[u8],
    offset: usize,
    sub: &[u8],
    gid_remap: &HashMap<u16, u16>,
    chained: bool,
) -> Option<ContextBody> {
    let mut pos = 2usize;
    let (back, input, ahead) = if chained {
        let back = read_context_coverages(data, offset, sub, &mut pos, gid_remap)?;
        let input = read_context_coverages(data, offset, sub, &mut pos, gid_remap)?;
        let ahead = read_context_coverages(data, offset, sub, &mut pos, gid_remap)?;
        (back, input, ahead)
    } else {
        let input = read_context_coverages(data, offset, sub, &mut pos, gid_remap)?;
        (Vec::new(), input, Vec::new())
    };
    if input.is_empty() {
        return None;
    }
    let records = read_seq_lookup_records(sub, pos)?;
    Some(ContextBody::Format3 {
        back,
        input,
        ahead,
        records,
    })
}

/// An empty ClassDef format 2 (`classFormat = 2`, `classRangeCount = 0`).
fn write_classdef_empty() -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    w_u16(&mut out, 2);
    w_u16(&mut out, 0);
    out
}

/// Read a `count` + `coverageOffsets[count]` group at `*pos` within `sub`
/// (= `data[offset..]`), remapping each coverage. Returns `None` if any
/// position's coverage empties out — the rule could never match again.
fn read_context_coverages(
    data: &[u8],
    offset: usize,
    sub: &[u8],
    pos: &mut usize,
    gid_remap: &HashMap<u16, u16>,
) -> Option<Vec<Vec<u8>>> {
    let count = r_u16(sub, *pos)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let cov_off = r_u16(sub, *pos + 2 + i * 2)? as usize;
        if cov_off == 0 {
            return None;
        }
        let (bytes, new_gids) = remap_coverage(data, offset + cov_off, gid_remap);
        if new_gids.is_empty() {
            return None;
        }
        out.push(bytes);
    }
    *pos += 2 + count * 2;
    Some(out)
}

/// Read `seqLookupCount` + `seqLookupRecords[]` at `pos` within `sub`.
fn read_seq_lookup_records(sub: &[u8], pos: usize) -> Option<Vec<(u16, u16)>> {
    let count = r_u16(sub, pos)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let seq = r_u16(sub, pos + 2 + i * 4)?;
        let lk = r_u16(sub, pos + 4 + i * 4)?;
        out.push((seq, lk));
    }
    Some(out)
}

/// Parse a `SequenceRuleSet` / `ChainedSequenceRuleSet` at absolute `rs_abs`.
fn parse_rule_set(data: &[u8], rs_abs: usize, chained: bool) -> Option<Vec<ContextRule>> {
    let rs = data.get(rs_abs..)?;
    let count = r_u16(rs, 0)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let r_off = r_u16(rs, 2 + i * 2)? as usize;
        if r_off == 0 {
            continue;
        }
        out.push(parse_rule(data, rs_abs + r_off, chained)?);
    }
    Some(out)
}

/// Parse a single `SequenceRule` / `ChainedSequenceRule` at absolute `abs`.
fn parse_rule(data: &[u8], abs: usize, chained: bool) -> Option<ContextRule> {
    let r = data.get(abs..)?;
    if chained {
        let mut pos = 0usize;
        let back = read_u16_run(r, &mut pos)?;
        // `inputGlyphCount` counts the coverage-matched first position too, so
        // only `inputGlyphCount - 1` values follow it on disk.
        let input_count = r_u16(r, pos)? as usize;
        pos += 2;
        let input = read_u16_array(r, pos, input_count.saturating_sub(1))?;
        pos += input.len() * 2;
        let ahead = read_u16_run(r, &mut pos)?;
        let records = read_seq_lookup_records(r, pos)?;
        Some(ContextRule {
            back,
            input,
            ahead,
            records,
        })
    } else {
        let glyph_count = r_u16(r, 0)? as usize;
        let rec_count = r_u16(r, 2)? as usize;
        let input = read_u16_array(r, 4, glyph_count.saturating_sub(1))?;
        let rec_pos = 4 + input.len() * 2;
        let mut records = Vec::with_capacity(rec_count);
        for i in 0..rec_count {
            let seq = r_u16(r, rec_pos + i * 4)?;
            let lk = r_u16(r, rec_pos + i * 4 + 2)?;
            records.push((seq, lk));
        }
        Some(ContextRule {
            back: Vec::new(),
            input,
            ahead: Vec::new(),
            records,
        })
    }
}

/// Read a `count` + `values[count]` run at `*pos`, advancing past both.
fn read_u16_run(r: &[u8], pos: &mut usize) -> Option<Vec<u16>> {
    let count = r_u16(r, *pos)? as usize;
    let vals = read_u16_array(r, *pos + 2, count)?;
    *pos += 2 + count * 2;
    Some(vals)
}

/// Read `count` big-endian `u16`s starting at `pos`.
fn read_u16_array(r: &[u8], pos: usize, count: usize) -> Option<Vec<u16>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(r_u16(r, pos + i * 2)?);
    }
    Some(out)
}

/// Remap every glyph ID in a format-1 rule; `None` if any glyph left the subset.
fn remap_rule_glyphs(rule: &ContextRule, gid_remap: &HashMap<u16, u16>) -> Option<ContextRule> {
    let map_all = |v: &Vec<u16>| -> Option<Vec<u16>> {
        v.iter().map(|g| gid_remap.get(g).copied()).collect()
    };
    Some(ContextRule {
        back: map_all(&rule.back)?,
        input: map_all(&rule.input)?,
        ahead: map_all(&rule.ahead)?,
        records: rule.records.clone(),
    })
}

// ---------------------------------------------------------------------------
// Serialisation
// ---------------------------------------------------------------------------

/// Apply the old→new lookup index map to a record list, dropping records whose
/// target lookup was removed from the subset.
fn map_records(records: &[(u16, u16)], index_map: Option<&[Option<u16>]>) -> Vec<(u16, u16)> {
    match index_map {
        None => records.to_vec(),
        Some(map) => records
            .iter()
            .filter_map(|&(seq, lk)| {
                map.get(lk as usize)
                    .copied()
                    .flatten()
                    .map(|new_lk| (seq, new_lk))
            })
            .collect(),
    }
}

impl ContextSubtable {
    /// Serialise to on-disk bytes, resolving `lookupListIndex` values through
    /// `index_map` (see [`SubtableOut::serialize`]).
    pub(crate) fn serialize(&self, index_map: Option<&[Option<u16>]>) -> Vec<u8> {
        match &self.body {
            ContextBody::Format1 {
                coverage,
                rule_sets,
            } => build_format1(self.chained, coverage, rule_sets, index_map),
            ContextBody::Format2 {
                coverage,
                back_class,
                input_class,
                ahead_class,
                rule_sets,
            } => build_format2(
                self.chained,
                coverage,
                back_class,
                input_class,
                ahead_class,
                rule_sets,
                index_map,
            ),
            ContextBody::Format3 {
                back,
                input,
                ahead,
                records,
            } => {
                let recs = map_records(records, index_map);
                build_format3(self.chained, back, input, ahead, &recs)
            }
        }
    }
}

/// Serialise one rule. Returns `None` when every record was dropped — such a
/// rule would match and then do nothing, so it is omitted.
fn build_rule(
    chained: bool,
    rule: &ContextRule,
    index_map: Option<&[Option<u16>]>,
) -> Option<Vec<u8>> {
    let records = map_records(&rule.records, index_map);
    if records.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    if chained {
        w_u16(&mut out, rule.back.len() as u16);
        for &v in &rule.back {
            w_u16(&mut out, v);
        }
        // Stored count includes the coverage-matched first position.
        w_u16(&mut out, (rule.input.len() + 1) as u16);
        for &v in &rule.input {
            w_u16(&mut out, v);
        }
        w_u16(&mut out, rule.ahead.len() as u16);
        for &v in &rule.ahead {
            w_u16(&mut out, v);
        }
        w_u16(&mut out, records.len() as u16);
        for &(seq, lk) in &records {
            w_u16(&mut out, seq);
            w_u16(&mut out, lk);
        }
    } else {
        w_u16(&mut out, (rule.input.len() + 1) as u16);
        w_u16(&mut out, records.len() as u16);
        for &v in &rule.input {
            w_u16(&mut out, v);
        }
        for &(seq, lk) in &records {
            w_u16(&mut out, seq);
            w_u16(&mut out, lk);
        }
    }
    Some(out)
}

/// Serialise a `SequenceRuleSet` / `ChainedSequenceRuleSet`, or `None` when it
/// holds no surviving rule (the caller then emits a NULL offset).
fn build_rule_set(
    chained: bool,
    rules: &[ContextRule],
    index_map: Option<&[Option<u16>]>,
) -> Option<Vec<u8>> {
    let blobs: Vec<Vec<u8>> = rules
        .iter()
        .filter_map(|r| build_rule(chained, r, index_map))
        .collect();
    if blobs.is_empty() {
        return None;
    }
    let n = blobs.len();
    let mut out = Vec::new();
    w_u16(&mut out, n as u16);
    let off_pos = out.len();
    for _ in 0..n {
        w_u16(&mut out, 0);
    }
    for (i, blob) in blobs.iter().enumerate() {
        let off = out.len();
        if off > OFFSET16_MAX {
            // Unreachable for realistic subsets; emitting NULL keeps the table
            // well-formed instead of writing a truncated offset.
            continue;
        }
        patch_u16(&mut out, off_pos + i * 2, off as u16);
        out.extend_from_slice(blob);
    }
    Some(out)
}

/// Append rule-set blobs after the header and patch their offset slots.
fn append_rule_sets(
    out: &mut Vec<u8>,
    off_array_pos: usize,
    chained: bool,
    rule_sets: &[Vec<ContextRule>],
    index_map: Option<&[Option<u16>]>,
) {
    for (i, rules) in rule_sets.iter().enumerate() {
        let blob = match build_rule_set(chained, rules, index_map) {
            Some(b) => b,
            None => continue, // NULL offset — already zeroed.
        };
        let off = out.len();
        if off > OFFSET16_MAX {
            continue;
        }
        patch_u16(out, off_array_pos + i * 2, off as u16);
        out.extend_from_slice(&blob);
    }
}

/// Serialise a `SequenceContextFormat1` / `ChainedSequenceContextFormat1`.
fn build_format1(
    chained: bool,
    coverage: &[u16],
    rule_sets: &[Vec<ContextRule>],
    index_map: Option<&[Option<u16>]>,
) -> Vec<u8> {
    let n = rule_sets.len();
    let mut out = Vec::new();
    w_u16(&mut out, 1); // format
    let cov_off_pos = out.len();
    w_u16(&mut out, 0); // coverageOffset placeholder
    w_u16(&mut out, n as u16); // (chained)SeqRuleSetCount
    let sets_pos = out.len();
    for _ in 0..n {
        w_u16(&mut out, 0);
    }
    // Coverage first so its Offset16 always fits.
    let cov_off = out.len() as u16;
    patch_u16(&mut out, cov_off_pos, cov_off);
    out.extend_from_slice(&write_coverage(coverage));
    append_rule_sets(&mut out, sets_pos, chained, rule_sets, index_map);
    out
}

/// Serialise a `SequenceContextFormat2` / `ChainedSequenceContextFormat2`.
#[allow(clippy::too_many_arguments)]
fn build_format2(
    chained: bool,
    coverage: &[u8],
    back_class: &[u8],
    input_class: &[u8],
    ahead_class: &[u8],
    rule_sets: &[Vec<ContextRule>],
    index_map: Option<&[Option<u16>]>,
) -> Vec<u8> {
    let n = rule_sets.len();
    let mut out = Vec::new();
    w_u16(&mut out, 2); // format
    let cov_off_pos = out.len();
    w_u16(&mut out, 0); // coverageOffset placeholder
    let class_off_pos = out.len();
    if chained {
        w_u16(&mut out, 0); // backtrackClassDefOffset
        w_u16(&mut out, 0); // inputClassDefOffset
        w_u16(&mut out, 0); // lookaheadClassDefOffset
    } else {
        w_u16(&mut out, 0); // classDefOffset
    }
    w_u16(&mut out, n as u16);
    let sets_pos = out.len();
    for _ in 0..n {
        w_u16(&mut out, 0);
    }

    let cov_off = out.len() as u16;
    patch_u16(&mut out, cov_off_pos, cov_off);
    out.extend_from_slice(coverage);

    if chained {
        let back_off = out.len() as u16;
        out.extend_from_slice(back_class);
        let input_off = out.len() as u16;
        out.extend_from_slice(input_class);
        let ahead_off = out.len() as u16;
        out.extend_from_slice(ahead_class);
        patch_u16(&mut out, class_off_pos, back_off);
        patch_u16(&mut out, class_off_pos + 2, input_off);
        patch_u16(&mut out, class_off_pos + 4, ahead_off);
    } else {
        let input_off = out.len() as u16;
        out.extend_from_slice(input_class);
        patch_u16(&mut out, class_off_pos, input_off);
    }

    append_rule_sets(&mut out, sets_pos, chained, rule_sets, index_map);
    out
}

/// Serialise a `SequenceContextFormat3` / `ChainedSequenceContextFormat3`.
fn build_format3(
    chained: bool,
    back: &[Vec<u8>],
    input: &[Vec<u8>],
    ahead: &[Vec<u8>],
    records: &[(u16, u16)],
) -> Vec<u8> {
    let mut out = Vec::new();
    w_u16(&mut out, 3); // format
    let mut off_positions: Vec<(usize, &[Vec<u8>])> = Vec::with_capacity(3);
    if chained {
        w_u16(&mut out, back.len() as u16);
        let p = out.len();
        for _ in back {
            w_u16(&mut out, 0);
        }
        off_positions.push((p, back));
    }
    w_u16(&mut out, input.len() as u16);
    let input_pos = out.len();
    for _ in input {
        w_u16(&mut out, 0);
    }
    off_positions.push((input_pos, input));
    if chained {
        w_u16(&mut out, ahead.len() as u16);
        let p = out.len();
        for _ in ahead {
            w_u16(&mut out, 0);
        }
        off_positions.push((p, ahead));
    }
    w_u16(&mut out, records.len() as u16);
    for &(seq, lk) in records {
        w_u16(&mut out, seq);
        w_u16(&mut out, lk);
    }
    for (pos, covs) in off_positions {
        for (i, blob) in covs.iter().enumerate() {
            let off = out.len() as u16;
            patch_u16(&mut out, pos + i * 2, off);
            out.extend_from_slice(blob);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn w16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn identity_remap(gids: &[u16]) -> HashMap<u16, u16> {
        gids.iter().map(|&g| (g, g)).collect()
    }

    /// SequenceContextFormat1 with a single rule set of one 3-glyph rule.
    fn build_seq_ctx_f1() -> Vec<u8> {
        // Rule: glyphCount=3, seqLookupCount=1, input=[20,30], record=(0,4)
        let mut rule = Vec::new();
        w16(&mut rule, 3);
        w16(&mut rule, 1);
        w16(&mut rule, 20);
        w16(&mut rule, 30);
        w16(&mut rule, 0);
        w16(&mut rule, 4);

        let mut rule_set = Vec::new();
        w16(&mut rule_set, 1); // seqRuleCount
        w16(&mut rule_set, 4); // offset to rule
        rule_set.extend_from_slice(&rule);

        // Header: format(2) covOff(2) count(2) offsets(2) = 8
        let mut out = Vec::new();
        w16(&mut out, 1);
        w16(&mut out, 8 + 4); // coverage placed after the rule set... patched below
        w16(&mut out, 1);
        w16(&mut out, 8); // ruleSetOffset
        out.extend_from_slice(&rule_set);
        let cov_off = out.len() as u16;
        // Patch coverage offset now that we know where it lands.
        out[2] = (cov_off >> 8) as u8;
        out[3] = (cov_off & 0xFF) as u8;
        w16(&mut out, 1); // coverage format 1
        w16(&mut out, 1);
        w16(&mut out, 10);
        out
    }

    #[test]
    fn format1_round_trip_preserves_rule() {
        let bytes = build_seq_ctx_f1();
        let remap = identity_remap(&[10, 20, 30]);
        let st = parse_context_subtable(&bytes, 0, &remap, false).expect("parse");
        match &st.body {
            ContextBody::Format1 {
                coverage,
                rule_sets,
            } => {
                assert_eq!(coverage, &vec![10]);
                assert_eq!(rule_sets.len(), 1);
                assert_eq!(rule_sets[0][0].input, vec![20, 30]);
                assert_eq!(rule_sets[0][0].records, vec![(0, 4)]);
            }
            other => panic!("expected format 1, got {other:?}"),
        }
        // Re-serialising with an identity index map must parse back identically.
        let out = st.serialize(None);
        let st2 = parse_context_subtable(&out, 0, &remap, false).expect("reparse");
        assert_eq!(st, st2);
    }

    #[test]
    fn format1_drops_rule_whose_glyph_left_the_subset() {
        let bytes = build_seq_ctx_f1();
        // Glyph 30 is gone → the only rule can never match → subtable dropped.
        let remap = identity_remap(&[10, 20]);
        assert!(parse_context_subtable(&bytes, 0, &remap, false).is_none());
    }

    #[test]
    fn format1_lookup_index_is_remapped_and_pruned() {
        let bytes = build_seq_ctx_f1();
        let remap = identity_remap(&[10, 20, 30]);
        let st = parse_context_subtable(&bytes, 0, &remap, false).expect("parse");

        // Lookup 4 survives as new index 1.
        let mut map = vec![None; 5];
        map[4] = Some(1);
        let out = st.serialize(Some(&map));
        let st2 = parse_context_subtable(&out, 0, &remap, false).expect("reparse");
        match &st2.body {
            ContextBody::Format1 { rule_sets, .. } => {
                assert_eq!(rule_sets[0][0].records, vec![(0, 1)]);
            }
            other => panic!("expected format 1, got {other:?}"),
        }

        // Lookup 4 dropped → the rule loses its only record → rule set empties
        // → the whole subtable becomes an unusable no-op (empty coverage list
        // is impossible here, but the rule set offset must be NULL).
        let map_dropped = vec![None; 5];
        let out2 = st.serialize(Some(&map_dropped));
        // ruleSetOffsets[0] lives at byte 6 and must be NULL.
        assert_eq!(u16::from_be_bytes([out2[6], out2[7]]), 0);
    }
}
