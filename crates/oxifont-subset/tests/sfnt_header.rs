//! Tests for the SFNT header emitted by [`oxifont_subset::tables::build_sfnt`].
//!
//! Two properties are pinned here:
//!
//! * the sfnt version must be `OTTO` when the assembled table list carries CFF
//!   outlines (`CFF ` or `CFF2`) and `0x00010000` otherwise, so that consumers
//!   which dispatch on the magic look for the outline table that is actually
//!   present;
//! * `searchRange` / `entrySelector` / `rangeShift` must follow the OpenType
//!   offset-table formulas, including when `numTables` is an exact power of two.

use std::borrow::Cow;
use std::collections::BTreeSet;

use oxifont_subset::tables::build_sfnt;

/// TrueType-flavoured sfnt version.
const SFNT_MAGIC_TT: u32 = 0x0001_0000;
/// CFF-flavoured sfnt version (`OTTO`).
const SFNT_MAGIC_OTTO: u32 = 0x4F54_544F;

/// Real TrueType fixture shared with `oxifont-parser`.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn be_u32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn be_u16(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data[at], data[at + 1]])
}

/// `n` distinct dummy tables, tagged `TST0`, `TST1`, … (`n` must be < 10).
fn dummy_tables(n: usize) -> Vec<([u8; 4], Cow<'static, [u8]>)> {
    (0..n)
        .map(|i| {
            let tag = [b'T', b'S', b'T', b'0' + u8::try_from(i).unwrap_or(0)];
            (tag, Cow::Owned(vec![0xABu8; 4]))
        })
        .collect()
}

/// Assemble an SFNT with an explicit sfnt version, independently of
/// `build_sfnt` (so the header tests do not validate the writer against
/// itself).
fn assemble_sfnt(version: u32, tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sorted: Vec<&([u8; 4], Vec<u8>)> = tables.iter().collect();
    sorted.sort_by_key(|(tag, _)| *tag);

    let num_tables = sorted.len();
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // searchRange   (not validated here)
    out.extend_from_slice(&0u16.to_be_bytes()); // entrySelector (not validated here)
    out.extend_from_slice(&0u16.to_be_bytes()); // rangeShift    (not validated here)

    let dir_start = out.len();
    out.resize(dir_start + num_tables * 16, 0);

    for (i, (tag, data)) in sorted.iter().enumerate() {
        let offset = out.len() as u32;
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let base = dir_start + i * 16;
        out[base..base + 4].copy_from_slice(tag);
        out[base + 4..base + 8].copy_from_slice(&0u32.to_be_bytes());
        out[base + 8..base + 12].copy_from_slice(&offset.to_be_bytes());
        out[base + 12..base + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
    }
    out
}

/// A minimal `head` table: 54 bytes, `unitsPerEm` = 1000, `indexToLocFormat` = 0.
fn minimal_head() -> Vec<u8> {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head[12..14].copy_from_slice(&0x5F0Fu16.to_be_bytes()); // magicNumber (high half)
    head[14..16].copy_from_slice(&0x3CF5u16.to_be_bytes()); // magicNumber (low half)
    head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head
}

/// A minimal CFF-flavoured font: `CFF `, `head`, `hhea`, `hmtx`, `maxp`.
///
/// The `CFF ` payload is deliberately unparseable so the pipeline takes its
/// documented verbatim-copy fallback; this test is about the sfnt version in
/// the header, not about CharString rewriting.
fn minimal_cff_font() -> Vec<u8> {
    let mut hhea = vec![0u8; 36];
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes()); // numberOfHMetrics
    let mut maxp = vec![0u8; 6];
    maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5 (CFF)
    maxp[4..6].copy_from_slice(&1u16.to_be_bytes()); // numGlyphs

    assemble_sfnt(
        SFNT_MAGIC_OTTO,
        &[
            (
                *b"CFF ",
                vec![0x01, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00],
            ),
            (*b"head", minimal_head()),
            (*b"hhea", hhea),
            (*b"hmtx", vec![0x02, 0x00, 0x00, 0x00]),
            (*b"maxp", maxp),
        ],
    )
}

// ---------------------------------------------------------------------------
// sfnt version selection
// ---------------------------------------------------------------------------

/// A table list carrying a `CFF ` table must be stamped `OTTO`.
#[test]
fn build_sfnt_stamps_otto_for_cff_outlines() {
    let tables: Vec<([u8; 4], Cow<'static, [u8]>)> = vec![
        (*b"CFF ", Cow::Owned(vec![0u8; 16])),
        (*b"head", Cow::Owned(minimal_head())),
    ];
    let sfnt = build_sfnt(&tables);
    assert_eq!(
        be_u32(&sfnt, 0),
        SFNT_MAGIC_OTTO,
        "a subset holding a `CFF ` table must be stamped OTTO, not the TrueType magic"
    );
}

/// A table list carrying a `CFF2` table must be stamped `OTTO` as well.
#[test]
fn build_sfnt_stamps_otto_for_cff2_outlines() {
    let tables: Vec<([u8; 4], Cow<'static, [u8]>)> = vec![
        (*b"CFF2", Cow::Owned(vec![0u8; 16])),
        (*b"head", Cow::Owned(minimal_head())),
    ];
    let sfnt = build_sfnt(&tables);
    assert_eq!(
        be_u32(&sfnt, 0),
        SFNT_MAGIC_OTTO,
        "a subset holding a `CFF2` table must be stamped OTTO"
    );
}

/// A `glyf`-flavoured table list keeps the TrueType magic.
#[test]
fn build_sfnt_keeps_truetype_magic_for_glyf_outlines() {
    let tables: Vec<([u8; 4], Cow<'static, [u8]>)> = vec![
        (*b"glyf", Cow::Owned(vec![0u8; 16])),
        (*b"head", Cow::Owned(minimal_head())),
        (*b"loca", Cow::Owned(vec![0u8; 4])),
    ];
    let sfnt = build_sfnt(&tables);
    assert_eq!(
        be_u32(&sfnt, 0),
        SFNT_MAGIC_TT,
        "a glyf-flavoured subset must keep the 0x00010000 magic"
    );
}

/// End-to-end: subsetting a CFF-flavoured input produces an `OTTO` subset.
#[test]
fn cff_input_round_trips_with_otto_magic() {
    let font = minimal_cff_font();
    assert_eq!(be_u32(&font, 0), SFNT_MAGIC_OTTO, "input must be OTTO");

    let gids: BTreeSet<u16> = BTreeSet::new(); // .notdef only
    let subset = oxifont_subset::subset_by_gids(&font, &gids)
        .expect("subsetting a minimal CFF font must succeed");

    assert_eq!(
        be_u32(&subset, 0),
        SFNT_MAGIC_OTTO,
        "a CFF-flavoured subset must round-trip as OTTO"
    );
}

/// End-to-end: subsetting a real TrueType font keeps `0x00010000`.
#[test]
fn glyf_input_round_trips_with_truetype_magic() {
    let codepoints: BTreeSet<char> = ['A', 'B'].into_iter().collect();
    let subset = oxifont_subset::subset_font(TTF, &codepoints).expect("subset must succeed");
    assert_eq!(
        be_u32(&subset, 0),
        SFNT_MAGIC_TT,
        "a TrueType subset must keep the 0x00010000 magic"
    );
}

// ---------------------------------------------------------------------------
// searchRange / entrySelector / rangeShift
// ---------------------------------------------------------------------------

/// `numTables` = 8 (an exact power of two) — the case the old
/// `16 * next_power_of_two(n) / 2` formula got wrong.
#[test]
fn search_fields_match_spec_at_exact_power_of_two() {
    let sfnt = build_sfnt(&dummy_tables(8));
    assert_eq!(be_u16(&sfnt, 4), 8, "numTables");
    assert_eq!(
        be_u16(&sfnt, 6),
        128,
        "searchRange = 2^floor(log2(8)) * 16 = 128"
    );
    assert_eq!(be_u16(&sfnt, 8), 3, "entrySelector = floor(log2(8)) = 3");
    assert_eq!(be_u16(&sfnt, 10), 0, "rangeShift = 8*16 - 128 = 0");
}

/// `numTables` = 9 — one past a power of two.
#[test]
fn search_fields_match_spec_just_past_power_of_two() {
    let sfnt = build_sfnt(&dummy_tables(9));
    assert_eq!(be_u16(&sfnt, 4), 9, "numTables");
    assert_eq!(
        be_u16(&sfnt, 6),
        128,
        "searchRange = 2^floor(log2(9)) * 16 = 128"
    );
    assert_eq!(be_u16(&sfnt, 8), 3, "entrySelector = floor(log2(9)) = 3");
    assert_eq!(be_u16(&sfnt, 10), 16, "rangeShift = 9*16 - 128 = 16");
}

/// The three fields must satisfy the spec invariants for every small table
/// count, not just the two hand-checked ones.
#[test]
fn search_fields_are_self_consistent_for_all_small_counts() {
    for n in 1..=9usize {
        let sfnt = build_sfnt(&dummy_tables(n));
        let num_tables = be_u16(&sfnt, 4);
        let search_range = be_u16(&sfnt, 6);
        let entry_selector = be_u16(&sfnt, 8);
        let range_shift = be_u16(&sfnt, 10);

        assert_eq!(num_tables as usize, n, "numTables for n = {n}");
        assert_eq!(
            search_range,
            16u16 << entry_selector,
            "searchRange must be 16 * 2^entrySelector for n = {n}"
        );
        assert!(
            search_range <= num_tables * 16,
            "searchRange must not exceed numTables*16 for n = {n}"
        );
        assert_eq!(
            search_range + range_shift,
            num_tables * 16,
            "searchRange + rangeShift must equal numTables*16 for n = {n}"
        );
    }
}

/// An empty table list must not panic and must emit an all-zero search block.
#[test]
fn search_fields_are_zero_for_empty_table_list() {
    let sfnt = build_sfnt(&[]);
    assert!(sfnt.len() >= 12, "header must still be written");
    assert_eq!(be_u16(&sfnt, 4), 0, "numTables");
    assert_eq!(be_u16(&sfnt, 6), 0, "searchRange");
    assert_eq!(be_u16(&sfnt, 8), 0, "entrySelector");
    assert_eq!(be_u16(&sfnt, 10), 0, "rangeShift");
}
