//! Tests for TrueType Collection (`ttcf`) support in the subsetter.
//!
//! Windows ships every stock CJK face inside a `.ttc` container
//! (`msgothic.ttc`, `meiryo.ttc`, `YuGothM.ttc`, `msyh.ttc`, …), so the
//! subsetter has to be able to select a face out of a collection. The
//! `*_at_face` entry points do that; the historical offset-0 entry points keep
//! refusing a collection, matching `SfntTableMap::parse`.
//!
//! The collection used by most tests here is hand-built from the shared
//! TrueType fixture so the suite does not depend on any system font.

use std::collections::BTreeSet;

use oxifont_subset::SubsetError;

/// Real TrueType fixture shared with `oxifont-parser`.
static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

/// `unitsPerEm` stamped into face 1 of the synthetic collection so that the two
/// faces are distinguishable in the subset output.
const FACE1_UNITS_PER_EM: u16 = 1234;

// ---------------------------------------------------------------------------
// Synthetic TTC builder
// ---------------------------------------------------------------------------

/// Read `(tag, bytes)` for every table of a plain per-face SFNT.
fn read_tables(sfnt: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    let mut out = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        let tag = [sfnt[rec], sfnt[rec + 1], sfnt[rec + 2], sfnt[rec + 3]];
        let offset =
            u32::from_be_bytes([sfnt[rec + 8], sfnt[rec + 9], sfnt[rec + 10], sfnt[rec + 11]])
                as usize;
        let length = u32::from_be_bytes([
            sfnt[rec + 12],
            sfnt[rec + 13],
            sfnt[rec + 14],
            sfnt[rec + 15],
        ]) as usize;
        out.push((tag, sfnt[offset..offset + length].to_vec()));
    }
    out
}

/// Build a spec-shaped two-face `ttcf` collection wrapping two copies of
/// `sfnt`.
///
/// Face 0 is byte-identical to the input's tables; face 1 differs only in
/// `head.unitsPerEm`, which is set to [`FACE1_UNITS_PER_EM`] so a caller can
/// tell which face a subset came from. Table records use absolute offsets from
/// the start of the collection, exactly as the OpenType spec requires.
fn build_two_face_ttc(sfnt: &[u8]) -> Vec<u8> {
    let sfnt_version = u32::from_be_bytes([sfnt[0], sfnt[1], sfnt[2], sfnt[3]]);
    let face0 = read_tables(sfnt);
    let mut face1 = face0.clone();
    for (tag, data) in &mut face1 {
        if tag == b"head" {
            data[18..20].copy_from_slice(&FACE1_UNITS_PER_EM.to_be_bytes());
        }
    }

    let n = face0.len();
    // TTC header: tag(4) + version(4) + numFonts(4) + offsetTable(4 * numFonts).
    let ttc_header_len = 12 + 4 * 2;
    let face0_dir = ttc_header_len;
    let face1_dir = face0_dir + 12 + n * 16;
    let mut cursor = face1_dir + 12 + n * 16;
    cursor = (cursor + 3) & !3;

    // Lay out both faces' table bodies, recording absolute offsets.
    let mut placed: Vec<Vec<(usize, usize)>> = Vec::with_capacity(2);
    for face in [&face0, &face1] {
        let mut offsets = Vec::with_capacity(face.len());
        for (_, data) in face.iter() {
            offsets.push((cursor, data.len()));
            cursor += (data.len() + 3) & !3;
        }
        placed.push(offsets);
    }
    let total = cursor;

    let mut out = vec![0u8; total];
    out[0..4].copy_from_slice(b"ttcf");
    out[4..6].copy_from_slice(&1u16.to_be_bytes()); // majorVersion
    out[6..8].copy_from_slice(&0u16.to_be_bytes()); // minorVersion
    out[8..12].copy_from_slice(&2u32.to_be_bytes()); // numFonts
    out[12..16].copy_from_slice(&(face0_dir as u32).to_be_bytes());
    out[16..20].copy_from_slice(&(face1_dir as u32).to_be_bytes());

    for (face_idx, (face, dir_start)) in [(&face0, face0_dir), (&face1, face1_dir)]
        .into_iter()
        .enumerate()
    {
        out[dir_start..dir_start + 4].copy_from_slice(&sfnt_version.to_be_bytes());
        out[dir_start + 4..dir_start + 6].copy_from_slice(&(n as u16).to_be_bytes());
        // searchRange / entrySelector / rangeShift are left zero: nothing in the
        // parsing path consults them.
        for (i, (tag, data)) in face.iter().enumerate() {
            let (offset, length) = placed[face_idx][i];
            let rec = dir_start + 12 + i * 16;
            out[rec..rec + 4].copy_from_slice(tag);
            out[rec + 8..rec + 12].copy_from_slice(&(offset as u32).to_be_bytes());
            out[rec + 12..rec + 16].copy_from_slice(&(length as u32).to_be_bytes());
            out[offset..offset + length].copy_from_slice(data);
        }
    }

    out
}

/// Read `head.unitsPerEm` out of a plain per-face SFNT.
fn units_per_em(sfnt: &[u8]) -> u16 {
    let tables = read_tables(sfnt);
    let head = tables
        .iter()
        .find(|(tag, _)| tag == b"head")
        .map(|(_, d)| d.clone())
        .expect("subset must carry a head table");
    u16::from_be_bytes([head[18], head[19]])
}

// ---------------------------------------------------------------------------
// Face counting
// ---------------------------------------------------------------------------

/// A plain TTF reports exactly one face.
#[test]
fn face_count_of_plain_sfnt_is_one() {
    assert_eq!(
        oxifont_subset::face_count(TTF).expect("plain TTF must report a face count"),
        1
    );
}

/// The synthetic collection reports its declared `numFonts`.
#[test]
fn face_count_of_collection_is_num_fonts() {
    let ttc = build_two_face_ttc(TTF);
    assert_eq!(
        oxifont_subset::face_count(&ttc).expect("TTC must report a face count"),
        2
    );
}

// ---------------------------------------------------------------------------
// Face selection
// ---------------------------------------------------------------------------

/// `subset_font_at_face` selects the requested face out of the collection.
#[test]
fn subset_font_at_face_selects_the_requested_face() {
    let ttc = build_two_face_ttc(TTF);
    let codepoints: BTreeSet<char> = ['A', 'B', 'C'].into_iter().collect();

    let face0 = oxifont_subset::subset_font_at_face(&ttc, 0, &codepoints)
        .expect("face 0 of the collection must subset");
    let face1 = oxifont_subset::subset_font_at_face(&ttc, 1, &codepoints)
        .expect("face 1 of the collection must subset");

    assert_eq!(
        units_per_em(&face0),
        units_per_em(TTF),
        "face 0 must carry the original head"
    );
    assert_eq!(
        units_per_em(&face1),
        FACE1_UNITS_PER_EM,
        "face 1 must carry its own head, not face 0's"
    );
    assert_ne!(
        face0, face1,
        "the two faces must produce distinguishable subsets"
    );
}

/// Subsetting face 0 of a collection must match subsetting the standalone font
/// the collection was built from.
#[test]
fn subset_font_at_face_zero_matches_the_standalone_font() {
    let ttc = build_two_face_ttc(TTF);
    let codepoints: BTreeSet<char> = ['A', 'B', 'C'].into_iter().collect();

    let from_ttc = oxifont_subset::subset_font_at_face(&ttc, 0, &codepoints)
        .expect("face 0 of the collection must subset");
    let standalone =
        oxifont_subset::subset_font(TTF, &codepoints).expect("standalone font must subset");

    assert_eq!(
        from_ttc, standalone,
        "face 0 of a collection wrapping X must subset exactly like X"
    );
}

/// `subset_font_at_face(data, 0, …)` on a plain TTF is byte-identical to
/// `subset_font`.
#[test]
fn subset_font_at_face_zero_on_plain_sfnt_is_unchanged() {
    let codepoints: BTreeSet<char> = "Hello".chars().collect();
    let via_face = oxifont_subset::subset_font_at_face(TTF, 0, &codepoints)
        .expect("face 0 of a plain TTF must subset");
    let direct = oxifont_subset::subset_font(TTF, &codepoints).expect("plain TTF must subset");
    assert_eq!(via_face, direct, "face 0 must be the offset-0 behaviour");
}

/// The GID-set and options entry points take a face index too.
#[test]
fn gid_and_option_entry_points_take_a_face_index() {
    let ttc = build_two_face_ttc(TTF);

    let gids: BTreeSet<u16> = [36u16, 72].into_iter().collect();
    let by_gids = oxifont_subset::subset_by_gids_at_face(&ttc, 1, &gids)
        .expect("subset_by_gids_at_face must select face 1");
    assert_eq!(units_per_em(&by_gids), FACE1_UNITS_PER_EM);

    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    let opts = oxifont_subset::SubsetOptions::default().strip_hints(true);
    let (bytes, stats) =
        oxifont_subset::subset_font_with_options_at_face(&ttc, 1, &codepoints, &opts)
            .expect("subset_font_with_options_at_face must select face 1");
    assert_eq!(units_per_em(&bytes), FACE1_UNITS_PER_EM);
    assert!(stats.glyphs_retained >= 1);
    assert!(
        !stats.tables_retained.contains(b"fpgm"),
        "strip_hints must still apply on the face-index path"
    );
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// A face index at or past `numFonts` is a named error, never a panic.
#[test]
fn out_of_range_face_index_is_a_named_error() {
    let ttc = build_two_face_ttc(TTF);
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();

    match oxifont_subset::subset_font_at_face(&ttc, 2, &codepoints) {
        Err(SubsetError::FaceIndexOutOfRange { index, count }) => {
            assert_eq!(index, 2);
            assert_eq!(count, 2);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other:?}"),
    }
}

/// A plain (non-collection) font has exactly one face, so any index above 0 is
/// out of range rather than silently clamped.
#[test]
fn face_index_above_zero_on_plain_sfnt_is_out_of_range() {
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    match oxifont_subset::subset_font_at_face(TTF, 1, &codepoints) {
        Err(SubsetError::FaceIndexOutOfRange { index, count }) => {
            assert_eq!(index, 1);
            assert_eq!(count, 1);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other:?}"),
    }
}

/// The historical offset-0 entry points keep refusing a collection: a caller
/// must say which face it wants.
#[test]
fn legacy_entry_points_still_refuse_a_collection() {
    let ttc = build_two_face_ttc(TTF);
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();

    assert!(
        oxifont_subset::subset_font(&ttc, &codepoints).is_err(),
        "subset_font must not auto-select a face from a collection"
    );
    assert!(
        oxifont_subset::subset_by_gids(&ttc, &BTreeSet::new()).is_err(),
        "subset_by_gids must not auto-select a face from a collection"
    );
}

/// `numFonts = 0` is a malformed collection, not an empty one.
#[test]
fn zero_num_fonts_is_refused() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[8..12].copy_from_slice(&0u32.to_be_bytes());
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    assert!(
        oxifont_subset::subset_font_at_face(&ttc, 0, &codepoints).is_err(),
        "a collection declaring zero faces must be refused"
    );
    assert!(
        oxifont_subset::face_count(&ttc).is_err(),
        "face_count must refuse a zero-face collection"
    );
}

/// A `numFonts` far larger than the data can hold must be refused before any
/// allocation is sized from it.
#[test]
fn huge_num_fonts_is_refused() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[8..12].copy_from_slice(&0x7FFF_FFFFu32.to_be_bytes());
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    assert!(
        oxifont_subset::subset_font_at_face(&ttc, 0, &codepoints).is_err(),
        "a collection whose offset table cannot fit must be refused"
    );
    assert!(
        oxifont_subset::face_count(&ttc).is_err(),
        "face_count must refuse an unrepresentable numFonts"
    );
}

/// A face offset past the end of the file must be refused.
#[test]
fn face_offset_past_eof_is_refused() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[16..20].copy_from_slice(&0xFFFF_0000u32.to_be_bytes());
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    assert!(
        oxifont_subset::subset_font_at_face(&ttc, 1, &codepoints).is_err(),
        "a face offset past EOF must be refused"
    );
    // Face 0 is untouched and must still work.
    assert!(
        oxifont_subset::subset_font_at_face(&ttc, 0, &codepoints).is_ok(),
        "a broken face 1 must not poison face 0"
    );
}

/// A face offset that does not point at an SFNT header must be refused.
#[test]
fn face_offset_at_non_sfnt_is_refused() {
    let mut ttc = build_two_face_ttc(TTF);
    // Byte 8 is `numFonts` (0x00000002) — a valid offset inside the file, but
    // not a recognised per-face SFNT magic.
    ttc[16..20].copy_from_slice(&8u32.to_be_bytes());
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    assert!(
        oxifont_subset::subset_font_at_face(&ttc, 1, &codepoints).is_err(),
        "a face offset pointing at non-SFNT bytes must be refused"
    );
}

/// A truncated `ttcf` header (shorter than the fixed 12 bytes) must be refused.
#[test]
fn truncated_collection_header_is_refused() {
    let stub = b"ttcf\x00\x01\x00\x00";
    assert!(
        oxifont_subset::face_count(stub).is_err(),
        "a truncated collection header must be refused"
    );
    let codepoints: BTreeSet<char> = ['A'].into_iter().collect();
    assert!(
        oxifont_subset::subset_font_at_face(stub, 0, &codepoints).is_err(),
        "a truncated collection header must be refused"
    );
}

/// An unknown collection version must be refused rather than trusted.
#[test]
fn unknown_collection_version_is_refused() {
    let mut ttc = build_two_face_ttc(TTF);
    ttc[4..6].copy_from_slice(&9u16.to_be_bytes()); // majorVersion = 9
    assert!(
        oxifont_subset::face_count(&ttc).is_err(),
        "an unknown ttcf major version must be refused"
    );
}

// ---------------------------------------------------------------------------
// Real Windows collection
// ---------------------------------------------------------------------------

/// Subset a genuine Windows CJK collection when one is present.
///
/// Follows the repo's system-font test convention: the test returns early
/// (rather than failing) when no such font is installed, so it is safe on CI
/// machines without Windows fonts.
#[cfg(windows)]
#[test]
fn subset_real_windows_collection() {
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        eprintln!("%SystemRoot% not set — skipping subset_real_windows_collection");
        return;
    };
    let fonts_dir = std::path::PathBuf::from(system_root).join("Fonts");

    let candidates = ["msgothic.ttc", "meiryo.ttc", "YuGothM.ttc", "msmincho.ttc"];
    let Some(path) = candidates
        .iter()
        .map(|name| fonts_dir.join(name))
        .find(|p| p.is_file())
    else {
        eprintln!("no stock Windows CJK collection found — skipping");
        return;
    };

    let Ok(data) = std::fs::read(&path) else {
        eprintln!("could not read {path:?} — skipping");
        return;
    };

    let count = oxifont_subset::face_count(&data)
        .unwrap_or_else(|e| panic!("face_count failed for {path:?}: {e}"));
    assert!(count >= 1, "{path:?} must declare at least one face");
    eprintln!("{path:?}: {count} faces");

    // Latin plus a couple of Japanese codepoints that every stock CJK face has.
    let codepoints: BTreeSet<char> = "AB\u{3042}\u{6F22}".chars().collect();

    for face_index in 0..count {
        let subset = oxifont_subset::subset_font_at_face(&data, face_index, &codepoints)
            .unwrap_or_else(|e| panic!("subsetting {path:?} face {face_index} failed: {e}"));
        assert!(
            subset.len() < data.len(),
            "subset of {path:?} face {face_index} should be smaller than the whole collection"
        );
        let face = ttf_parser::Face::parse(&subset, 0).unwrap_or_else(|e| {
            panic!("subset of {path:?} face {face_index} did not re-parse: {e:?}")
        });
        assert!(
            face.number_of_glyphs() > 1,
            "subset of {path:?} face {face_index} must retain more than .notdef"
        );
    }

    // Out-of-range selection on a real collection is still a named error.
    match oxifont_subset::subset_font_at_face(&data, count, &codepoints) {
        Err(SubsetError::FaceIndexOutOfRange { index, .. }) => assert_eq!(index, count),
        other => panic!("expected FaceIndexOutOfRange for face {count}, got {other:?}"),
    }
}
