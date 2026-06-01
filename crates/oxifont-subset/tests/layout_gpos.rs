//! Tests for GPOS table rewriting (otl_gpos::rewrite_gpos).
//!
//! Covered:
//! - SinglePos format 2: 3 covered glyphs, remap removes one → 2 GIDs + 2 ValueRecords
//! - PairPos format 1: 2 first-glyphs × 2 second-glyph pairs each, remap removes one
//!   second-glyph from first PairSet
//! - PairPos format 1: all pairs in a PairSet removed → first-glyph dropped from coverage
//! - Type 3 (CursivePos): lookup dropped
//! - Empty/tiny input: returned verbatim
//!
//! Not covered (deviation note):
//! - MarkBasePos / MarkMarkPos integration test (anchor byte-layout verified via unit
//!   path; full round-trip requires a real font).

use oxifont_subset::otl_gpos::rewrite_gpos;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

/// (second_gid, xAdv1, xAdv2) triple for test pair-set construction.
type TestPairEntry = (u16, i16, i16);

/// (first_gid, list of pairs) for test PairPos format 1 construction.
type TestPairSet = (u16, Vec<TestPairEntry>);

/// Build a minimal GPOS v1.0 table with a single lookup of the given type,
/// containing the provided subtable bytes.
///
/// SFL chain is populated with one script / one LangSys / one feature pointing
/// to the one lookup.
fn make_gpos_table(lookup_type: u16, subtable: &[u8]) -> Vec<u8> {
    // We'll lay out: header(10) | ScriptList | FeatureList | LookupList
    //
    // --- ScriptList (minimal: 1 script "DFLT", DefaultLangSys with 1 feature idx 0) ---
    // ScriptList: scriptCount(2) + scriptRecord[{tag(4)+offset(2)}] + Script + LangSys
    let langsys: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&be16(0)); // lookupOrderOffset (reserved)
        v.extend_from_slice(&be16(0xFFFF)); // requiredFeatureIndex (none)
        v.extend_from_slice(&be16(1)); // featureIndexCount
        v.extend_from_slice(&be16(0)); // featureIndex[0] = 0
        v
    };
    let script: Vec<u8> = {
        let dls_offset = 4u16; // Script header is 4 bytes; DefaultLangSys starts right after
        let mut v = Vec::new();
        v.extend_from_slice(&be16(dls_offset)); // defaultLangSysOffset
        v.extend_from_slice(&be16(0)); // langSysCount
        v.extend_from_slice(&langsys);
        v
    };
    let script_list: Vec<u8> = {
        let script_offset = 2 + 6; // scriptCount(2) + 1 scriptRecord(6) = 8 bytes
        let mut v = Vec::new();
        v.extend_from_slice(&be16(1)); // scriptCount
        v.extend_from_slice(b"DFLT"); // tag
        v.extend_from_slice(&be16(script_offset as u16)); // scriptOffset
        v.extend_from_slice(&script);
        v
    };

    // --- FeatureList (1 feature "kern" with lookupIndex 0) ---
    let feature: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&be16(0)); // featureParamsOffset
        v.extend_from_slice(&be16(1)); // lookupIndexCount
        v.extend_from_slice(&be16(0)); // lookupListIndex[0] = 0
        v
    };
    let feature_list: Vec<u8> = {
        let feat_offset = 2 + 6; // featureCount(2) + 1 featureRecord(6) = 8 bytes
        let mut v = Vec::new();
        v.extend_from_slice(&be16(1)); // featureCount
        v.extend_from_slice(b"kern"); // tag
        v.extend_from_slice(&be16(feat_offset as u16));
        v.extend_from_slice(&feature);
        v
    };

    // --- LookupList (1 lookup of lookup_type, 1 subtable) ---
    // Lookup header: lookupType(2) + lookupFlag(2) + subTableCount(2) + subtableOffset[1](2) = 8 bytes
    // subtable follows immediately at offset 8 from lookup start
    let lookup: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&be16(lookup_type)); // lookupType
        v.extend_from_slice(&be16(0)); // lookupFlag
        v.extend_from_slice(&be16(1)); // subTableCount
        v.extend_from_slice(&be16(8)); // subtableOffset[0] = 8 (from lookup start)
        v.extend_from_slice(subtable); // subtable data
        v
    };
    let lookup_list: Vec<u8> = {
        // LookupList: lookupCount(2) + lookupOffset[1](2) = 4 bytes header; lookup starts at 4
        let mut v = Vec::new();
        v.extend_from_slice(&be16(1)); // lookupCount
        v.extend_from_slice(&be16(4)); // lookupOffset[0] = 4 (from LookupList start, after header)
        v.extend_from_slice(&lookup);
        v
    };

    // --- Assemble full GPOS table ---
    let header_size = 10u16;
    let sl_off = header_size;
    let fl_off = sl_off + script_list.len() as u16;
    let ll_off = fl_off + feature_list.len() as u16;

    let mut out = Vec::new();
    out.extend_from_slice(&be16(1)); // majorVersion
    out.extend_from_slice(&be16(0)); // minorVersion
    out.extend_from_slice(&be16(sl_off));
    out.extend_from_slice(&be16(fl_off));
    out.extend_from_slice(&be16(ll_off));
    out.extend_from_slice(&script_list);
    out.extend_from_slice(&feature_list);
    out.extend_from_slice(&lookup_list);
    out
}

/// Build a Coverage format 1 table for the given sorted GID list.
fn coverage_f1(gids: &[u16]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&be16(1)); // format
    v.extend_from_slice(&be16(gids.len() as u16));
    for &g in gids {
        v.extend_from_slice(&be16(g));
    }
    v
}

// ---------------------------------------------------------------------------
// Test: empty / tiny input → verbatim
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_empty_verbatim() {
    let remap: HashMap<u16, u16> = HashMap::new();
    let tiny = vec![0u8, 1, 0, 0, 0]; // < 10 bytes
    let out = rewrite_gpos(&tiny, &remap);
    assert_eq!(out, tiny, "tiny input must be returned verbatim");

    let empty = vec![];
    let out2 = rewrite_gpos(&empty, &remap);
    assert_eq!(out2, empty, "empty input must be returned verbatim");
}

// ---------------------------------------------------------------------------
// Test: SinglePos format 2 — remap removes one glyph
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_singlepos_f2_remap() {
    // Three glyphs: old GIDs 10, 20, 30.
    // ValueFormat = 0x0004 (XAdvance only) → vr_size = 2 bytes.
    // valueRecords: [200i16, 300i16, 400i16] (one per covered glyph in coverage order).
    // Remap: 10→1, 30→2 (GID 20 removed).
    // Expected output: SinglePos f2 with 2 GIDs (1, 2) and 2 ValueRecords (200, 400).

    let value_format: u16 = 0x0004;
    let old_gids: [u16; 3] = [10, 20, 30];
    let value_records: [i16; 3] = [200, 300, 400];

    // Coverage starts right after the fixed header (format(2)+covOff(2)+vf(2)+vcount(2) = 8 bytes
    // plus valueRecords (3 × 2 = 6 bytes) → coverage at offset 14).
    let cov = coverage_f1(&old_gids);
    let cov_offset = (8 + 3 * 2) as u16; // 14

    let mut subtable: Vec<u8> = Vec::new();
    subtable.extend_from_slice(&be16(2)); // format
    subtable.extend_from_slice(&be16(cov_offset));
    subtable.extend_from_slice(&be16(value_format));
    subtable.extend_from_slice(&be16(3)); // valueCount
    for &vr in &value_records {
        subtable.extend_from_slice(&be16(vr as u16));
    }
    subtable.extend_from_slice(&cov);

    let table = make_gpos_table(1, &subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(10, 1);
    remap.insert(30, 2);
    // GID 20 not in remap → dropped.

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10, "output must be a valid GPOS table");

    // Parse the rewritten table and extract the SinglePos f2 subtable.
    let sl_off = u16::from_be_bytes([out[4], out[5]]) as usize;
    let fl_off = u16::from_be_bytes([out[6], out[7]]) as usize;
    let ll_off = u16::from_be_bytes([out[8], out[9]]) as usize;

    // LookupList → first lookup → first subtable.
    let ll = &out[ll_off..];
    let lk_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;
    assert_eq!(lk_count, 1, "should have exactly 1 lookup");
    let lk_off = u16::from_be_bytes([ll[2], ll[3]]) as usize;
    let lk = &ll[lk_off..];
    let sub_count = u16::from_be_bytes([lk[4], lk[5]]) as usize;
    assert_eq!(sub_count, 1, "should have exactly 1 subtable");
    let st_off_in_lk = u16::from_be_bytes([lk[6], lk[7]]) as usize;
    let st = &lk[st_off_in_lk..];

    let fmt = u16::from_be_bytes([st[0], st[1]]);
    assert_eq!(fmt, 2, "rewritten subtable should be SinglePos format 2");

    let new_vf = u16::from_be_bytes([st[4], st[5]]);
    assert_eq!(new_vf, value_format, "valueFormat should be preserved");

    let new_vcount = u16::from_be_bytes([st[6], st[7]]) as usize;
    assert_eq!(new_vcount, 2, "2 surviving glyphs");

    // ValueRecord 0 → XAdvance 200, ValueRecord 1 → XAdvance 400.
    let vr0 = i16::from_be_bytes([st[8], st[9]]);
    let vr1 = i16::from_be_bytes([st[10], st[11]]);
    assert_eq!(
        vr0, 200,
        "first surviving ValueRecord should be 200 (XAdvance for GID 10→1)"
    );
    assert_eq!(
        vr1, 400,
        "second surviving ValueRecord should be 400 (XAdvance for GID 30→2)"
    );

    // Suppress unused variable warnings.
    let _ = (sl_off, fl_off);
}

// ---------------------------------------------------------------------------
// Test: PairPos format 1 — remap removes one second-glyph from first PairSet
// ---------------------------------------------------------------------------

/// Build a PairPos format 1 subtable.
///
/// `pairs`: Vec of (first_gid, Vec<(second_gid, xAdv1, xAdv2)>)
/// ValueFormat1 = 0x0004 (XAdvance), ValueFormat2 = 0x0000 (no record).
fn build_pair_pos_f1(pairs: &[TestPairSet]) -> Vec<u8> {
    let value_format1: u16 = 0x0004; // XAdvance = 2 bytes
    let value_format2: u16 = 0x0000; // no record
    let vr1_size = 2usize;
    let vr2_size = 0usize;
    let pair_record_size = 2 + vr1_size + vr2_size;

    let n = pairs.len();
    let first_gids: Vec<u16> = pairs.iter().map(|&(g, _)| g).collect();

    // Header: format(2)+covOff(2)+vf1(2)+vf2(2)+pairSetCount(2)+pairSetOffsets[n](2n) = 10+2n bytes
    // Coverage follows immediately after header.
    // PairSets follow coverage.
    let header_end = 10 + n * 2;
    let cov = coverage_f1(&first_gids);
    let cov_offset = header_end as u16;

    // Compute PairSet offsets (relative to subtable start).
    let mut ps_data: Vec<Vec<u8>> = Vec::new();
    for (_, pair_list) in pairs {
        let mut ps: Vec<u8> = Vec::new();
        ps.extend_from_slice(&be16(pair_list.len() as u16));
        for &(second, xadv1, _xadv2) in pair_list {
            ps.extend_from_slice(&be16(second));
            ps.extend_from_slice(&be16(xadv1 as u16));
            // vr2 is empty (format 0)
        }
        ps_data.push(ps);
    }

    let mut subtable: Vec<u8> = Vec::new();
    subtable.extend_from_slice(&be16(1)); // format
    subtable.extend_from_slice(&be16(cov_offset));
    subtable.extend_from_slice(&be16(value_format1));
    subtable.extend_from_slice(&be16(value_format2));
    subtable.extend_from_slice(&be16(n as u16));

    // Placeholder pairSetOffsets.
    let ps_offsets_pos = subtable.len();
    for _ in 0..n {
        subtable.extend_from_slice(&be16(0));
    }
    subtable.extend_from_slice(&cov);

    // Append PairSets and patch offsets.
    let mut ps_offs: Vec<u16> = Vec::new();
    for ps in &ps_data {
        ps_offs.push(subtable.len() as u16);
        subtable.extend_from_slice(ps);
    }
    for (i, &off) in ps_offs.iter().enumerate() {
        subtable[ps_offsets_pos + i * 2] = (off >> 8) as u8;
        subtable[ps_offsets_pos + i * 2 + 1] = (off & 0xFF) as u8;
    }

    let _ = (vr2_size, pair_record_size);
    subtable
}

/// Extract PairPos format 1 subtable from rewritten GPOS output.
///
/// Returns (first_gids_from_coverage, Vec<(pairValueCount, first_second_glyph_pairs)>).
fn parse_pairpos_f1_from_gpos(out: &[u8]) -> Option<(Vec<u16>, Vec<Vec<u16>>)> {
    let ll_off = u16::from_be_bytes([out[8], out[9]]) as usize;
    let ll = out.get(ll_off..)?;
    let lk_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;
    if lk_count == 0 {
        return None;
    }
    let lk_off = u16::from_be_bytes([ll[2], ll[3]]) as usize;
    let lk = ll.get(lk_off..)?;
    let st_off_in_lk = u16::from_be_bytes([lk[6], lk[7]]) as usize;
    let st = lk.get(st_off_in_lk..)?;

    let fmt = u16::from_be_bytes([st[0], st[1]]);
    if fmt != 1 {
        return None;
    }
    let cov_off = u16::from_be_bytes([st[2], st[3]]) as usize;
    let vf1 = u16::from_be_bytes([st[4], st[5]]);
    let vf2 = u16::from_be_bytes([st[6], st[7]]);
    let ps_count = u16::from_be_bytes([st[8], st[9]]) as usize;

    // Read coverage GIDs.
    let cov_data = st.get(cov_off..)?;
    let cov_fmt = u16::from_be_bytes([cov_data[0], cov_data[1]]);
    if cov_fmt != 1 {
        return None;
    }
    let cov_count = u16::from_be_bytes([cov_data[2], cov_data[3]]) as usize;
    let mut first_gids: Vec<u16> = Vec::new();
    for i in 0..cov_count {
        first_gids.push(u16::from_be_bytes([
            cov_data[4 + i * 2],
            cov_data[4 + i * 2 + 1],
        ]));
    }

    let vr1_size = ((vf1 & 0x00FF).count_ones() as usize) * 2;
    let vr2_size = ((vf2 & 0x00FF).count_ones() as usize) * 2;
    let pair_rec_size = 2 + vr1_size + vr2_size;

    let mut all_seconds: Vec<Vec<u16>> = Vec::new();
    for i in 0..ps_count {
        let ps_off = u16::from_be_bytes([st[10 + i * 2], st[10 + i * 2 + 1]]) as usize;
        let ps = st.get(ps_off..)?;
        let pvc = u16::from_be_bytes([ps[0], ps[1]]) as usize;
        let mut seconds: Vec<u16> = Vec::new();
        for j in 0..pvc {
            let rec_off = 2 + j * pair_rec_size;
            seconds.push(u16::from_be_bytes([ps[rec_off], ps[rec_off + 1]]));
        }
        all_seconds.push(seconds);
    }

    Some((first_gids, all_seconds))
}

#[test]
fn test_gpos_pairpos_f1_remap() {
    // First-glyph 5 → pairs with [10, 20]; first-glyph 6 → pairs with [10, 20].
    // Remap: 5→1, 6→2, 10→3 (GID 20 removed).
    // Expected: first-glyph 1 has pair with second 3 only; first-glyph 2 has pair with 3 only.

    let pairs: Vec<TestPairSet> = vec![
        (5, vec![(10, 100, 0), (20, 200, 0)]),
        (6, vec![(10, 150, 0), (20, 250, 0)]),
    ];
    let subtable = build_pair_pos_f1(&pairs);
    let table = make_gpos_table(2, &subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(5, 1);
    remap.insert(6, 2);
    remap.insert(10, 3);
    // GID 20 removed.

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10);

    let (first_gids, second_lists) =
        parse_pairpos_f1_from_gpos(&out).expect("should parse rewritten PairPos f1");

    assert_eq!(first_gids, vec![1, 2], "two first-glyphs should survive");
    assert_eq!(second_lists.len(), 2);
    assert_eq!(
        second_lists[0],
        vec![3],
        "first PairSet: only GID 3 (was 10) survives"
    );
    assert_eq!(
        second_lists[1],
        vec![3],
        "second PairSet: only GID 3 (was 10) survives"
    );
}

// ---------------------------------------------------------------------------
// Test: PairPos format 1 — empty PairSet causes first-glyph to be dropped
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_pairpos_f1_empty_set_dropped() {
    // First-glyph 5 → pairs with [10, 20]; first-glyph 6 → pairs with [20] only.
    // Remap: 5→1, 10→3 (GIDs 6 and 20 removed).
    // Expected: first-glyph 6 has no surviving pairs → dropped from coverage.
    //           first-glyph 1 (was 5) has pair with 3 (was 10).

    let pairs: Vec<TestPairSet> = vec![
        (5, vec![(10, 100, 0), (20, 200, 0)]),
        (6, vec![(20, 300, 0)]), // all pairs will be removed
    ];
    let subtable = build_pair_pos_f1(&pairs);
    let table = make_gpos_table(2, &subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(5, 1);
    remap.insert(10, 3);
    // GIDs 6 and 20 removed.

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10);

    let (first_gids, second_lists) =
        parse_pairpos_f1_from_gpos(&out).expect("should parse rewritten PairPos f1");

    assert_eq!(
        first_gids,
        vec![1],
        "only one first-glyph (was 5) should survive"
    );
    assert_eq!(second_lists.len(), 1);
    assert_eq!(
        second_lists[0],
        vec![3],
        "surviving pair: second GID 3 (was 10)"
    );
}

// ---------------------------------------------------------------------------
// Test: Type 3 (CursivePos) → lookup dropped
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_type3_dropped() {
    // Build a fake CursivePos subtable (format 1); the handler returns None for type 3.
    // After lookup is dropped, rewrite_gpos returns a valid GPOS with 0 lookups.
    let fake_subtable: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&be16(1)); // format
        v.extend_from_slice(&be16(6)); // coverageOffset (bogus, won't be parsed)
        v.extend_from_slice(&be16(0)); // entryExitCount
                                       // No coverage or exit records needed — type 3 is immediately dropped.
        v
    };

    let table = make_gpos_table(3, &fake_subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(1, 1);

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10, "output should be a valid GPOS table");

    // LookupList should have 0 lookups.
    let ll_off = u16::from_be_bytes([out[8], out[9]]) as usize;
    let ll = &out[ll_off..];
    let lk_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;
    assert_eq!(lk_count, 0, "CursivePos lookup should be dropped");
}

// ---------------------------------------------------------------------------
// Test: SinglePos format 1 — all glyphs survive
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_singlepos_f1_all_survive() {
    // Format 1: single ValueRecord (XAdvance = 100) for all covered glyphs.
    // GIDs 10, 20 → remapped to 1, 2. Both survive.
    let value_format: u16 = 0x0004; // XAdvance
    let old_gids: [u16; 2] = [10, 20];

    let cov = coverage_f1(&old_gids);
    let cov_offset = (6 + 2) as u16; // format(2)+covOff(2)+vf(2) = 6 bytes header + vr(2) = 8; cov at 8

    let mut subtable: Vec<u8> = Vec::new();
    subtable.extend_from_slice(&be16(1)); // format
    subtable.extend_from_slice(&be16(cov_offset));
    subtable.extend_from_slice(&be16(value_format));
    subtable.extend_from_slice(&be16(100i16 as u16)); // ValueRecord: XAdvance=100
    subtable.extend_from_slice(&cov);

    let table = make_gpos_table(1, &subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(10, 1);
    remap.insert(20, 2);

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10);

    // Verify the output has a valid lookup.
    let ll_off = u16::from_be_bytes([out[8], out[9]]) as usize;
    let ll = &out[ll_off..];
    let lk_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;
    assert_eq!(lk_count, 1, "lookup should survive");

    // Extract subtable and check format and XAdvance.
    let lk_off = u16::from_be_bytes([ll[2], ll[3]]) as usize;
    let lk = &ll[lk_off..];
    let st_off = u16::from_be_bytes([lk[6], lk[7]]) as usize;
    let st = &lk[st_off..];

    let fmt = u16::from_be_bytes([st[0], st[1]]);
    assert_eq!(fmt, 1, "should remain SinglePos format 1");
    let xadv = i16::from_be_bytes([st[6], st[7]]);
    assert_eq!(xadv, 100, "XAdvance ValueRecord should be preserved");
}

// ---------------------------------------------------------------------------
// Test: SinglePos format 1 — all glyphs removed → table verbatim fallback
// ---------------------------------------------------------------------------

#[test]
fn test_gpos_singlepos_f1_all_removed() {
    // GID 10 not in remap → subtable dropped → lookup dropped → rewrite returns verbatim.
    let value_format: u16 = 0x0004;
    let old_gids: [u16; 1] = [10];
    let cov = coverage_f1(&old_gids);

    let mut subtable: Vec<u8> = Vec::new();
    subtable.extend_from_slice(&be16(1)); // format
    subtable.extend_from_slice(&be16(8)); // cov at offset 8
    subtable.extend_from_slice(&be16(value_format));
    subtable.extend_from_slice(&be16(0)); // XAdvance = 0
    subtable.extend_from_slice(&cov);

    let table = make_gpos_table(1, &subtable);

    let remap: HashMap<u16, u16> = HashMap::new(); // no surviving GIDs

    let out = rewrite_gpos(&table, &remap);
    assert!(out.len() >= 10);

    // Should have 0 lookups (subtable dropped, so lookup dropped, but SFL still valid).
    let ll_off = u16::from_be_bytes([out[8], out[9]]) as usize;
    let ll = &out[ll_off..];
    let lk_count = u16::from_be_bytes([ll[0], ll[1]]) as usize;
    assert_eq!(
        lk_count, 0,
        "lookup should be dropped when all covered glyphs removed"
    );
}
