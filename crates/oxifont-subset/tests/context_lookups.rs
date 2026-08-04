//! Integration tests for contextual / chaining-contextual GSUB rewriting.
//!
//! Prior to this fix, GSUB lookup types 5/6 (and GPOS 7/8) were mapped to
//! `None` and silently dropped during subsetting, losing nearly all real
//! shaping (Arabic joining, Indic reordering, Latin `calt`). Format-3
//! (coverage-based) contextual subtables are now remapped: coverage GIDs are
//! rewritten and the embedded `seqLookupRecords` have their `lookupListIndex`
//! fixed up against the old→new lookup index map.

use oxifont_subset::otl::rewrite_gsub;
use std::collections::HashMap;

fn w16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn patch16(out: &mut [u8], pos: usize, v: u16) {
    out[pos] = (v >> 8) as u8;
    out[pos + 1] = (v & 0xFF) as u8;
}

fn r16(data: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([data[off], data[off + 1]])
}

/// SingleSubst format 2 subtable mapping each `(gid → substitute)` pair.
fn build_single_subst_f2(pairs: &[(u16, u16)]) -> Vec<u8> {
    let n = pairs.len() as u16;
    let cov_off = 6 + n * 2;
    let mut out = Vec::new();
    w16(&mut out, 2); // format
    w16(&mut out, cov_off);
    w16(&mut out, n);
    for &(_, subst) in pairs {
        w16(&mut out, subst);
    }
    w16(&mut out, 1); // coverage format 1
    w16(&mut out, n);
    for &(gid, _) in pairs {
        w16(&mut out, gid);
    }
    out
}

/// ChainedSequenceContext format 3 with only an input coverage (no
/// backtrack / lookahead) and the given seqLookupRecords.
fn build_chain_context_f3(input_gids: &[u16], records: &[(u16, u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, 3); // format
    w16(&mut out, 0); // backtrackGlyphCount
    w16(&mut out, 1); // inputGlyphCount
    let input_off_pos = out.len();
    w16(&mut out, 0); // input coverage offset placeholder
    w16(&mut out, 0); // lookaheadGlyphCount
    w16(&mut out, records.len() as u16); // seqLookupCount
    for (seq, lk) in records {
        w16(&mut out, *seq);
        w16(&mut out, *lk);
    }
    let cov_off = out.len() as u16;
    patch16(&mut out, input_off_pos, cov_off);
    w16(&mut out, 1); // coverage format 1
    w16(&mut out, input_gids.len() as u16);
    for &g in input_gids {
        w16(&mut out, g);
    }
    out
}

/// Frame a subtable into a Lookup: type(2) + flag(2) + subCount(2) + offset(2).
fn build_single_lookup(lookup_type: u16, subtable: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, lookup_type);
    w16(&mut out, 0); // lookupFlag
    w16(&mut out, 1); // subTableCount
    w16(&mut out, 8); // subtableOffset (header is 8 bytes)
    out.extend_from_slice(subtable);
    out
}

/// Assemble a GSUB with three lookups and one feature referencing lookup 0.
fn build_gsub(lookups: &[Vec<u8>]) -> Vec<u8> {
    // LookupList.
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

    // FeatureList: one feature referencing lookup index 0.
    let mut feat = Vec::new();
    w16(&mut feat, 0); // featureParams
    w16(&mut feat, 1); // lookupIndexCount
    w16(&mut feat, 0); // lookup index 0
    let mut fl = Vec::new();
    w16(&mut fl, 1); // featureCount
    fl.extend_from_slice(b"calt");
    w16(&mut fl, 2 + 6); // feature offset
    fl.extend_from_slice(&feat);

    // ScriptList: one script whose DefaultLangSys references feature 0.
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

    let header_size: u16 = 10;
    let sl_off = header_size;
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

fn lookup_count(gsub: &[u8]) -> u16 {
    let ll_off = r16(gsub, 8) as usize;
    r16(gsub, ll_off)
}

fn lookup_offset(gsub: &[u8], idx: usize) -> usize {
    let ll_off = r16(gsub, 8) as usize;
    ll_off + r16(gsub, ll_off + 2 + idx * 2) as usize
}

#[test]
fn chained_context_f3_is_remapped_with_lookup_index_fixup() {
    // Three lookups:
    //   0: type 6 chained context (fmt 3), input coverage [6], record → lookup 2
    //   1: type 1 single subst over GID 9 (NOT in subset → dropped)
    //   2: type 1 single subst 6→8 (both in subset → survives)
    let lk0 = build_single_lookup(6, &build_chain_context_f3(&[6], &[(0, 2)]));
    let lk1 = build_single_lookup(1, &build_single_subst_f2(&[(9, 10)]));
    let lk2 = build_single_lookup(1, &build_single_subst_f2(&[(6, 8)]));
    let gsub = build_gsub(&[lk0, lk1, lk2]);

    // Keep GIDs 6→1 and 8→2. GID 9 is absent, so lookup 1 drops.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(6, 1);
    remap.insert(8, 2);

    let result = rewrite_gsub(&gsub, &remap);

    // Survivors: lookup 0 (context) and lookup 2 (single) → 2 lookups.
    assert_eq!(lookup_count(&result), 2, "context lookup must survive");

    // Lookup 0 is still the context lookup (type 6).
    let lk0_off = lookup_offset(&result, 0);
    assert_eq!(r16(&result, lk0_off), 6, "lookup 0 must be type 6");

    // Navigate to its format-3 subtable.
    let st_off = lk0_off + r16(&result, lk0_off + 6) as usize;
    assert_eq!(r16(&result, st_off), 3, "subtable must be format 3");
    assert_eq!(r16(&result, st_off + 2), 0, "backtrackCount = 0");
    assert_eq!(r16(&result, st_off + 4), 1, "inputCount = 1");
    let input_cov_off = st_off + r16(&result, st_off + 6) as usize;
    assert_eq!(r16(&result, st_off + 8), 0, "lookaheadCount = 0");
    assert_eq!(r16(&result, st_off + 10), 1, "seqLookupCount = 1");
    // seqLookupRecord: sequenceIndex(2) then lookupListIndex(2).
    assert_eq!(r16(&result, st_off + 12), 0, "sequenceIndex preserved");
    // Old lookup index 2 (single subst) maps to new index 1 after lookup 1 dropped.
    assert_eq!(
        r16(&result, st_off + 14),
        1,
        "lookupListIndex must be remapped 2 → 1"
    );

    // Input coverage must be remapped to the new GID (6 → 1).
    assert_eq!(r16(&result, input_cov_off), 1, "coverage format 1");
    assert_eq!(r16(&result, input_cov_off + 2), 1, "one covered glyph");
    assert_eq!(
        r16(&result, input_cov_off + 4),
        1,
        "coverage GID must be remapped 6 → 1"
    );
}

#[test]
fn context_f3_dropped_when_input_coverage_empties() {
    // A context lookup whose only input glyph leaves the subset must be dropped
    // (the rule can never match), not emitted with an empty coverage.
    let lk0 = build_single_lookup(6, &build_chain_context_f3(&[42], &[(0, 1)]));
    let lk1 = build_single_lookup(1, &build_single_subst_f2(&[(6, 8)]));
    let gsub = build_gsub(&[lk0, lk1]);

    // Keep only GIDs 6 and 8; GID 42 (the context input) is absent.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(6, 1);
    remap.insert(8, 2);

    let result = rewrite_gsub(&gsub, &remap);
    // Only the surviving single-subst lookup remains.
    assert_eq!(
        lookup_count(&result),
        1,
        "context lookup with emptied coverage must drop"
    );
    let lk0_off = lookup_offset(&result, 0);
    assert_eq!(
        r16(&result, lk0_off),
        1,
        "surviving lookup is the single subst"
    );
}

#[test]
fn context_f1_with_null_coverage_is_dropped() {
    // Formats 1 and 2 are now fully remapped (see tests/advanced_lookups.rs);
    // a *malformed* one — NULL coverage offset, no rule sets — must still be
    // dropped rather than emitted.
    let f1_subtable = {
        let mut v = Vec::new();
        w16(&mut v, 1); // format 1 (glyph rule sets)
        w16(&mut v, 0); // coverageOffset = NULL → malformed
        w16(&mut v, 0); // seqRuleSetCount
        v
    };
    let lk0 = build_single_lookup(6, &f1_subtable);
    let lk1 = build_single_lookup(1, &build_single_subst_f2(&[(6, 8)]));
    let gsub = build_gsub(&[lk0, lk1]);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(6, 1);
    remap.insert(8, 2);

    let result = rewrite_gsub(&gsub, &remap);
    assert_eq!(
        lookup_count(&result),
        1,
        "malformed format-1 context lookup must be dropped"
    );
}
