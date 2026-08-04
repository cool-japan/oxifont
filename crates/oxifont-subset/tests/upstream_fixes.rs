//! Contract tests for the 0.2.2 upstream fixes that ride along with static
//! instancing: `.notdef` retention across every entry point, the unconditional
//! `DSIG` drop, [`SubsetOptions::drop_variations`], and the
//! [`SubsetStats::cff_charstrings_verbatim`] indicator.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use oxifont_subset::{
    face_count, subset_by_gids_mapped, subset_font_with_options_mapped, subset_with_gid_set_mapped,
    tables, SubsetOptions, SubsetStats,
};
use ttf_parser::{Face, Tag};

/// Real TrueType fixture shared with `oxifont-parser`.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");
/// A static, layout-rich face for the DSIG / options assertions.
static NOTO: &[u8] = include_bytes!("../../oxifont-bundled/fonts/NotoSans-Regular.ttf");

fn system_font(name: &str) -> Option<Vec<u8>> {
    let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"))?;
    std::fs::read(PathBuf::from(root).join("Fonts").join(name)).ok()
}

fn tags_of(bytes: &[u8]) -> Vec<[u8; 4]> {
    let mut tags: Vec<[u8; 4]> = tables::read_table_directory(bytes)
        .expect("directory")
        .keys()
        .copied()
        .collect();
    tags.sort_unstable();
    tags
}

// ---------------------------------------------------------------------------
// `.notdef` is retained by every entry point
// ---------------------------------------------------------------------------

/// `subset_with_gid_set*` used to drop `.notdef` while `subset_by_gids*` kept
/// it, so the same glyph set produced two different numberings depending on
/// which door you came in. A PDF CIDFont built on the first one silently turns
/// glyph 0 into whatever the caller's lowest requested glyph was — U+0020 in
/// the common case, which then fails a `cid != 0` assertion on the first space
/// in the document.
#[test]
fn every_entry_point_retains_notdef_as_glyph_zero() {
    let requested: BTreeSet<u16> = [3u16, 5, 9].into_iter().collect();
    let empty: BTreeMap<u32, u16> = BTreeMap::new();
    let opts = SubsetOptions::default();

    let (_, gid_set_stats, gid_set_map) =
        subset_with_gid_set_mapped(TTF, &requested, &empty, &opts).expect("gid set subset");
    assert_eq!(
        gid_set_map.new_gid(0),
        Some(0),
        "subset_with_gid_set_mapped lost .notdef"
    );

    let (_, by_gids_stats, by_gids_map) =
        subset_by_gids_mapped(TTF, &requested).expect("by gids subset");
    assert_eq!(by_gids_map.new_gid(0), Some(0));

    // The two doors now agree on the numbering, not merely on the glyph set.
    assert_eq!(gid_set_stats.glyphs_retained, by_gids_stats.glyphs_retained);
    for gid in &requested {
        assert_eq!(gid_set_map.new_gid(*gid), by_gids_map.new_gid(*gid));
    }

    // …and asking for gid 0 explicitly changes nothing.
    let mut with_zero = requested.clone();
    with_zero.insert(0);
    let (_, explicit_stats, explicit_map) =
        subset_with_gid_set_mapped(TTF, &with_zero, &empty, &opts).expect("subset");
    assert_eq!(
        explicit_stats.glyphs_retained,
        gid_set_stats.glyphs_retained
    );
    for gid in &requested {
        assert_eq!(explicit_map.new_gid(*gid), gid_set_map.new_gid(*gid));
    }
}

#[test]
fn the_codepoint_entry_point_also_retains_notdef() {
    let cps: BTreeSet<char> = ['A', 'B'].into_iter().collect();
    let (_, _stats, map) =
        subset_font_with_options_mapped(TTF, &cps, &SubsetOptions::default()).expect("subset");
    assert_eq!(map.new_gid(0), Some(0));
}

#[test]
fn an_empty_request_still_produces_a_notdef_only_font() {
    let empty_gids: BTreeSet<u16> = BTreeSet::new();
    let empty_cps: BTreeMap<u32, u16> = BTreeMap::new();
    let (bytes, stats, map) =
        subset_with_gid_set_mapped(TTF, &empty_gids, &empty_cps, &SubsetOptions::default())
            .expect("subset");
    assert_eq!(stats.glyphs_retained, 1);
    assert_eq!(map.new_gid(0), Some(0));
    let face = Face::parse(&bytes, 0).expect("parse");
    assert_eq!(face.number_of_glyphs(), 1);
}

// ---------------------------------------------------------------------------
// DSIG
// ---------------------------------------------------------------------------

/// A `DSIG` signs the bytes of the font it was made for. Every table in a
/// subset has been rewritten, so carrying it over ships a signature that is
/// invalid by construction — and 8–10 KB of it on a stock Windows UI face.
#[test]
fn no_subset_carries_a_dsig() {
    let cps: BTreeSet<char> = "Hello".chars().collect();
    for opts in [
        SubsetOptions::default(),
        SubsetOptions::default()
            .strip_hints(true)
            .retain_names(false)
            .retain_layout_tables(false),
    ] {
        let (bytes, stats, _) = subset_font_with_options_mapped(NOTO, &cps, &opts).expect("subset");
        assert!(
            !stats.tables_retained.contains(b"DSIG"),
            "DSIG reported in stats"
        );
        assert!(!tags_of(&bytes).contains(b"DSIG"), "DSIG in the output");
    }
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_a_dsig_bearing_face_loses_it() {
    let Some(data) = system_font("bahnschrift.ttf") else {
        eprintln!("skipping: bahnschrift.ttf is not installed");
        return;
    };
    // The source really does carry one, so the assertion is not vacuous.
    let raw = ttf_parser::RawFace::parse(&data, 0).expect("raw");
    assert!(
        raw.table(Tag::from_bytes(b"DSIG")).is_some(),
        "fixture assumption broken: bahnschrift has no DSIG"
    );
    let cps: BTreeSet<char> = "AB".chars().collect();
    let (bytes, _, _) =
        subset_font_with_options_mapped(&data, &cps, &SubsetOptions::default()).expect("subset");
    assert!(!tags_of(&bytes).contains(b"DSIG"));
}

// ---------------------------------------------------------------------------
// drop_variations
// ---------------------------------------------------------------------------

const VARIATION_TAGS: [&[u8; 4]; 8] = [
    b"fvar", b"avar", b"gvar", b"cvar", b"HVAR", b"VVAR", b"MVAR", b"STAT",
];

#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_drop_variations_produces_a_static_subset() {
    let Some(data) = system_font("bahnschrift.ttf") else {
        eprintln!("skipping: bahnschrift.ttf is not installed");
        return;
    };
    let cps: BTreeSet<char> = "Ag".chars().collect();
    let keep = SubsetOptions::default()
        .retain_layout_tables(false)
        .retain_names(false);
    let drop = keep.clone().drop_variations(true);

    let (kept_bytes, _, _) = subset_font_with_options_mapped(&data, &cps, &keep).expect("subset");
    let (dropped_bytes, dropped_stats, _) =
        subset_font_with_options_mapped(&data, &cps, &drop).expect("subset");

    let kept_tags = tags_of(&kept_bytes);
    // Without the flag a variable face stays variable — that is the defect the
    // flag exists to fix, so assert the precondition too.
    assert!(
        VARIATION_TAGS.iter().any(|t| kept_tags.contains(t)),
        "fixture assumption broken: no variation tables survived by default"
    );
    let dropped_tags = tags_of(&dropped_bytes);
    for tag in VARIATION_TAGS {
        assert!(
            !dropped_tags.contains(tag),
            "{} survived drop_variations",
            std::str::from_utf8(tag).unwrap_or("????")
        );
        assert!(!dropped_stats.tables_retained.contains(tag));
    }
    assert!(
        dropped_bytes.len() < kept_bytes.len(),
        "dropping the variation tables did not shrink the subset"
    );
    let face = Face::parse(&dropped_bytes, 0).expect("parse");
    assert!(!face.is_variable());
    // Outlines and glyph count are untouched: only the axes are gone.
    let kept_face = Face::parse(&kept_bytes, 0).expect("parse");
    assert_eq!(face.number_of_glyphs(), kept_face.number_of_glyphs());
}

/// A synthetic variable face, so the flag is covered on every machine.
#[test]
fn drop_variations_removes_every_variation_table() {
    let font = synthetic_variable_font();
    let tags = tags_of(&font);
    assert!(tags.contains(b"fvar") && tags.contains(b"gvar") && tags.contains(b"HVAR"));

    let gids: BTreeSet<u16> = [1u16].into_iter().collect();
    let empty: BTreeMap<u32, u16> = BTreeMap::new();

    let keep = SubsetOptions::default();
    let (kept, _, _) = subset_with_gid_set_mapped(&font, &gids, &empty, &keep).expect("subset");
    let kept_tags = tags_of(&kept);
    assert!(
        VARIATION_TAGS.iter().any(|t| kept_tags.contains(t)),
        "default options should keep the face variable"
    );

    let drop = SubsetOptions::default().drop_variations(true);
    let (dropped, stats, _) =
        subset_with_gid_set_mapped(&font, &gids, &empty, &drop).expect("subset");
    for tag in VARIATION_TAGS {
        assert!(
            !tags_of(&dropped).contains(tag),
            "{} survived",
            std::str::from_utf8(tag).unwrap_or("????")
        );
    }
    assert!(!stats.tables_retained.contains(b"fvar"));
    // The outline tables are still there; only the variation machinery went.
    assert!(tags_of(&dropped).contains(b"glyf"));
}

#[test]
fn drop_variations_is_a_no_op_on_an_instanced_face() {
    let font = synthetic_variable_font();
    let static_bytes = oxifont_subset::instance(&font, 0, &[(*b"wght", 700.0)]).expect("instance");
    let gids: BTreeSet<u16> = [1u16].into_iter().collect();
    let empty: BTreeMap<u32, u16> = BTreeMap::new();
    let a = subset_with_gid_set_mapped(&static_bytes, &gids, &empty, &SubsetOptions::default())
        .expect("subset")
        .0;
    let b = subset_with_gid_set_mapped(
        &static_bytes,
        &gids,
        &empty,
        &SubsetOptions::default().drop_variations(true),
    )
    .expect("subset")
    .0;
    assert_eq!(a, b, "the flag changed an already-static font");
}

// ---------------------------------------------------------------------------
// SubsetStats
// ---------------------------------------------------------------------------

#[test]
fn a_glyf_subset_never_reports_verbatim_cff_charstrings() {
    let cps: BTreeSet<char> = "Hello".chars().collect();
    let (_, stats, _) =
        subset_font_with_options_mapped(NOTO, &cps, &SubsetOptions::default()).expect("subset");
    assert!(!stats.cff_charstrings_verbatim);
    assert_stats_is_debug(&stats);
}

fn assert_stats_is_debug(stats: &SubsetStats) {
    // The field is part of the public surface; make sure it renders.
    assert!(format!("{stats:?}").contains("cff_charstrings_verbatim"));
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn subsetting_is_byte_deterministic_across_runs() {
    let cps: BTreeSet<char> = "The quick brown fox".chars().collect();
    let opts = SubsetOptions::default().drop_variations(true);
    let first = subset_font_with_options_mapped(NOTO, &cps, &opts)
        .expect("subset")
        .0;
    for _ in 0..2 {
        let again = subset_font_with_options_mapped(NOTO, &cps, &opts)
            .expect("subset")
            .0;
        assert_eq!(first, again);
    }
}

#[test]
fn face_count_of_the_fixture_is_one() {
    assert_eq!(face_count(TTF).expect("face count"), 1);
}

// ---------------------------------------------------------------------------
// A minimal variable face
// ---------------------------------------------------------------------------

/// Two glyphs, one axis, and one of every variation table the flag must remove.
fn synthetic_variable_font() -> Vec<u8> {
    fn be16(v: u16) -> [u8; 2] {
        v.to_be_bytes()
    }
    fn be32(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    let mut glyph = Vec::new();
    glyph.extend_from_slice(&be16(1)); // numberOfContours
    for v in [0i16, 0, 500, 700] {
        glyph.extend_from_slice(&v.to_be_bytes());
    }
    glyph.extend_from_slice(&be16(3)); // endPtsOfContours[0]
    glyph.extend_from_slice(&be16(0)); // instructionLength
    glyph.extend(std::iter::repeat_n(0x01u8, 4)); // on-curve, long coords
    for d in [0i16, 500, 0, -500] {
        glyph.extend_from_slice(&d.to_be_bytes());
    }
    for d in [0i16, 0, 700, 0] {
        glyph.extend_from_slice(&d.to_be_bytes());
    }
    while !glyph.len().is_multiple_of(4) {
        glyph.push(0);
    }

    let mut glyf = Vec::new();
    let mut offsets = vec![0u32];
    for g in [Vec::new(), glyph] {
        glyf.extend_from_slice(&g);
        offsets.push(glyf.len() as u32);
    }
    let loca: Vec<u8> = offsets.iter().flat_map(|o| be32(*o)).collect();

    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&be32(0x0001_0000));
    head[12..16].copy_from_slice(&be32(0x5F0F_3CF5));
    head[18..20].copy_from_slice(&be16(1000));
    head[50..52].copy_from_slice(&1i16.to_be_bytes());

    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&be32(0x0001_0000));
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[34..36].copy_from_slice(&be16(2));

    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&be32(0x0001_0000));
    maxp[4..6].copy_from_slice(&be16(2));

    let hmtx: Vec<u8> = [(500u16, 0i16), (600, 0)]
        .iter()
        .flat_map(|(a, l)| {
            let mut v = a.to_be_bytes().to_vec();
            v.extend_from_slice(&l.to_be_bytes());
            v
        })
        .collect();

    let mut fvar = Vec::new();
    fvar.extend_from_slice(&be16(1));
    fvar.extend_from_slice(&be16(0));
    fvar.extend_from_slice(&be16(16));
    fvar.extend_from_slice(&be16(2));
    fvar.extend_from_slice(&be16(1));
    fvar.extend_from_slice(&be16(20));
    fvar.extend_from_slice(&be16(0));
    fvar.extend_from_slice(&be16(0));
    fvar.extend_from_slice(b"wght");
    for v in [100.0f32, 400.0, 900.0] {
        fvar.extend_from_slice(&be32(((v * 65536.0) as i32) as u32));
    }
    fvar.extend_from_slice(&be16(0));
    fvar.extend_from_slice(&be16(0));

    // gvar: header + offsets + one shared tuple + two empty per-glyph blocks.
    let mut gvar = Vec::new();
    gvar.extend_from_slice(&be16(1));
    gvar.extend_from_slice(&be16(0));
    gvar.extend_from_slice(&be16(1)); // axisCount
    gvar.extend_from_slice(&be16(1)); // sharedTupleCount
    gvar.extend_from_slice(&be32(32)); // sharedTuplesOffset
    gvar.extend_from_slice(&be16(2)); // glyphCount
    gvar.extend_from_slice(&be16(1)); // LONG_OFFSETS
    gvar.extend_from_slice(&be32(34)); // glyphVariationDataArrayOffset
    for _ in 0..3 {
        gvar.extend_from_slice(&be32(0));
    }
    gvar.extend_from_slice(&16384i16.to_be_bytes());

    let tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"hmtx", hmtx),
        (*b"glyf", glyf),
        (*b"loca", loca),
        (*b"fvar", fvar),
        (*b"gvar", gvar),
        (*b"avar", vec![0, 1, 0, 0, 0, 0, 0, 1, 0, 0]),
        (
            *b"HVAR",
            vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            *b"VVAR",
            vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (*b"MVAR", vec![0, 1, 0, 0, 0, 0, 0, 0]),
        (*b"STAT", vec![0, 1, 0, 0, 0, 0, 0, 0]),
        (*b"cvar", vec![0, 1, 0, 0, 0, 0, 0, 8]),
    ];

    let mut sorted: Vec<&([u8; 4], Vec<u8>)> = tables.iter().collect();
    sorted.sort_by_key(|(tag, _)| *tag);
    let n = sorted.len();
    let mut out = Vec::new();
    out.extend_from_slice(&be32(0x0001_0000));
    out.extend_from_slice(&be16(n as u16));
    out.extend_from_slice(&be16(16));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));
    let dir = out.len();
    out.resize(dir + n * 16, 0);
    for (i, (tag, data)) in sorted.iter().enumerate() {
        let offset = out.len() as u32;
        out.extend_from_slice(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        let base = dir + i * 16;
        out[base..base + 4].copy_from_slice(tag);
        out[base + 8..base + 12].copy_from_slice(&be32(offset));
        out[base + 12..base + 16].copy_from_slice(&be32(data.len() as u32));
    }
    out
}
