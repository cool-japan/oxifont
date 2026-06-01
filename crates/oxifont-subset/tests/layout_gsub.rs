//! Integration tests for `otl::rewrite_gsub`.
//!
//! Each test builds a minimal but structurally valid GSUB binary, calls
//! `rewrite_gsub`, and checks the output with the same binary-level helpers
//! used to construct the input.

use oxifont_subset::otl::rewrite_gsub;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Binary helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Subtable builders
// ---------------------------------------------------------------------------

/// SingleSubst Format 2 subtable.
///
/// Layout (per OpenType spec):
/// offset 0: format(2)
/// offset 2: coverageOffset(2)  — points to Coverage table
/// offset 4: glyphCount(2)
/// offset 6: substituteGlyphIDs[glyphCount](2 each)
/// offset 6 + n*2: Coverage table (format 1)
fn build_single_subst_f2(pairs: &[(u16, u16)]) -> Vec<u8> {
    let n = pairs.len() as u16;
    // coverageOffset: after format(2) + coverageOffset(2) + glyphCount(2) + substituteGlyphIDs(n*2)
    let cov_off = 6 + n * 2;
    let mut out = Vec::new();
    w16(&mut out, 2); // format
    w16(&mut out, cov_off);
    w16(&mut out, n);
    // Substitute GIDs (aligned with coverage indices).
    for &(_, subst) in pairs {
        w16(&mut out, subst);
    }
    // Coverage (format 1, explicit list of input GIDs).
    w16(&mut out, 1); // cov format
    w16(&mut out, n);
    for &(gid, _) in pairs {
        w16(&mut out, gid);
    }
    out
}

/// LigatureSubst Format 1 subtable.
///
/// `coverage_gids[i]` is the first glyph of ligature set `i`.
/// `ligature_sets[i]` is a list of `(lig_glyph, additional_components)`.
fn build_ligature_subst(coverage_gids: &[u16], ligature_sets: &[Vec<(u16, Vec<u16>)>]) -> Vec<u8> {
    let n = coverage_gids.len();
    assert_eq!(n, ligature_sets.len());

    // We'll build pieces and assemble bottom-up.
    // Subtable header: format(2) + coverageOffset(2) + ligSetCount(2) + offsets[n](2*n)
    let header_size = 6 + n * 2;

    // Coverage bytes (format 1).
    let mut cov = Vec::new();
    w16(&mut cov, 1); // format
    w16(&mut cov, n as u16);
    for &g in coverage_gids {
        w16(&mut cov, g);
    }
    let cov_off_in_subtable = header_size as u16;

    // Build LigatureSet blobs.
    let mut ls_blobs: Vec<Vec<u8>> = Vec::new();
    for ligs in ligature_sets {
        let lc = ligs.len();
        // LigatureSet: ligCount(2) + offsets[lc](2*lc) + Ligature data
        let ls_header_size = 2 + lc * 2;
        let mut ls = Vec::new();
        w16(&mut ls, lc as u16);
        // Placeholder lig offsets (relative to LigatureSet start).
        let lig_off_pos = ls.len();
        for _ in 0..lc {
            w16(&mut ls, 0);
        }
        // Lig data.
        let mut lig_offs: Vec<u16> = Vec::new();
        for (lig_glyph, comps) in ligs {
            lig_offs.push(ls.len() as u16);
            w16(&mut ls, *lig_glyph);
            w16(&mut ls, (comps.len() + 1) as u16); // componentCount
            for &c in comps {
                w16(&mut ls, c);
            }
        }
        // Patch.
        for (k, &off) in lig_offs.iter().enumerate() {
            patch16(&mut ls, lig_off_pos + k * 2, off);
        }
        ls_blobs.push(ls);
        let _ = ls_header_size;
    }

    // Now assemble subtable.
    let mut out = Vec::new();
    w16(&mut out, 1); // format
    w16(&mut out, cov_off_in_subtable);
    w16(&mut out, n as u16);

    let ls_offsets_pos = out.len();
    for _ in 0..n {
        w16(&mut out, 0);
    }
    out.extend_from_slice(&cov);

    let mut ls_offs: Vec<u16> = Vec::new();
    for blob in &ls_blobs {
        ls_offs.push(out.len() as u16);
        out.extend_from_slice(blob);
    }
    for (i, &off) in ls_offs.iter().enumerate() {
        patch16(&mut out, ls_offsets_pos + i * 2, off);
    }

    out
}

/// Build a single Lookup wrapping one subtable.
fn build_single_lookup(lookup_type: u16, subtable: &[u8]) -> Vec<u8> {
    // Lookup: type(2) + flag(2) + subCount(2) + subOffsets[1](2) + subtable
    let st_off: u16 = 8; // 3×u16 + 1×u16 = 8 bytes
    let mut out = Vec::new();
    w16(&mut out, lookup_type);
    w16(&mut out, 0); // lookupFlag
    w16(&mut out, 1); // subTableCount
    w16(&mut out, st_off);
    out.extend_from_slice(subtable);
    out
}

/// Build a LookupList containing one lookup.
fn build_lookup_list(lookup: &[u8]) -> Vec<u8> {
    // LookupList: lookupCount(2) + lookupOffsets[1](2) + lookup
    let lk_off: u16 = 4;
    let mut out = Vec::new();
    w16(&mut out, 1);
    w16(&mut out, lk_off);
    out.extend_from_slice(lookup);
    out
}

/// Build a Feature table for one lookup index.
fn build_feature(lookup_idx: u16) -> Vec<u8> {
    let mut out = Vec::new();
    w16(&mut out, 0); // featureParamsOffset
    w16(&mut out, 1); // lookupIndexCount
    w16(&mut out, lookup_idx);
    out
}

/// Build a minimal GSUB with one script, one feature, one lookup.
///
/// Returns the full GSUB table bytes.
fn build_gsub_one_lookup(
    script_tag: &[u8; 4],
    feature_tag: &[u8; 4],
    lookup_type: u16,
    subtable: &[u8],
) -> Vec<u8> {
    // ---- Build LookupList ----
    let lookup = build_single_lookup(lookup_type, subtable);
    let ll = build_lookup_list(&lookup);

    // ---- Build FeatureList ----
    // featureCount(2) + featureRecord[1](6) + Feature
    let feat = build_feature(0); // lookup index 0
    let feat_off_in_fl: u16 = 2 + 6; // after header + 1 record
    let mut fl = Vec::new();
    w16(&mut fl, 1); // featureCount
    fl.extend_from_slice(feature_tag);
    w16(&mut fl, feat_off_in_fl);
    fl.extend_from_slice(&feat);

    // ---- Build ScriptList ----
    // scriptCount(2) + scriptRecord[1](6) + Script + DefaultLangSys
    // Script: defaultLangSysOffset(2) + langSysCount(2)
    // DefaultLangSys: lookupOrderOffset(2) + requiredFeatureIndex(2) + featureIndexCount(2) + featureIndices[1](2)
    let mut dls = Vec::new();
    w16(&mut dls, 0); // lookupOrderOffset
    w16(&mut dls, 0xFFFF); // requiredFeatureIndex
    w16(&mut dls, 1); // featureIndexCount
    w16(&mut dls, 0); // feature index 0

    // Script: header(4) + DefaultLangSys
    let dls_off_in_script: u16 = 4; // after defaultLangSysOffset(2) + langSysCount(2)
    let mut sc = Vec::new();
    w16(&mut sc, dls_off_in_script);
    w16(&mut sc, 0); // langSysCount
    sc.extend_from_slice(&dls);

    // ScriptList: scriptCount(2) + scriptRecord[1](6) + Script
    let sc_off_in_sl: u16 = 2 + 6; // after header + 1 record (4-byte tag + 2-byte offset)
    let mut sl = Vec::new();
    w16(&mut sl, 1); // scriptCount
    sl.extend_from_slice(script_tag);
    w16(&mut sl, sc_off_in_sl);
    sl.extend_from_slice(&sc);

    // ---- Assemble GSUB header ----
    // Header: major(2) + minor(2) + scriptListOffset(2) + featureListOffset(2) + lookupListOffset(2)
    let header_size: u16 = 10;
    let sl_off = header_size;
    let fl_off = sl_off + sl.len() as u16;
    let ll_off = fl_off + fl.len() as u16;

    let mut out = Vec::new();
    w16(&mut out, 1); // majorVersion
    w16(&mut out, 0); // minorVersion
    w16(&mut out, sl_off);
    w16(&mut out, fl_off);
    w16(&mut out, ll_off);
    out.extend_from_slice(&sl);
    out.extend_from_slice(&fl);
    out.extend_from_slice(&ll);
    out
}

// ---------------------------------------------------------------------------
// Read helpers for decoded GSUB
// ---------------------------------------------------------------------------

/// Read the (scriptListOffset, featureListOffset, lookupListOffset) from a GSUB header.
fn gsub_offsets(data: &[u8]) -> (usize, usize, usize) {
    (
        r16(data, 4) as usize,
        r16(data, 6) as usize,
        r16(data, 8) as usize,
    )
}

fn lookup_count(gsub: &[u8]) -> u16 {
    let (_, _, ll_off) = gsub_offsets(gsub);
    r16(gsub, ll_off)
}

fn feature_count(gsub: &[u8]) -> u16 {
    let (_, fl_off, _) = gsub_offsets(gsub);
    r16(gsub, fl_off)
}

fn script_count(gsub: &[u8]) -> u16 {
    let (sl_off, _, _) = gsub_offsets(gsub);
    r16(gsub, sl_off)
}

/// Read the lookup type of lookup index `i`.
fn lookup_type_at(gsub: &[u8], i: usize) -> u16 {
    let (_, _, ll_off) = gsub_offsets(gsub);
    let lk_off = r16(gsub, ll_off + 2 + i * 2) as usize;
    r16(gsub, ll_off + lk_off)
}

/// Parse SingleSubst Format 2 from lookup 0 / subtable 0, returning (gid, subst) pairs.
///
/// Per spec layout (and how `emit_single_subst_f2` builds it):
/// offset 0: format(2)
/// offset 2: coverageOffset(2) — points to Coverage table
/// offset 4: glyphCount(2)
/// offset 6: substituteGlyphIDs[n]
/// offset 6 + n*2: Coverage table
fn read_single_subst_f2_pairs(gsub: &[u8]) -> Vec<(u16, u16)> {
    let (_, _, ll_off) = gsub_offsets(gsub);
    let lk_off = r16(gsub, ll_off + 2) as usize;
    let lk_abs = ll_off + lk_off;
    let st_off = r16(gsub, lk_abs + 6) as usize;
    let st_abs = lk_abs + st_off;

    let _format = r16(gsub, st_abs); // should be 2
    let cov_off = r16(gsub, st_abs + 2) as usize;
    let glyph_count = r16(gsub, st_abs + 4) as usize;

    // SubstituteGlyphIDs start at offset 6 in the subtable.
    let subst_base = st_abs + 6;

    // Coverage (Format 1) at st_abs + cov_off.
    let cov_abs = st_abs + cov_off;
    let _cov_fmt = r16(gsub, cov_abs);
    let cov_n = r16(gsub, cov_abs + 2) as usize;
    let mut gids = Vec::with_capacity(cov_n);
    for j in 0..cov_n {
        gids.push(r16(gsub, cov_abs + 4 + j * 2));
    }

    // Pair coverage GID with corresponding substitute GID.
    let mut pairs = Vec::with_capacity(glyph_count);
    for j in 0..glyph_count {
        let gid = gids.get(j).copied().unwrap_or(0);
        let subst = r16(gsub, subst_base + j * 2);
        pairs.push((gid, subst));
    }
    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// SingleSubst F2 with 3 pairs; remap removes one pair.
#[test]
fn test_gsub_singlesubst_f2_remap() {
    // GIDs 10→20, 11→21, 12→22 in the original.
    // gid_remap: 10→0, 11→1, (12 dropped), 20→2, 21→3, (22 dropped)
    let pairs = [(10u16, 20u16), (11, 21), (12, 22)];
    let subtable = build_single_subst_f2(&pairs);
    let gsub = build_gsub_one_lookup(b"DFLT", b"test", 1, &subtable);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(10, 0);
    remap.insert(11, 1);
    remap.insert(20, 2);
    remap.insert(21, 3);
    // 12 and 22 are NOT in remap → pair (12,22) is dropped.

    let result = rewrite_gsub(&gsub, &remap);
    assert!(result.len() >= 10, "output must be at least a GSUB header");

    // Should have 1 lookup, 1 feature, 1 script.
    assert_eq!(lookup_count(&result), 1);
    assert_eq!(feature_count(&result), 1);
    assert_eq!(script_count(&result), 1);

    // The surviving pairs should be (0→2) and (1→3).
    let out_pairs = read_single_subst_f2_pairs(&result);
    assert_eq!(out_pairs.len(), 2);
    // Pairs sorted by new GID: (0,2) then (1,3).
    assert_eq!(out_pairs[0], (0, 2));
    assert_eq!(out_pairs[1], (1, 3));
}

/// LigatureSubst: "fi" and "ffi" ligatures; removing "f" drops both.
#[test]
fn test_gsub_ligaturesubst_remap() {
    // GIDs: f=10, i=11, ffi-lig=20, fi-lig=21
    // LigatureSet for f(10): [ffi(20) with components [f(10),i(11)], fi(21) with components [i(11)]]
    // (f is the first component from coverage; additional components listed separately)
    let ligature_sets = vec![
        // LigatureSet for coverage GID 10 (f):
        vec![
            (20u16, vec![10u16, 11u16]), // ffi: lig_glyph=20, extra comps = [f, i]
            (21u16, vec![11u16]),        // fi:  lig_glyph=21, extra comps = [i]
        ],
    ];
    let subtable = build_ligature_subst(&[10], &ligature_sets);
    let gsub = build_gsub_one_lookup(b"DFLT", b"liga", 4, &subtable);

    // Drop GID 10 (f) — both ligatures reference it as coverage or component.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(11, 0); // i → 0
    remap.insert(20, 1); // ffi-lig → 1
    remap.insert(21, 2); // fi-lig → 2
                         // 10 (f) NOT in remap → coverage entry dropped → both ligatures dropped.

    let result = rewrite_gsub(&gsub, &remap);
    // All lookups should be dropped (lookup dropped when all subtables drop).
    assert_eq!(lookup_count(&result), 0);
    // Feature references lookup 0 which was dropped → feature dropped.
    assert_eq!(feature_count(&result), 0);
}

/// SFL chain: after a lookup is dropped, the feature/script chain is rebuilt correctly.
#[test]
fn test_gsub_sfl_chain_rebuilt() {
    // Two-lookup GSUB: lookup 0 (type 1, will survive), lookup 1 (type 5, always dropped).
    // We build this manually since build_gsub_one_lookup only handles one lookup.

    // Subtable for lookup 0: SingleSubst F2, pair (5→6).
    let st0 = build_single_subst_f2(&[(5u16, 6u16)]);
    // "Subtable" for lookup 1: type 5 (ContextSubst) — just 2 bytes (format=1), will be dropped.
    let st1 = vec![0u8, 1u8]; // minimal non-empty bytes, format=1

    let lk0 = build_single_lookup(1, &st0);
    let lk1 = build_single_lookup(5, &st1);

    // LookupList with two lookups.
    let mut ll = Vec::new();
    w16(&mut ll, 2); // lookupCount
    let lk0_off: u16 = 2 + 2 * 2; // after header + 2 offsets = 6
    let lk1_off = lk0_off + lk0.len() as u16;
    w16(&mut ll, lk0_off);
    w16(&mut ll, lk1_off);
    ll.extend_from_slice(&lk0);
    ll.extend_from_slice(&lk1);

    // FeatureList: two features, feat0 references lookup 0, feat1 references lookup 1.
    let feat0 = build_feature(0);
    let feat1 = build_feature(1);
    let feat0_off_in_fl: u16 = 2 + 2 * 6; // 2 + 2×featureRecord(6)
    let feat1_off_in_fl = feat0_off_in_fl + feat0.len() as u16;
    let mut fl = Vec::new();
    w16(&mut fl, 2); // featureCount
    fl.extend_from_slice(b"feat");
    w16(&mut fl, feat0_off_in_fl);
    fl.extend_from_slice(b"feat");
    w16(&mut fl, feat1_off_in_fl);
    fl.extend_from_slice(&feat0);
    fl.extend_from_slice(&feat1);

    // ScriptList: one script, DefaultLangSys references both features.
    let mut dls = Vec::new();
    w16(&mut dls, 0); // lookupOrderOffset
    w16(&mut dls, 0xFFFF); // requiredFeatureIndex
    w16(&mut dls, 2); // featureIndexCount
    w16(&mut dls, 0); // feature 0
    w16(&mut dls, 1); // feature 1

    let dls_off_in_script: u16 = 4;
    let mut sc = Vec::new();
    w16(&mut sc, dls_off_in_script);
    w16(&mut sc, 0);
    sc.extend_from_slice(&dls);

    let sc_off_in_sl: u16 = 2 + 6;
    let mut sl = Vec::new();
    w16(&mut sl, 1);
    sl.extend_from_slice(b"DFLT");
    w16(&mut sl, sc_off_in_sl);
    sl.extend_from_slice(&sc);

    // Assemble.
    let header_size: u16 = 10;
    let sl_off = header_size;
    let fl_off = sl_off + sl.len() as u16;
    let ll_off = fl_off + fl.len() as u16;
    let mut gsub = Vec::new();
    w16(&mut gsub, 1);
    w16(&mut gsub, 0);
    w16(&mut gsub, sl_off);
    w16(&mut gsub, fl_off);
    w16(&mut gsub, ll_off);
    gsub.extend_from_slice(&sl);
    gsub.extend_from_slice(&fl);
    gsub.extend_from_slice(&ll);

    // Remap: keep GIDs 5 and 6.
    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(5, 0);
    remap.insert(6, 1);

    let result = rewrite_gsub(&gsub, &remap);

    // Lookup 0 (type 1) survives; lookup 1 (type 5) is dropped.
    assert_eq!(lookup_count(&result), 1);
    // feat0 references lookup 0 → survives as new lookup 0.
    // feat1 references lookup 1 → all dropped → feat1 is dropped.
    assert_eq!(feature_count(&result), 1);
    // Script has DefaultLangSys with at least one feature → script survives.
    assert_eq!(script_count(&result), 1);

    // The surviving lookup should be type 1.
    assert_eq!(lookup_type_at(&result, 0), 1);
}

/// Lookup type 5 is always dropped; but if another lookup survives, feature stays.
#[test]
fn test_gsub_type5_dropped() {
    // Build a GSUB where the only lookup is type 5.
    let st = vec![0u8, 1u8]; // type 5, minimal
    let gsub = build_gsub_one_lookup(b"DFLT", b"calt", 5, &st);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0); // even with valid GIDs, type 5 is dropped

    let result = rewrite_gsub(&gsub, &remap);
    // Lookup dropped, so feature is dropped, so script has no features → also dropped.
    assert_eq!(lookup_count(&result), 0);
    assert_eq!(feature_count(&result), 0);
}

/// Input < 10 bytes → returned verbatim.
#[test]
fn test_gsub_empty_input_verbatim() {
    let tiny = vec![0u8; 6];
    let remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gsub(&tiny, &remap);
    assert_eq!(result, tiny);
}

/// All lookups removed → valid GSUB with empty LookupList.
#[test]
fn test_gsub_all_lookups_dropped() {
    // Single-lookup GSUB, type 5 (always dropped).
    let st = vec![0u8, 1u8];
    let gsub = build_gsub_one_lookup(b"DFLT", b"liga", 5, &st);

    let remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_gsub(&gsub, &remap);

    // Must be at least 10 bytes (valid GSUB v1.0 header).
    assert!(result.len() >= 10);
    // Major version must be 1, minor 0.
    assert_eq!(r16(&result, 0), 1);
    assert_eq!(r16(&result, 2), 0);
    // LookupList must exist and have count 0.
    assert_eq!(lookup_count(&result), 0);
    // FeatureList must exist and have count 0.
    assert_eq!(feature_count(&result), 0);
}

/// Build a GSUB v1.1 (FeatureVariations) — should be output as v1.0.
#[test]
fn test_gsub_v11_downgraded_to_v10() {
    // Build a minimal v1.0 GSUB and patch the minorVersion to 1 + append FV offset.
    let st = build_single_subst_f2(&[(1u16, 2u16)]);
    let mut gsub = build_gsub_one_lookup(b"DFLT", b"test", 1, &st);
    // Patch minorVersion at offset 2.
    patch16(&mut gsub, 2, 1);
    // Extend to at least 14 bytes for v1.1 header (Offset32 at offset 10).
    if gsub.len() < 14 {
        gsub.resize(14, 0);
    }
    // Write dummy FeatureVariations Offset32 at offset 10.
    let fv_off: u32 = 14;
    let b = fv_off.to_be_bytes();
    gsub[10] = b[0];
    gsub[11] = b[1];
    gsub[12] = b[2];
    gsub[13] = b[3];

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(1, 0);
    remap.insert(2, 1);

    let result = rewrite_gsub(&gsub, &remap);
    // Must be v1.0.
    assert_eq!(r16(&result, 0), 1);
    assert_eq!(r16(&result, 2), 0);
    // Should have the surviving lookup.
    assert_eq!(lookup_count(&result), 1);
}
