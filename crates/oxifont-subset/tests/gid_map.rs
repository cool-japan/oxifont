//! Tests for [`oxifont_subset::SubsetGidMap`] — the old↔new glyph-ID mapping
//! produced alongside a subset.
//!
//! A PDF embedder using `Identity-H` with `/CIDToGIDMap /Identity` has to emit
//! the subset's *new* glyph IDs as CIDs, so the renumbering the subsetter
//! performs must be recoverable from the public API. These tests pin the three
//! properties that make the map usable for that: it is exactly the dense rank
//! order of the composite-expanded closure, it round-trips in both directions,
//! and it lists a composite's components even when only the composite was
//! asked for.

use std::collections::{BTreeMap, BTreeSet};

use oxifont_subset::{glyf, tables, SubsetOptions};

/// Real TrueType fixture shared with `oxifont-parser`.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a `loca` entry, returning `(start, end)` byte offsets into `glyf`.
fn loca_entry(loca: &[u8], format: i16, gid: u16) -> Option<(usize, usize)> {
    let idx = gid as usize;
    if format == 0 {
        let s = loca.get(idx * 2..idx * 2 + 2)?;
        let e = loca.get((idx + 1) * 2..(idx + 1) * 2 + 2)?;
        Some((
            u16::from_be_bytes([s[0], s[1]]) as usize * 2,
            u16::from_be_bytes([e[0], e[1]]) as usize * 2,
        ))
    } else {
        let s = loca.get(idx * 4..idx * 4 + 4)?;
        let e = loca.get((idx + 1) * 4..(idx + 1) * 4 + 4)?;
        Some((
            u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize,
            u32::from_be_bytes([e[0], e[1], e[2], e[3]]) as usize,
        ))
    }
}

/// Direct components of `gid`, or an empty vector for a simple glyph.
fn components_of(gid: u16) -> Vec<u16> {
    let tables = tables::read_table_directory(TTF).expect("fixture must parse");
    let glyf = tables.get(b"glyf").copied().expect("fixture has glyf");
    let loca = tables.get(b"loca").copied().expect("fixture has loca");
    let head = tables.get(b"head").copied().expect("fixture has head");
    let format = i16::from_be_bytes([head[50], head[51]]);

    let Some((start, end)) = loca_entry(loca, format, gid) else {
        return Vec::new();
    };
    if start >= end || end > glyf.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    glyf::collect_composite_components(glyf, start, end, &mut out).expect("fixture glyph parses");
    out
}

/// Find the first glyph in the fixture that is a composite with at least one
/// component whose GID differs from its own.
fn find_composite() -> (u16, Vec<u16>) {
    let tables = tables::read_table_directory(TTF).expect("fixture must parse");
    let maxp = tables.get(b"maxp").copied().expect("fixture has maxp");
    let num_glyphs = u16::from_be_bytes([maxp[4], maxp[5]]);

    for gid in 1..num_glyphs {
        let comps = components_of(gid);
        if !comps.is_empty() && comps.iter().any(|&c| c != gid) {
            return (gid, comps);
        }
    }
    panic!("the fixture is expected to contain at least one composite glyph");
}

/// Transitive composite-component closure of `roots`, plus `.notdef`.
fn expected_closure(roots: &BTreeSet<u16>) -> BTreeSet<u16> {
    let mut closure: BTreeSet<u16> = roots.clone();
    closure.insert(0);
    let mut queue: Vec<u16> = closure.iter().copied().collect();
    while let Some(gid) = queue.pop() {
        for comp in components_of(gid) {
            if closure.insert(comp) {
                queue.push(comp);
            }
        }
    }
    closure
}

// ---------------------------------------------------------------------------
// (a) dense rank order
// ---------------------------------------------------------------------------

/// The map's new IDs must be exactly the rank of each old GID within the
/// composite-expanded closure, numbered densely from 0.
#[test]
fn new_ids_are_the_dense_rank_order_of_the_closure() {
    let (composite, _) = find_composite();
    let requested: BTreeSet<u16> = [composite].into_iter().collect();

    let (_bytes, stats, map) = oxifont_subset::subset_by_gids_mapped(TTF, &requested)
        .expect("subsetting a composite glyph must succeed");

    let closure = expected_closure(&requested);
    assert_eq!(
        map.len(),
        closure.len(),
        "the map must cover the whole composite closure"
    );
    assert_eq!(
        u16::try_from(map.len()).unwrap_or(u16::MAX),
        stats.glyphs_retained,
        "the map must have one entry per retained glyph"
    );

    for (rank, &old) in closure.iter().enumerate() {
        let expected_new = u16::try_from(rank).unwrap_or(u16::MAX);
        assert_eq!(
            map.new_gid(old),
            Some(expected_new),
            "old GID {old} must map to its rank {rank} in the closure"
        );
    }

    // The map's own iteration order agrees.
    let pairs: Vec<(u16, u16)> = map.iter().collect();
    let expected: Vec<(u16, u16)> = closure
        .iter()
        .enumerate()
        .map(|(rank, &old)| (old, u16::try_from(rank).unwrap_or(u16::MAX)))
        .collect();
    assert_eq!(pairs, expected, "iter() must yield (old, new) in old order");

    // `new_to_old` is the same information indexed by new GID.
    let by_new: Vec<u16> = closure.iter().copied().collect();
    assert_eq!(map.new_to_old(), by_new.as_slice());
}

// ---------------------------------------------------------------------------
// (b) round-trip
// ---------------------------------------------------------------------------

/// old → new → old must be the identity over every mapped glyph, and the
/// reverse direction must be total over `0..len`.
#[test]
fn old_to_new_to_old_round_trips() {
    let codepoints: BTreeSet<char> = ('A'..='Z').chain('\u{00C0}'..='\u{00FF}').collect();
    let opts = SubsetOptions::default();
    let (_bytes, _stats, map) =
        oxifont_subset::subset_font_with_options_mapped(TTF, &codepoints, &opts)
            .expect("subset must succeed");

    assert!(map.len() > 1, "expected more than just .notdef");

    for (old, new) in map.iter() {
        assert_eq!(
            map.old_gid(new),
            Some(old),
            "new GID {new} must map back to old GID {old}"
        );
    }
    for new in 0..u16::try_from(map.len()).unwrap_or(u16::MAX) {
        let old = map.old_gid(new).expect("new GID space must be dense");
        assert_eq!(map.new_gid(old), Some(new), "old→new→old must be identity");
    }

    // `.notdef` is always present and always stays GID 0.
    assert_eq!(map.new_gid(0), Some(0));
    assert_eq!(map.old_gid(0), Some(0));

    // A GID that was never requested is absent from both directions.
    let absent = u16::try_from(map.len()).unwrap_or(u16::MAX);
    assert_eq!(map.old_gid(absent), None);
    assert!(!map.is_empty());
}

// ---------------------------------------------------------------------------
// (c) composite components are in the map
// ---------------------------------------------------------------------------

/// Requesting only a composite still lists its components in the map — that is
/// exactly the information a caller cannot otherwise recover.
#[test]
fn composite_components_appear_in_the_map() {
    let (composite, comps) = find_composite();
    let requested: BTreeSet<u16> = [composite].into_iter().collect();

    let (_bytes, _stats, map) =
        oxifont_subset::subset_by_gids_mapped(TTF, &requested).expect("subset must succeed");

    assert!(
        map.new_gid(composite).is_some(),
        "the requested composite must be mapped"
    );
    for comp in comps {
        assert!(
            map.new_gid(comp).is_some(),
            "component GID {comp} must be mapped even though only the composite was requested"
        );
    }
}

// ---------------------------------------------------------------------------
// Entry-point coverage
// ---------------------------------------------------------------------------

/// The `*_mapped` entry points must produce byte-identical output to their
/// existing siblings — they only add the map.
#[test]
fn mapped_entry_points_match_their_unmapped_siblings() {
    let codepoints: BTreeSet<char> = "Hello, world!".chars().collect();
    let opts = SubsetOptions::default();

    let (plain_bytes, plain_stats) =
        oxifont_subset::subset_font_with_options(TTF, &codepoints, &opts)
            .expect("plain subset must succeed");
    let (mapped_bytes, mapped_stats, _map) =
        oxifont_subset::subset_font_with_options_mapped(TTF, &codepoints, &opts)
            .expect("mapped subset must succeed");
    assert_eq!(plain_bytes, mapped_bytes);
    assert_eq!(plain_stats.glyphs_retained, mapped_stats.glyphs_retained);

    let gids: BTreeSet<u16> = [1u16, 2, 3].into_iter().collect();
    let plain_gid_bytes = oxifont_subset::subset_by_gids(TTF, &gids).expect("subset_by_gids");
    let (mapped_gid_bytes, _, _) =
        oxifont_subset::subset_by_gids_mapped(TTF, &gids).expect("subset_by_gids_mapped");
    assert_eq!(plain_gid_bytes, mapped_gid_bytes);

    let gid_set: BTreeSet<u16> = [0u16, 5, 9].into_iter().collect();
    let cp_to_old: BTreeMap<u32, u16> = BTreeMap::new();
    let (plain_core, _) = oxifont_subset::subset_with_gid_set(TTF, &gid_set, &cp_to_old, &opts)
        .expect("subset_with_gid_set");
    let (mapped_core, _, core_map) =
        oxifont_subset::subset_with_gid_set_mapped(TTF, &gid_set, &cp_to_old, &opts)
            .expect("subset_with_gid_set_mapped");
    assert_eq!(plain_core, mapped_core);
    assert_eq!(core_map.new_gid(5), Some(1));
    assert_eq!(core_map.new_gid(9), Some(2));
}

/// The PDF accumulator — the caller that actually needs the CID assignment —
/// exposes the map from both `finalize_mapped` and `PdfSubsetResult`.
#[test]
fn pdf_subsetter_exposes_the_gid_map() {
    use oxifont_subset::pdf_subset::PdfFontSubsetter;

    let (composite, comps) = find_composite();

    let mut subsetter = PdfFontSubsetter::for_pdf(TTF.to_vec());
    subsetter.add_text("Hi");
    subsetter.add_gid(composite);

    let (bytes, stats, map) = subsetter
        .finalize_mapped()
        .expect("finalize_mapped must succeed");
    assert_eq!(
        u16::try_from(map.len()).unwrap_or(u16::MAX),
        stats.glyphs_retained
    );
    for comp in &comps {
        assert!(
            map.new_gid(*comp).is_some(),
            "component GID {comp} must be reachable from the PDF path"
        );
    }

    let result = subsetter
        .finalize_into_result()
        .expect("finalize_into_result must succeed");
    assert_eq!(result.bytes, bytes);
    assert_eq!(result.gid_map.new_to_old(), map.new_to_old());

    // The plain `finalize` is unchanged.
    let (plain_bytes, plain_stats) = subsetter.finalize().expect("finalize must succeed");
    assert_eq!(plain_bytes, bytes);
    assert_eq!(plain_stats.glyphs_retained, stats.glyphs_retained);
}
