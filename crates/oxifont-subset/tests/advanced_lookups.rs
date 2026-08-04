//! Per-format tests for the advanced OpenType layout lookups.
//!
//! Covers what earlier waves dropped outright:
//! - `SequenceContext` / `ChainedSequenceContext` **formats 1 and 2**
//!   (GSUB 5/6, GPOS 7/8) — glyph rule sets and class rule sets,
//! - contextual lookups wrapped in an **Extension** subtable (GSUB 7 / GPOS 9),
//! - **GSUB type 8** `ReverseChainSingleSubst`,
//! - **GPOS type 3** `CursivePos`,
//! - **GPOS type 5** `MarkLigPos`.
//!
//! Every table here is assembled byte-by-byte from the OpenType spec layouts so
//! the assertions check real on-disk structure, not a round-trip of our own
//! writer.

use oxifont_subset::otl::rewrite_gsub;
use oxifont_subset::otl_gpos::rewrite_gpos;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Byte helpers
// ---------------------------------------------------------------------------

fn w16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn w32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn patch16(out: &mut [u8], pos: usize, v: u16) {
    out[pos] = (v >> 8) as u8;
    out[pos + 1] = (v & 0xFF) as u8;
}

fn r16(data: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([data[off], data[off + 1]])
}

/// Coverage format 1 over a sorted GID list.
fn coverage_f1(gids: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    w16(&mut v, 1);
    w16(&mut v, gids.len() as u16);
    for &g in gids {
        w16(&mut v, g);
    }
    v
}

/// ClassDef format 1 assigning `classes[i]` to `start + i`.
fn classdef_f1(start: u16, classes: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    w16(&mut v, 1);
    w16(&mut v, start);
    w16(&mut v, classes.len() as u16);
    for &c in classes {
        w16(&mut v, c);
    }
    v
}

/// Resolve a glyph's class from either ClassDef format.
fn class_of(data: &[u8], off: usize, gid: u16) -> u16 {
    match r16(data, off) {
        1 => {
            let start = r16(data, off + 2);
            let count = r16(data, off + 4);
            if gid >= start && gid < start + count {
                r16(data, off + 6 + (gid - start) as usize * 2)
            } else {
                0
            }
        }
        2 => {
            let n = r16(data, off + 2) as usize;
            for i in 0..n {
                let s = r16(data, off + 4 + i * 6);
                let e = r16(data, off + 6 + i * 6);
                let c = r16(data, off + 8 + i * 6);
                if gid >= s && gid <= e {
                    return c;
                }
            }
            0
        }
        _ => 0,
    }
}

/// Anchor format 1 (x, y).
fn anchor_f1(x: i16, y: i16) -> Vec<u8> {
    let mut v = Vec::new();
    w16(&mut v, 1);
    w16(&mut v, x as u16);
    w16(&mut v, y as u16);
    v
}

/// Anchor format 3 (x, y) with an x Device table placed right after the
/// 10-byte header. `deltaFormat = 1` (2-bit deltas) over `startSize..=endSize`.
fn anchor_f3_with_x_device(x: i16, y: i16, start: u16, end: u16, deltas: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    w16(&mut v, 3);
    w16(&mut v, x as u16);
    w16(&mut v, y as u16);
    w16(&mut v, 10); // xDeviceOffset — Device sits immediately after the header
    w16(&mut v, 0); // yDeviceOffset = NULL
    w16(&mut v, start);
    w16(&mut v, end);
    w16(&mut v, 1); // deltaFormat = LOCAL_2_BIT_DELTAS
    for &d in deltas {
        w16(&mut v, d);
    }
    v
}

// ---------------------------------------------------------------------------
// SFL scaffolding (GSUB and GPOS share the layout)
// ---------------------------------------------------------------------------

/// Frame a subtable into a Lookup: type(2) + flag(2) + subCount(2) + offset(2).
fn build_single_lookup(lookup_type: u16, subtable: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, lookup_type);
    w16(&mut out, 0);
    w16(&mut out, 1);
    w16(&mut out, 8);
    out.extend_from_slice(subtable);
    out
}

/// Assemble a GSUB/GPOS table whose single feature references lookup 0.
fn build_layout_table(lookups: &[Vec<u8>]) -> Vec<u8> {
    let n = lookups.len();
    let mut ll = Vec::new();
    w16(&mut ll, n as u16);
    let off_array = ll.len();
    for _ in 0..n {
        w16(&mut ll, 0);
    }
    let mut offs = Vec::new();
    for lk in lookups {
        offs.push(ll.len() as u16);
        ll.extend_from_slice(lk);
    }
    for (i, &o) in offs.iter().enumerate() {
        patch16(&mut ll, off_array + i * 2, o);
    }

    let mut feat = Vec::new();
    w16(&mut feat, 0);
    w16(&mut feat, 1);
    w16(&mut feat, 0);
    let mut fl = Vec::new();
    w16(&mut fl, 1);
    fl.extend_from_slice(b"calt");
    w16(&mut fl, 2 + 6);
    fl.extend_from_slice(&feat);

    let mut dls = Vec::new();
    w16(&mut dls, 0);
    w16(&mut dls, 0xFFFF);
    w16(&mut dls, 1);
    w16(&mut dls, 0);
    let mut sc = Vec::new();
    w16(&mut sc, 4);
    w16(&mut sc, 0);
    sc.extend_from_slice(&dls);
    let mut sl = Vec::new();
    w16(&mut sl, 1);
    sl.extend_from_slice(b"DFLT");
    w16(&mut sl, 2 + 6);
    sl.extend_from_slice(&sc);

    let sl_off = 10u16;
    let fl_off = sl_off + sl.len() as u16;
    let ll_off = fl_off + fl.len() as u16;
    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, 0);
    w16(&mut out, sl_off);
    w16(&mut out, fl_off);
    w16(&mut out, ll_off);
    out.extend_from_slice(&sl);
    out.extend_from_slice(&fl);
    out.extend_from_slice(&ll);
    out
}

fn lookup_count(table: &[u8]) -> u16 {
    let ll_off = r16(table, 8) as usize;
    r16(table, ll_off)
}

fn lookup_offset(table: &[u8], idx: usize) -> usize {
    let ll_off = r16(table, 8) as usize;
    ll_off + r16(table, ll_off + 2 + idx * 2) as usize
}

/// Absolute offset of subtable `st` of lookup `idx`.
fn subtable_offset(table: &[u8], idx: usize, st: usize) -> usize {
    let lk = lookup_offset(table, idx);
    lk + r16(table, lk + 6 + st * 2) as usize
}

/// SingleSubst format 2 over the given (gid → substitute) pairs.
fn single_subst_f2(pairs: &[(u16, u16)]) -> Vec<u8> {
    let n = pairs.len() as u16;
    let mut out = Vec::new();
    w16(&mut out, 2);
    w16(&mut out, 6 + n * 2);
    w16(&mut out, n);
    for &(_, s) in pairs {
        w16(&mut out, s);
    }
    out.extend_from_slice(&coverage_f1(&pairs.iter().map(|p| p.0).collect::<Vec<_>>()));
    out
}

/// A remap that keeps the listed `(old, new)` pairs.
fn remap(pairs: &[(u16, u16)]) -> HashMap<u16, u16> {
    pairs.iter().copied().collect()
}

// ---------------------------------------------------------------------------
// SequenceContext format 1 (GSUB type 5)
// ---------------------------------------------------------------------------

/// `SequenceContextFormat1` with one rule set holding one rule.
///
/// The rule's `glyphCount` counts the coverage-matched glyph, so `input` here
/// carries only the 2nd..Nth positions.
fn seq_context_f1(first: u16, input: &[u16], records: &[(u16, u16)]) -> Vec<u8> {
    let mut rule = Vec::new();
    w16(&mut rule, (input.len() + 1) as u16); // glyphCount
    w16(&mut rule, records.len() as u16); // seqLookupCount
    for &g in input {
        w16(&mut rule, g);
    }
    for &(s, l) in records {
        w16(&mut rule, s);
        w16(&mut rule, l);
    }

    let mut rule_set = Vec::new();
    w16(&mut rule_set, 1); // seqRuleCount
    w16(&mut rule_set, 4); // seqRuleOffsets[0]
    rule_set.extend_from_slice(&rule);

    // Header: format(2) + coverageOffset(2) + count(2) + offsets(2) = 8
    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, 0); // coverageOffset, patched below
    w16(&mut out, 1); // seqRuleSetCount
    w16(&mut out, 8); // seqRuleSetOffsets[0]
    out.extend_from_slice(&rule_set);
    let cov_at = out.len() as u16;
    patch16(&mut out, 2, cov_at);
    out.extend_from_slice(&coverage_f1(&[first]));
    out
}

#[test]
fn seq_context_format1_remaps_rule_glyphs_and_lookup_index() {
    // Lookup 0: type 5, coverage [6], rule "6 7 8" → apply lookup 2 at pos 0.
    // Lookup 1: dropped (its glyph leaves the subset) → renumbers lookup 2 to 1.
    // Lookup 2: survives.
    let lk0 = build_single_lookup(5, &seq_context_f1(6, &[7, 8], &[(0, 2)]));
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(90, 91)]));
    let lk2 = build_single_lookup(1, &single_subst_f2(&[(6, 8)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1), (7, 3), (8, 2)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(lookup_count(&out), 2, "context lookup must survive");
    let lk = lookup_offset(&out, 0);
    assert_eq!(r16(&out, lk), 5, "lookup 0 is still type 5");

    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "format 1 preserved");
    assert_eq!(r16(&out, st + 4), 1, "one rule set");

    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov), 1, "coverage format 1");
    assert_eq!(r16(&out, cov + 2), 1, "one covered glyph");
    assert_eq!(r16(&out, cov + 4), 1, "coverage GID remapped 6 → 1");

    let rs = st + r16(&out, st + 6) as usize;
    assert_eq!(r16(&out, rs), 1, "one rule in the set");
    let rule = rs + r16(&out, rs + 2) as usize;
    assert_eq!(
        r16(&out, rule),
        3,
        "glyphCount still counts all 3 positions"
    );
    assert_eq!(r16(&out, rule + 2), 1, "one seqLookupRecord");
    assert_eq!(r16(&out, rule + 4), 3, "input[0] remapped 7 → 3");
    assert_eq!(r16(&out, rule + 6), 2, "input[1] remapped 8 → 2");
    assert_eq!(r16(&out, rule + 8), 0, "sequenceIndex preserved");
    assert_eq!(r16(&out, rule + 10), 1, "lookupListIndex remapped 2 → 1");
}

#[test]
fn seq_context_format1_drops_rule_whose_glyph_left_the_subset() {
    // GID 7 sits in the middle of the rule; without it the rule can never match
    // and the whole (single-rule) lookup must go.
    let lk0 = build_single_lookup(5, &seq_context_f1(6, &[7, 8], &[(0, 1)]));
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(6, 8)]));
    let table = build_layout_table(&[lk0, lk1]);

    let map = remap(&[(6, 1), (8, 2)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(lookup_count(&out), 1, "context lookup must drop");
    assert_eq!(r16(&out, lookup_offset(&out, 0)), 1, "single subst remains");
}

#[test]
fn seq_context_format1_prunes_record_when_target_lookup_is_dropped() {
    // The rule's only record targets lookup 1, which is dropped. The record
    // must be pruned — never remapped to some other lookup's index.
    let lk0 = build_single_lookup(5, &seq_context_f1(6, &[7], &[(0, 1)]));
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(90, 91)]));
    let lk2 = build_single_lookup(1, &single_subst_f2(&[(6, 8)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1), (7, 3), (8, 2)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(lookup_count(&out), 2);
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "format 1 preserved");
    assert_eq!(r16(&out, st + 4), 1, "rule-set slot still present");
    assert_eq!(
        r16(&out, st + 6),
        0,
        "rule set must be NULL once its only record is pruned"
    );
}

// ---------------------------------------------------------------------------
// ChainedSequenceContext format 2 (GSUB type 6)
// ---------------------------------------------------------------------------

/// `ChainedSequenceContextFormat2` with `rule_set_count` slots, a single rule
/// in slot `rule_class`.
fn chained_context_f2(
    cov: &[u16],
    class_start: u16,
    classes: &[u16],
    rule_set_count: u16,
    rule_class: u16,
    input_classes: &[u16],
    records: &[(u16, u16)],
) -> Vec<u8> {
    let mut rule = Vec::new();
    w16(&mut rule, 0); // backtrackGlyphCount
    w16(&mut rule, (input_classes.len() + 1) as u16); // inputGlyphCount
    for &c in input_classes {
        w16(&mut rule, c);
    }
    w16(&mut rule, 0); // lookaheadGlyphCount
    w16(&mut rule, records.len() as u16);
    for &(s, l) in records {
        w16(&mut rule, s);
        w16(&mut rule, l);
    }

    let mut rule_set = Vec::new();
    w16(&mut rule_set, 1);
    w16(&mut rule_set, 4);
    rule_set.extend_from_slice(&rule);

    // Header: format(2) cov(2) backCD(2) inCD(2) aheadCD(2) count(2) offsets
    let mut out = Vec::new();
    w16(&mut out, 2);
    w16(&mut out, 0); // coverageOffset
    w16(&mut out, 0); // backtrackClassDefOffset
    w16(&mut out, 0); // inputClassDefOffset
    w16(&mut out, 0); // lookaheadClassDefOffset
    w16(&mut out, rule_set_count);
    let sets_pos = out.len();
    for _ in 0..rule_set_count {
        w16(&mut out, 0);
    }

    let cov_at = out.len() as u16;
    patch16(&mut out, 2, cov_at);
    out.extend_from_slice(&coverage_f1(cov));

    let cd = classdef_f1(class_start, classes);
    let cd_at = out.len() as u16;
    patch16(&mut out, 4, cd_at); // backtrack shares the same ClassDef
    patch16(&mut out, 6, cd_at);
    patch16(&mut out, 8, cd_at);
    out.extend_from_slice(&cd);

    let rs_at = out.len() as u16;
    patch16(&mut out, sets_pos + rule_class as usize * 2, rs_at);
    out.extend_from_slice(&rule_set);
    out
}

#[test]
fn chained_context_format2_remaps_classdefs_and_keeps_class_values() {
    // Coverage [6,7], both class 1. Rule in class-1 slot: input class [1].
    let sub = chained_context_f2(&[6, 7], 6, &[1, 1], 2, 1, &[1], &[(1, 2)]);
    let lk0 = build_single_lookup(6, &sub);
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(90, 91)]));
    let lk2 = build_single_lookup(1, &single_subst_f2(&[(6, 8)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1), (7, 3), (8, 2)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(lookup_count(&out), 2, "format-2 context must survive");
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 2, "format 2 preserved");

    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov + 2), 2, "two covered glyphs");

    let in_cd = st + r16(&out, st + 6) as usize;
    assert_eq!(class_of(&out, in_cd, 1), 1, "new GID 1 keeps class 1");
    assert_eq!(class_of(&out, in_cd, 3), 1, "new GID 3 keeps class 1");
    assert_eq!(
        class_of(&out, in_cd, 6),
        0,
        "old GID 6 no longer classified"
    );

    assert_eq!(r16(&out, st + 10), 2, "rule-set count preserved");
    assert_eq!(r16(&out, st + 12), 0, "class-0 rule set still NULL");
    let rs = st + r16(&out, st + 14) as usize;
    let rule = rs + r16(&out, rs + 2) as usize;
    assert_eq!(r16(&out, rule), 0, "backtrackGlyphCount");
    assert_eq!(r16(&out, rule + 2), 2, "inputGlyphCount");
    assert_eq!(r16(&out, rule + 4), 1, "input class value untouched");
    assert_eq!(r16(&out, rule + 6), 0, "lookaheadGlyphCount");
    assert_eq!(r16(&out, rule + 8), 1, "one record");
    assert_eq!(r16(&out, rule + 10), 1, "sequenceIndex preserved");
    assert_eq!(r16(&out, rule + 12), 1, "lookupListIndex remapped 2 → 1");
}

// ---------------------------------------------------------------------------
// Extension-wrapped contextual lookups (GSUB type 7 → 6)
// ---------------------------------------------------------------------------

fn extension_subtable(ext_type: u16, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, ext_type);
    w32(&mut out, 8);
    out.extend_from_slice(inner);
    out
}

/// `ChainedSequenceContextFormat3` with only an input coverage.
fn chained_context_f3(input: &[u16], records: &[(u16, u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, 3);
    w16(&mut out, 0); // backtrackGlyphCount
    w16(&mut out, 1); // inputGlyphCount
    let cov_pos = out.len();
    w16(&mut out, 0);
    w16(&mut out, 0); // lookaheadGlyphCount
    w16(&mut out, records.len() as u16);
    for &(s, l) in records {
        w16(&mut out, s);
        w16(&mut out, l);
    }
    let cov_at = out.len() as u16;
    patch16(&mut out, cov_pos, cov_at);
    out.extend_from_slice(&coverage_f1(input));
    out
}

#[test]
fn extension_wrapped_chained_context_survives_with_index_fixup() {
    let inner = chained_context_f3(&[6], &[(0, 2)]);
    let lk0 = build_single_lookup(7, &extension_subtable(6, &inner));
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(90, 91)]));
    let lk2 = build_single_lookup(1, &single_subst_f2(&[(6, 8)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1), (8, 2)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(
        lookup_count(&out),
        2,
        "extension-wrapped context must survive"
    );
    let lk = lookup_offset(&out, 0);
    assert_eq!(r16(&out, lk), 7, "outer lookup is still Extension");

    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "extension format 1");
    assert_eq!(r16(&out, st + 2), 6, "extensionLookupType preserved");
    let ext_off = u32::from_be_bytes([out[st + 4], out[st + 5], out[st + 6], out[st + 7]]) as usize;
    let inner_off = st + ext_off;

    assert_eq!(r16(&out, inner_off), 3, "inner subtable format 3");
    assert_eq!(r16(&out, inner_off + 10), 1, "seqLookupCount");
    assert_eq!(
        r16(&out, inner_off + 14),
        1,
        "inner lookupListIndex remapped 2 → 1 through the Extension wrapper"
    );
    let cov = inner_off + r16(&out, inner_off + 6) as usize;
    assert_eq!(r16(&out, cov + 4), 1, "inner coverage remapped 6 → 1");
}

// ---------------------------------------------------------------------------
// ReverseChainSingleSubst (GSUB type 8)
// ---------------------------------------------------------------------------

fn reverse_chain_single_subst(
    cov: &[u16],
    backtrack: &[&[u16]],
    lookahead: &[&[u16]],
    substitutes: &[u16],
) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, 1); // substFormat
    let cov_pos = out.len();
    w16(&mut out, 0);
    w16(&mut out, backtrack.len() as u16);
    let back_pos = out.len();
    for _ in backtrack {
        w16(&mut out, 0);
    }
    w16(&mut out, lookahead.len() as u16);
    let ahead_pos = out.len();
    for _ in lookahead {
        w16(&mut out, 0);
    }
    w16(&mut out, substitutes.len() as u16);
    for &s in substitutes {
        w16(&mut out, s);
    }

    let cov_at = out.len() as u16;
    patch16(&mut out, cov_pos, cov_at);
    out.extend_from_slice(&coverage_f1(cov));
    for (i, gids) in backtrack.iter().enumerate() {
        let at = out.len() as u16;
        patch16(&mut out, back_pos + i * 2, at);
        out.extend_from_slice(&coverage_f1(gids));
    }
    for (i, gids) in lookahead.iter().enumerate() {
        let at = out.len() as u16;
        patch16(&mut out, ahead_pos + i * 2, at);
        out.extend_from_slice(&coverage_f1(gids));
    }
    out
}

#[test]
fn reverse_chain_single_subst_is_remapped() {
    let sub = reverse_chain_single_subst(&[6, 7], &[&[8]], &[], &[10, 11]);
    let table = build_layout_table(&[build_single_lookup(8, &sub)]);

    let map = remap(&[(6, 1), (7, 3), (8, 2), (10, 4), (11, 5)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(lookup_count(&out), 1, "reverse-chain lookup must survive");
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "substFormat 1");
    assert_eq!(r16(&out, st + 4), 1, "backtrackGlyphCount");
    let back_cov = st + r16(&out, st + 6) as usize;
    assert_eq!(r16(&out, back_cov + 4), 2, "backtrack coverage 8 → 2");
    assert_eq!(r16(&out, st + 8), 0, "lookaheadGlyphCount");
    assert_eq!(r16(&out, st + 10), 2, "glyphCount");
    assert_eq!(r16(&out, st + 12), 4, "substitute 10 → 4");
    assert_eq!(r16(&out, st + 14), 5, "substitute 11 → 5");
    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov + 2), 2, "two covered glyphs");
    assert_eq!(r16(&out, cov + 4), 1, "coverage 6 → 1");
    assert_eq!(r16(&out, cov + 6), 3, "coverage 7 → 3");
}

#[test]
fn reverse_chain_drops_entry_whose_substitute_left_the_subset() {
    let sub = reverse_chain_single_subst(&[6, 7], &[], &[], &[10, 11]);
    let table = build_layout_table(&[build_single_lookup(8, &sub)]);

    // Substitute 11 is gone → the 7 → 11 entry cannot be kept.
    let map = remap(&[(6, 1), (7, 3), (10, 4)]);
    let out = rewrite_gsub(&table, &map);

    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st + 8), 1, "glyphCount reduced to 1");
    assert_eq!(r16(&out, st + 10), 4, "surviving substitute 10 → 4");
    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov + 2), 1, "one covered glyph left");
    assert_eq!(r16(&out, cov + 4), 1, "coverage 6 → 1");
}

#[test]
fn reverse_chain_dropped_when_backtrack_coverage_empties() {
    let sub = reverse_chain_single_subst(&[6], &[&[42]], &[], &[10]);
    let table = build_layout_table(&[build_single_lookup(8, &sub)]);

    // GID 42 (backtrack) is gone → the rule can never match.
    let map = remap(&[(6, 1), (10, 4)]);
    let out = rewrite_gsub(&table, &map);
    assert_eq!(lookup_count(&out), 0, "unmatched reverse chain must drop");
}

// ---------------------------------------------------------------------------
// CursivePos (GPOS type 3)
// ---------------------------------------------------------------------------

/// `(gid, entry_anchor, exit_anchor)`; `None` means a NULL anchor offset.
type CursiveEntry = (u16, Option<Vec<u8>>, Option<Vec<u8>>);

/// `CursivePosFormat1` built from the entry/exit records above.
fn cursive_pos(entries: &[CursiveEntry]) -> Vec<u8> {
    let n = entries.len();
    let mut out = Vec::new();
    w16(&mut out, 1); // posFormat
    let cov_pos = out.len();
    w16(&mut out, 0);
    w16(&mut out, n as u16);
    let rec_pos = out.len();
    for _ in 0..n {
        w16(&mut out, 0);
        w16(&mut out, 0);
    }
    let cov_at = out.len() as u16;
    patch16(&mut out, cov_pos, cov_at);
    out.extend_from_slice(&coverage_f1(
        &entries.iter().map(|e| e.0).collect::<Vec<_>>(),
    ));
    for (i, (_, entry, exit)) in entries.iter().enumerate() {
        if let Some(a) = entry {
            let at = out.len() as u16;
            patch16(&mut out, rec_pos + i * 4, at);
            out.extend_from_slice(a);
        }
        if let Some(a) = exit {
            let at = out.len() as u16;
            patch16(&mut out, rec_pos + i * 4 + 2, at);
            out.extend_from_slice(a);
        }
    }
    out
}

#[test]
fn cursive_pos_is_remapped_and_preserves_null_anchors() {
    let sub = cursive_pos(&[
        (6, Some(anchor_f1(10, 20)), None),
        (7, None, Some(anchor_f1(30, 40))),
        (42, Some(anchor_f1(50, 60)), Some(anchor_f1(70, 80))),
    ]);
    let table = build_layout_table(&[build_single_lookup(3, &sub)]);

    // GID 42 leaves the subset; 6 → 1 and 7 → 3 stay.
    let map = remap(&[(6, 1), (7, 3)]);
    let out = rewrite_gpos(&table, &map);

    assert_eq!(lookup_count(&out), 1, "cursive lookup must survive");
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "posFormat 1");
    assert_eq!(r16(&out, st + 4), 2, "entryExitCount pruned to 2");

    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov + 2), 2, "two covered glyphs");
    assert_eq!(r16(&out, cov + 4), 1, "coverage 6 → 1");
    assert_eq!(r16(&out, cov + 6), 3, "coverage 7 → 3");

    // Record 0: entry anchor (10, 20), NULL exit.
    let e0 = r16(&out, st + 6) as usize;
    assert_ne!(e0, 0, "entry anchor must be present");
    assert_eq!(r16(&out, st + e0), 1, "anchor format 1");
    assert_eq!(r16(&out, st + e0 + 2), 10, "anchor x");
    assert_eq!(r16(&out, st + e0 + 4), 20, "anchor y");
    assert_eq!(r16(&out, st + 8), 0, "NULL exit anchor stays NULL");

    // Record 1: NULL entry, exit anchor (30, 40).
    assert_eq!(r16(&out, st + 10), 0, "NULL entry anchor stays NULL");
    let x1 = r16(&out, st + 12) as usize;
    assert_ne!(x1, 0, "exit anchor must be present");
    assert_eq!(r16(&out, st + x1 + 2), 30, "anchor x");
    assert_eq!(r16(&out, st + x1 + 4), 40, "anchor y");
}

#[test]
fn cursive_pos_dropped_when_all_glyphs_leave() {
    let sub = cursive_pos(&[(42, Some(anchor_f1(10, 20)), None)]);
    let table = build_layout_table(&[build_single_lookup(3, &sub)]);
    let out = rewrite_gpos(&table, &remap(&[(6, 1)]));
    assert_eq!(lookup_count(&out), 0, "empty cursive lookup must drop");
}

// ---------------------------------------------------------------------------
// MarkLigPos (GPOS type 5)
// ---------------------------------------------------------------------------

/// `MarkLigPosFormat1`.
///
/// `marks` is `(gid, markClass, anchor)`; `ligs` is
/// `(gid, components[component][markClass])` with `None` for NULL anchors.
#[allow(clippy::type_complexity)]
fn mark_lig_pos(
    marks: &[(u16, u16, Vec<u8>)],
    ligs: &[(u16, Vec<Vec<Option<Vec<u8>>>>)],
    mark_class_count: usize,
) -> Vec<u8> {
    // MarkArray
    let mut ma = Vec::new();
    w16(&mut ma, marks.len() as u16);
    let ma_rec = ma.len();
    for _ in marks {
        w16(&mut ma, 0);
        w16(&mut ma, 0);
    }
    for (i, (_, class, anc)) in marks.iter().enumerate() {
        let at = ma.len() as u16;
        patch16(&mut ma, ma_rec + i * 4, *class);
        patch16(&mut ma, ma_rec + i * 4 + 2, at);
        ma.extend_from_slice(anc);
    }

    // LigatureArray
    let mut la = Vec::new();
    w16(&mut la, ligs.len() as u16);
    let la_off = la.len();
    for _ in ligs {
        w16(&mut la, 0);
    }
    for (i, (_, comps)) in ligs.iter().enumerate() {
        let mut attach = Vec::new();
        w16(&mut attach, comps.len() as u16);
        let slots = attach.len();
        for _ in 0..(comps.len() * mark_class_count) {
            w16(&mut attach, 0);
        }
        for (c, comp) in comps.iter().enumerate() {
            for (k, anc) in comp.iter().enumerate() {
                if let Some(a) = anc {
                    let at = attach.len() as u16;
                    patch16(&mut attach, slots + (c * mark_class_count + k) * 2, at);
                    attach.extend_from_slice(a);
                }
            }
        }
        let at = la.len() as u16;
        patch16(&mut la, la_off + i * 2, at);
        la.extend_from_slice(&attach);
    }

    let mark_cov = coverage_f1(&marks.iter().map(|m| m.0).collect::<Vec<_>>());
    let lig_cov = coverage_f1(&ligs.iter().map(|l| l.0).collect::<Vec<_>>());
    let header = 12u16;
    let mark_cov_at = header;
    let lig_cov_at = mark_cov_at + mark_cov.len() as u16;
    let ma_at = lig_cov_at + lig_cov.len() as u16;
    let la_at = ma_at + ma.len() as u16;

    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, mark_cov_at);
    w16(&mut out, lig_cov_at);
    w16(&mut out, mark_class_count as u16);
    w16(&mut out, ma_at);
    w16(&mut out, la_at);
    out.extend_from_slice(&mark_cov);
    out.extend_from_slice(&lig_cov);
    out.extend_from_slice(&ma);
    out.extend_from_slice(&la);
    out
}

#[test]
fn mark_lig_pos_is_remapped_and_preserves_null_component_anchors() {
    let marks = vec![
        (20u16, 0u16, anchor_f1(1, 2)),
        (21u16, 1u16, anchor_f1(3, 4)),
        (99u16, 0u16, anchor_f1(5, 6)), // leaves the subset
    ];
    let ligs = vec![(
        6u16,
        vec![
            vec![Some(anchor_f1(11, 12)), None],
            vec![None, Some(anchor_f1(13, 14))],
        ],
    )];
    let sub = mark_lig_pos(&marks, &ligs, 2);
    let table = build_layout_table(&[build_single_lookup(5, &sub)]);

    let map = remap(&[(20, 7), (21, 8), (6, 1)]);
    let out = rewrite_gpos(&table, &map);

    assert_eq!(lookup_count(&out), 1, "MarkLigPos lookup must survive");
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "posFormat 1");
    assert_eq!(r16(&out, st + 6), 2, "markClassCount preserved");

    let mark_cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, mark_cov + 2), 2, "mark 99 pruned");
    assert_eq!(r16(&out, mark_cov + 4), 7, "mark 20 → 7");
    assert_eq!(r16(&out, mark_cov + 6), 8, "mark 21 → 8");

    let lig_cov = st + r16(&out, st + 4) as usize;
    assert_eq!(r16(&out, lig_cov + 2), 1, "one ligature");
    assert_eq!(r16(&out, lig_cov + 4), 1, "ligature 6 → 1");

    // MarkArray: classes and anchors follow the pruned coverage order.
    let ma = st + r16(&out, st + 8) as usize;
    assert_eq!(r16(&out, ma), 2, "two mark records");
    assert_eq!(r16(&out, ma + 2), 0, "mark 20 keeps class 0");
    let m0 = ma + r16(&out, ma + 4) as usize;
    assert_eq!(r16(&out, m0 + 2), 1, "mark 20 anchor x");
    assert_eq!(r16(&out, ma + 6), 1, "mark 21 keeps class 1");
    let m1 = ma + r16(&out, ma + 8) as usize;
    assert_eq!(r16(&out, m1 + 2), 3, "mark 21 anchor x");

    // LigatureArray → LigatureAttach with 2 components × 2 classes.
    let la = st + r16(&out, st + 10) as usize;
    assert_eq!(r16(&out, la), 1, "one LigatureAttach");
    let at = la + r16(&out, la + 2) as usize;
    assert_eq!(r16(&out, at), 2, "componentCount");
    let a00 = r16(&out, at + 2) as usize;
    assert_ne!(a00, 0, "component 0 / class 0 anchor present");
    assert_eq!(r16(&out, at + a00 + 2), 11, "component 0 anchor x");
    assert_eq!(r16(&out, at + 4), 0, "component 0 / class 1 stays NULL");
    assert_eq!(r16(&out, at + 6), 0, "component 1 / class 0 stays NULL");
    let a11 = r16(&out, at + 8) as usize;
    assert_ne!(a11, 0, "component 1 / class 1 anchor present");
    assert_eq!(r16(&out, at + a11 + 2), 13, "component 1 anchor x");
}

// ---------------------------------------------------------------------------
// Contextual GPOS (types 7 / 8) and Extension-wrapped GPOS context
// ---------------------------------------------------------------------------

#[test]
fn gpos_chained_context_and_extension_wrapper_are_remapped() {
    // Lookup 0: type 9 Extension wrapping a type-8 chained context → lookup 2.
    // Lookup 1: dropped, so lookup 2 renumbers to 1.
    let inner = chained_context_f3(&[6], &[(0, 2)]);
    let lk0 = build_single_lookup(9, &extension_subtable(8, &inner));
    let lk1 = build_single_lookup(3, &cursive_pos(&[(99, Some(anchor_f1(1, 2)), None)]));
    let lk2 = build_single_lookup(3, &cursive_pos(&[(6, Some(anchor_f1(3, 4)), None)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1)]);
    let out = rewrite_gpos(&table, &map);

    assert_eq!(lookup_count(&out), 2, "extension + cursive survive");
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st + 2), 8, "extensionLookupType preserved");
    let ext_off = u32::from_be_bytes([out[st + 4], out[st + 5], out[st + 6], out[st + 7]]) as usize;
    let inner_off = st + ext_off;
    assert_eq!(r16(&out, inner_off), 3, "inner format 3");
    assert_eq!(
        r16(&out, inner_off + 14),
        1,
        "GPOS lookupListIndex remapped 2 → 1 through the Extension"
    );
}

// ---------------------------------------------------------------------------
// Format-3 anchors (Device tables) must travel with the anchor
// ---------------------------------------------------------------------------

#[test]
fn cursive_pos_format3_anchor_keeps_its_device_table() {
    // One 2-bit-delta Device covering sizes 12..=13 → 6 + 2 = 8 bytes, so the
    // whole anchor is 18 bytes. A naive "copy up to the next anchor offset"
    // heuristic would copy only 10 and leave xDeviceOffset dangling.
    let anchor = anchor_f3_with_x_device(10, 20, 12, 13, &[0x5000]);
    assert_eq!(
        anchor.len(),
        18,
        "test fixture: 10-byte header + 8-byte Device"
    );

    let sub = cursive_pos(&[(6, Some(anchor), None)]);
    let table = build_layout_table(&[build_single_lookup(3, &sub)]);
    let out = rewrite_gpos(&table, &remap(&[(6, 1)]));

    assert_eq!(lookup_count(&out), 1, "cursive lookup must survive");
    let st = subtable_offset(&out, 0, 0);
    let entry = r16(&out, st + 6) as usize;
    assert_ne!(entry, 0, "entry anchor present");
    let anc = st + entry;
    assert_eq!(r16(&out, anc), 3, "anchor format 3 preserved");
    let dev = r16(&out, anc + 6) as usize;
    assert_eq!(dev, 10, "xDeviceOffset still points just past the header");
    // The Device table must live inside the copied blob, not past the subtable.
    assert_eq!(r16(&out, anc + dev), 12, "Device startSize");
    assert_eq!(r16(&out, anc + dev + 2), 13, "Device endSize");
    assert_eq!(r16(&out, anc + dev + 4), 1, "Device deltaFormat");
    assert_eq!(
        r16(&out, anc + dev + 6),
        0x5000,
        "Device deltaValue survived"
    );
}

#[test]
fn cursive_pos_dropped_when_anchor_is_truncated() {
    // An anchor offset that points past the end of the table must drop the
    // subtable, not be silently turned into a NULL (unpositioned) anchor.
    let mut sub = cursive_pos(&[(6, Some(anchor_f1(10, 20)), None)]);
    let bogus = sub.len() as u16 + 4;
    patch16(&mut sub, 6, bogus); // entryAnchorOffset → past the subtable
    let table = build_layout_table(&[build_single_lookup(3, &sub)]);

    let out = rewrite_gpos(&table, &remap(&[(6, 1)]));
    assert_eq!(
        lookup_count(&out),
        0,
        "truncated anchor must drop the subtable, not fabricate a NULL anchor"
    );
}

// ---------------------------------------------------------------------------
// ChainedSequenceContext format 1 (GSUB type 6, glyph rule sets)
// ---------------------------------------------------------------------------

/// `ChainedSequenceContextFormat1` with one rule set holding one rule.
///
/// Field order per spec: backtrack count + sequence, input count + sequence
/// (minus the coverage-matched first glyph), lookahead count + sequence, then
/// the records.
fn chained_context_f1(
    first: u16,
    back: &[u16],
    input: &[u16],
    ahead: &[u16],
    records: &[(u16, u16)],
) -> Vec<u8> {
    let mut rule = Vec::new();
    w16(&mut rule, back.len() as u16);
    for &g in back {
        w16(&mut rule, g);
    }
    w16(&mut rule, (input.len() + 1) as u16);
    for &g in input {
        w16(&mut rule, g);
    }
    w16(&mut rule, ahead.len() as u16);
    for &g in ahead {
        w16(&mut rule, g);
    }
    w16(&mut rule, records.len() as u16);
    for &(s, l) in records {
        w16(&mut rule, s);
        w16(&mut rule, l);
    }

    let mut rule_set = Vec::new();
    w16(&mut rule_set, 1);
    w16(&mut rule_set, 4);
    rule_set.extend_from_slice(&rule);

    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, 0); // coverageOffset, patched below
    w16(&mut out, 1); // chainedSeqRuleSetCount
    w16(&mut out, 8); // chainedSeqRuleSetOffsets[0]
    out.extend_from_slice(&rule_set);
    let cov_at = out.len() as u16;
    patch16(&mut out, 2, cov_at);
    out.extend_from_slice(&coverage_f1(&[first]));
    out
}

#[test]
fn chained_context_format1_remaps_backtrack_input_and_lookahead() {
    // Rule: backtrack [20], input "6 7" (glyphCount 2), lookahead [21, 22].
    let sub = chained_context_f1(6, &[20], &[7], &[21, 22], &[(0, 2)]);
    let lk0 = build_single_lookup(6, &sub);
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(90, 91)]));
    let lk2 = build_single_lookup(1, &single_subst_f2(&[(6, 7)]));
    let table = build_layout_table(&[lk0, lk1, lk2]);

    let map = remap(&[(6, 1), (7, 2), (20, 30), (21, 31), (22, 32)]);
    let out = rewrite_gsub(&table, &map);

    assert_eq!(
        lookup_count(&out),
        2,
        "chained format-1 context must survive"
    );
    let st = subtable_offset(&out, 0, 0);
    assert_eq!(r16(&out, st), 1, "format 1 preserved");

    let cov = st + r16(&out, st + 2) as usize;
    assert_eq!(r16(&out, cov + 4), 1, "coverage remapped 6 → 1");

    let rs = st + r16(&out, st + 6) as usize;
    let rule = rs + r16(&out, rs + 2) as usize;
    assert_eq!(r16(&out, rule), 1, "backtrackGlyphCount");
    assert_eq!(r16(&out, rule + 2), 30, "backtrack 20 → 30");
    assert_eq!(
        r16(&out, rule + 4),
        2,
        "inputGlyphCount counts the first glyph"
    );
    assert_eq!(r16(&out, rule + 6), 2, "input[0] remapped 7 → 2");
    assert_eq!(r16(&out, rule + 8), 2, "lookaheadGlyphCount");
    assert_eq!(r16(&out, rule + 10), 31, "lookahead[0] remapped 21 → 31");
    assert_eq!(r16(&out, rule + 12), 32, "lookahead[1] remapped 22 → 32");
    assert_eq!(r16(&out, rule + 14), 1, "seqLookupCount");
    assert_eq!(r16(&out, rule + 16), 0, "sequenceIndex preserved");
    assert_eq!(r16(&out, rule + 18), 1, "lookupListIndex remapped 2 → 1");
}

#[test]
fn chained_context_format1_drops_rule_whose_lookahead_left_the_subset() {
    let sub = chained_context_f1(6, &[20], &[7], &[21], &[(0, 1)]);
    let lk0 = build_single_lookup(6, &sub);
    let lk1 = build_single_lookup(1, &single_subst_f2(&[(6, 7)]));
    let table = build_layout_table(&[lk0, lk1]);

    // GID 21 (lookahead) is gone → the rule, and with it the lookup, must drop.
    let map = remap(&[(6, 1), (7, 2), (20, 30)]);
    let out = rewrite_gsub(&table, &map);
    assert_eq!(lookup_count(&out), 1, "unmatched chained rule must drop");
    assert_eq!(r16(&out, lookup_offset(&out, 0)), 1, "single subst remains");
}
