//! Live differential tests for [`oxifont_subset::instance`], against the
//! variable fonts Windows ships.
//!
//! Every test here is `#[ignore]`d: it reads `%SystemRoot%\Fonts`, which is not
//! something a CI image can be assumed to have. Each one also skips gracefully
//! when its font is absent, so `--run-ignored ignored-only` is meaningful on a
//! machine that has only some of them.
//!
//! The advance constants are a pinned oracle, generated offline from
//! `subsetter` 0.2.6's `subset_with_variations` output read back through
//! `ttf_parser::Face::glyph_hor_advance` — the exact call a PDF `/W` array is
//! built from. They are asserted **exactly**; a ±1 tolerance would hide the
//! rounding-rule regressions this file exists to catch.

use std::path::PathBuf;

use ttf_parser::{Face, GlyphId, OutlineBuilder, Tag};

/// Read a font out of the system font directory, or `None` when it is absent.
fn system_font(name: &str) -> Option<Vec<u8>> {
    let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"))?;
    let path = PathBuf::from(root).join("Fonts").join(name);
    std::fs::read(path).ok()
}

macro_rules! font_or_skip {
    ($name:expr) => {
        match system_font($name) {
            Some(data) => data,
            None => {
                eprintln!("skipping: {} is not installed", $name);
                return;
            }
        }
    };
}

/// Collects every emitted control point, giving the control box — the same box
/// `glyf` stores and the instancer recomputes.
#[derive(Default)]
struct BoxBuilder {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    any: bool,
    points: Vec<(f32, f32)>,
}

impl BoxBuilder {
    fn push(&mut self, x: f32, y: f32) {
        if self.any {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        } else {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
            self.any = true;
        }
        self.points.push((x, y));
    }
}

impl OutlineBuilder for BoxBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.push(x1, y1);
        self.push(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.push(x1, y1);
        self.push(x2, y2);
        self.push(x, y);
    }
    fn close(&mut self) {}
}

fn outline_of(face: &Face<'_>, gid: GlyphId) -> Option<BoxBuilder> {
    let mut b = BoxBuilder::default();
    face.outline_glyph(gid, &mut b)?;
    Some(b)
}

/// A `(tag, user value)` pair, the shape [`oxifont_subset::instance`] takes.
type AxisValue = ([u8; 4], f32);

fn varied_face<'a>(data: &'a [u8], coords: &[AxisValue]) -> Face<'a> {
    let mut face = Face::parse(data, 0).expect("source face parses");
    for (tag, value) in coords {
        face.set_variation(Tag::from_bytes(tag), *value);
    }
    face
}

/// Assert the pinned advance of every listed character, exactly.
fn assert_advances(name: &str, coords: &[([u8; 4], f32)], oracle: &[(char, u16)]) {
    let data = match system_font(name) {
        Some(d) => d,
        None => {
            eprintln!("skipping: {name} is not installed");
            return;
        }
    };
    let bytes = oxifont_subset::instance(&data, 0, coords).expect("instance");
    let face = Face::parse(&bytes, 0).expect("instanced face parses");
    for &(ch, expected) in oracle {
        let gid = face
            .glyph_index(ch)
            .unwrap_or_else(|| panic!("{name}: no glyph for {ch:?}"));
        let advance = face
            .glyph_hor_advance(gid)
            .unwrap_or_else(|| panic!("{name}: no advance for {ch:?}"));
        assert_eq!(
            advance, expected,
            "{name} {coords:?}: advance for {ch:?} (gid {}) was {advance}, oracle {expected}",
            gid.0
        );
    }
}

// ---------------------------------------------------------------------------
// Oracle tables (see the module doc)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSansJP-VF.ttf"]
fn live_noto_sans_jp_at_weight_400() {
    assert_advances(
        "NotoSansJP-VF.ttf",
        &[(*b"wght", 400.0)],
        &[
            (' ', 224),
            ('(', 338),
            (')', 338),
            ('0', 555),
            ('2', 555),
            ('5', 555),
            ('6', 555),
            ('A', 608),
            ('M', 812),
            ('T', 599),
            ('V', 575),
            ('a', 563),
            ('f', 325),
            ('i', 275),
            ('k', 552),
            ('m', 926),
            ('o', 606),
            ('p', 620),
            ('y', 521),
        ],
    );
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSansJP-VF.ttf"]
fn live_noto_sans_jp_at_weight_700() {
    assert_advances(
        "NotoSansJP-VF.ttf",
        &[(*b"wght", 700.0)],
        &[
            (' ', 227),
            ('(', 378),
            ('0', 590),
            ('A', 641),
            ('M', 853),
            ('T', 625),
            ('V', 619),
            ('a', 591),
            ('f', 372),
            ('i', 304),
            ('k', 604),
            ('m', 964),
            ('o', 626),
            ('p', 644),
            ('y', 574),
        ],
    );
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_segoe_ui_variable_at_weight_700_optical_size_10_5() {
    assert_advances(
        "SegUIVar.ttf",
        &[(*b"wght", 700.0), (*b"opsz", 10.5)],
        &[
            (' ', 565),
            ('(', 756),
            ('0', 1178),
            ('A', 1440),
            ('M', 1960),
            ('T', 1200),
            ('V', 1366),
            ('a', 1102),
            ('f', 785),
            ('i', 582),
            ('k', 1145),
            ('m', 1876),
            ('o', 1252),
            ('p', 1270),
            ('y', 1102),
        ],
    );
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SitkaVF.ttf"]
fn live_sitka_variable_at_optical_size_11_weight_700() {
    assert_advances(
        "SitkaVF.ttf",
        &[(*b"opsz", 11.0), (*b"wght", 700.0)],
        &[
            (' ', 641),
            ('(', 1040),
            ('0', 1449),
            ('A', 1522),
            ('M', 2140),
            ('T', 1483),
            ('V', 1537),
            ('a', 1286),
            ('f', 911),
            ('i', 803),
            ('k', 1356),
            ('m', 2110),
            ('o', 1327),
            ('p', 1403),
            ('y', 1290),
        ],
    );
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_segoe_ui_variable_weight_sweep() {
    for (wght, expected) in [(300.0f32, 1288u16), (400.0, 1321), (700.0, 1440)] {
        assert_advances("SegUIVar.ttf", &[(*b"wght", wght)], &[('A', expected)]);
    }
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSansJP-VF.ttf"]
fn live_noto_sans_jp_weight_sweep() {
    for (wght, expected) in [(100.0f32, 574u16), (400.0, 608), (900.0, 660)] {
        assert_advances("NotoSansJP-VF.ttf", &[(*b"wght", wght)], &[('A', expected)]);
    }
}

/// bahnschrift's advances are DIN-derived and weight-invariant, so an
/// advance-only assertion on it passes vacuously. The width axis does move
/// them, and the outlines move under both axes — hence the outline check below.
#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_bahnschrift_width_sweep() {
    assert_advances("bahnschrift.ttf", &[(*b"wdth", 75.0)], &[('A', 906)]);
    assert_advances("bahnschrift.ttf", &[(*b"wdth", 100.0)], &[('A', 1326)]);
}

// ---------------------------------------------------------------------------
// Outline / bounding-box differential
// ---------------------------------------------------------------------------

/// Compare the instanced face's outlines against `ttf_parser`'s live evaluation
/// of the same location on the original bytes.
///
/// Tolerance is ±1 design unit on every control point and every box edge:
/// `glyf` stores `int16`, so a composite's base glyph must be rounded *before*
/// its transform is applied, whereas a live renderer keeps the sub-unit deltas
/// and rounds after. That residue is inherent to baking, is bounded by one
/// unit, and is exactly what fontTools' instancer produces too.
fn assert_outlines_match(name: &str, coords: &[([u8; 4], f32)], chars: &str) {
    let data = match system_font(name) {
        Some(d) => d,
        None => {
            eprintln!("skipping: {name} is not installed");
            return;
        }
    };
    let bytes = oxifont_subset::instance(&data, 0, coords).expect("instance");
    let baked = Face::parse(&bytes, 0).expect("instanced face parses");
    let live = varied_face(&data, coords);

    let mut compared = 0usize;
    for ch in chars.chars() {
        let Some(gid) = live.glyph_index(ch) else {
            continue;
        };
        let (Some(a), Some(b)) = (outline_of(&baked, gid), outline_of(&live, gid)) else {
            continue;
        };
        assert_eq!(
            a.points.len(),
            b.points.len(),
            "{name} {ch:?}: point count {} vs {}",
            a.points.len(),
            b.points.len()
        );
        for (i, (p, q)) in a.points.iter().zip(b.points.iter()).enumerate() {
            assert!(
                (p.0 - q.0).abs() <= 1.0 && (p.1 - q.1).abs() <= 1.0,
                "{name} {ch:?}: point {i} baked {p:?} vs live {q:?}"
            );
        }
        for (baked_edge, live_edge) in [
            (a.min_x, b.min_x),
            (a.min_y, b.min_y),
            (a.max_x, b.max_x),
            (a.max_y, b.max_y),
        ] {
            assert!(
                (baked_edge - live_edge).abs() <= 1.0,
                "{name} {ch:?}: box edge {baked_edge} vs {live_edge}"
            );
        }
        compared += 1;
    }
    assert!(compared > 0, "{name}: no glyphs were compared");
}

const SAMPLE: &str = "ABCMTVWafikmopy0256()";

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSansJP-VF.ttf"]
fn live_noto_sans_jp_outlines_match_the_live_renderer() {
    assert_outlines_match("NotoSansJP-VF.ttf", &[(*b"wght", 700.0)], SAMPLE);
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_segoe_ui_variable_outlines_match_the_live_renderer() {
    assert_outlines_match(
        "SegUIVar.ttf",
        &[(*b"wght", 700.0), (*b"opsz", 10.5)],
        SAMPLE,
    );
}

/// bahnschrift is the composite-heavy case: 448 composites, 19 of them with a
/// rotated 2×2, and 79–86 % of its component offsets vary with the axes.
#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_bahnschrift_outlines_match_the_live_renderer() {
    assert_outlines_match(
        "bahnschrift.ttf",
        &[(*b"wght", 700.0), (*b"wdth", 75.0)],
        "ABCÀÁÂÃÄÅÇÈÉÊËÌÍÎÏÑÒÓÔÕÖabcàáâãäåçèéêëìíîïñòóôõöy",
    );
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SitkaVF.ttf"]
fn live_sitka_outlines_match_the_live_renderer() {
    assert_outlines_match(
        "SitkaVF.ttf",
        &[(*b"opsz", 11.0), (*b"wght", 700.0)],
        SAMPLE,
    );
}

/// Every glyph of every installed variable face, at a location that is *not* an
/// axis endpoint, where the baked-vs-live rounding residue is largest.
///
/// Simple glyphs must land within 0.6 units: the instancer emits
/// `ot_round(live)`, which bounds the residue at 0.5, plus up to ~0.02 of slack
/// because `ttf_parser` normalises user coordinates through its own fixed-point
/// path and can land one F2Dot14 unit away. Measured worst case 0.518 — anything
/// beyond that is a coordinate-pipeline disagreement, not rounding.
///
/// Composites get two units, because their base glyph is rounded before the
/// component transform and the component offset is rounded again; measured worst
/// case across all six installed faces is 1.23.
#[test]
#[ignore = "reads %SystemRoot%/Fonts"]
fn live_interior_locations_stay_within_the_rounding_residue() {
    let cases: [(&str, &[AxisValue]); 5] = [
        ("bahnschrift.ttf", &[(*b"wght", 550.0), (*b"wdth", 88.0)]),
        ("SegUIVar.ttf", &[(*b"wght", 553.0), (*b"opsz", 13.7)]),
        ("NotoSansJP-VF.ttf", &[(*b"wght", 537.0)]),
        ("SitkaVF.ttf", &[(*b"opsz", 9.3), (*b"wght", 462.0)]),
        ("CascadiaCode.ttf", &[(*b"wght", 512.0)]),
    ];
    let mut faces_checked = 0usize;
    for (name, coords) in cases {
        let Some(data) = system_font(name) else {
            eprintln!("skipping: {name} is not installed");
            continue;
        };
        let bytes = oxifont_subset::instance(&data, 0, coords).expect("instance");
        let baked = Face::parse(&bytes, 0).expect("instanced face parses");
        let live = varied_face(&data, coords);
        let source = Face::parse(&data, 0).expect("source face parses");

        for gid in 0..source.number_of_glyphs() {
            let gid = GlyphId(gid);
            let (Some(a), Some(b)) = (outline_of(&baked, gid), outline_of(&live, gid)) else {
                continue;
            };
            if a.points.len() != b.points.len() {
                panic!("{name}: glyph {} changed its point count", gid.0);
            }
            let tolerance = if is_composite(&data, gid.0) { 2.0 } else { 0.6 };
            for (p, q) in a.points.iter().zip(b.points.iter()) {
                assert!(
                    (p.0 - q.0).abs() <= tolerance && (p.1 - q.1).abs() <= tolerance,
                    "{name}: glyph {} baked {p:?} vs live {q:?} (tolerance {tolerance})",
                    gid.0
                );
            }
        }
        faces_checked += 1;
    }
    assert!(faces_checked > 0, "no variable faces were available");
}

/// Whether glyph `gid` is a composite in the source face.
fn is_composite(data: &[u8], gid: u16) -> bool {
    let Ok(raw) = ttf_parser::RawFace::parse(data, 0) else {
        return false;
    };
    let (Some(glyf), Some(loca), Some(head)) = (
        raw.table(Tag::from_bytes(b"glyf")),
        raw.table(Tag::from_bytes(b"loca")),
        raw.table(Tag::from_bytes(b"head")),
    ) else {
        return false;
    };
    let long = i16::from_be_bytes([head[50], head[51]]) == 1;
    let i = usize::from(gid);
    let (start, end) = if long {
        let Some(s) = loca.get(i * 4..i * 4 + 8) else {
            return false;
        };
        (
            u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize,
            u32::from_be_bytes([s[4], s[5], s[6], s[7]]) as usize,
        )
    } else {
        let Some(s) = loca.get(i * 2..i * 2 + 4) else {
            return false;
        };
        (
            u16::from_be_bytes([s[0], s[1]]) as usize * 2,
            u16::from_be_bytes([s[2], s[3]]) as usize * 2,
        )
    };
    if start >= end {
        return false;
    }
    glyf.get(start..start + 2)
        .is_some_and(|g| i16::from_be_bytes([g[0], g[1]]) < 0)
}

// ---------------------------------------------------------------------------
// Whole-face invariants
// ---------------------------------------------------------------------------

/// Instancing at the `fvar` default must leave every outline point untouched.
/// This is the cheapest regression net there is: it fails on any error in the
/// coordinate pipeline, the tuple walk, the delta application, or the glyph
/// re-serialization, without needing a single golden number.
fn assert_default_location_identity(name: &str) {
    let data = match system_font(name) {
        Some(d) => d,
        None => {
            eprintln!("skipping: {name} is not installed");
            return;
        }
    };
    let source = Face::parse(&data, 0).expect("source face parses");
    let defaults: Vec<([u8; 4], f32)> = source
        .variation_axes()
        .into_iter()
        .map(|a| (a.tag.to_bytes(), a.def_value))
        .collect();
    assert!(!defaults.is_empty(), "{name} is not a variable font");

    let bytes = oxifont_subset::instance(&data, 0, &defaults).expect("instance");
    let baked = Face::parse(&bytes, 0).expect("instanced face parses");
    assert_eq!(baked.number_of_glyphs(), source.number_of_glyphs());

    for gid in 0..source.number_of_glyphs() {
        let gid = GlyphId(gid);
        let (a, b) = (outline_of(&baked, gid), outline_of(&source, gid));
        match (a, b) {
            (Some(a), Some(b)) => {
                assert_eq!(a.points, b.points, "{name}: glyph {} moved", gid.0);
            }
            (None, None) => {}
            _ => panic!("{name}: glyph {} gained or lost an outline", gid.0),
        }
        assert_eq!(
            baked.glyph_hor_advance(gid),
            source.glyph_hor_advance(gid),
            "{name}: glyph {} advance moved",
            gid.0
        );
    }
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSansJP-VF.ttf"]
fn live_noto_sans_jp_default_location_is_identity() {
    assert_default_location_identity("NotoSansJP-VF.ttf");
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/NotoSerifJP-VF.ttf"]
fn live_noto_serif_jp_default_location_is_identity() {
    assert_default_location_identity("NotoSerifJP-VF.ttf");
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_segoe_ui_variable_default_location_is_identity() {
    assert_default_location_identity("SegUIVar.ttf");
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_bahnschrift_default_location_is_identity() {
    assert_default_location_identity("bahnschrift.ttf");
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_instanced_face_carries_no_variation_tables() {
    let data = font_or_skip!("SegUIVar.ttf");
    let bytes = oxifont_subset::instance(&data, 0, &[(*b"wght", 700.0)]).expect("instance");
    let face = Face::parse(&bytes, 0).expect("instanced face parses");
    assert!(!face.is_variable(), "instanced face still advertises axes");
    assert!(face.variation_axes().is_empty());

    let raw = ttf_parser::RawFace::parse(&bytes, 0).expect("raw face");
    for tag in [
        b"fvar", b"gvar", b"avar", b"cvar", b"HVAR", b"VVAR", b"MVAR", b"STAT", b"DSIG", b"VORG",
        b"cvt ", b"fpgm", b"prep", b"gasp",
    ] {
        assert!(
            raw.table(Tag::from_bytes(tag)).is_none(),
            "{} survived instancing",
            std::str::from_utf8(tag).unwrap_or("????")
        );
    }
    // Glyph identity is the whole architectural payoff: assert it.
    let source = Face::parse(&data, 0).expect("source");
    assert_eq!(face.number_of_glyphs(), source.number_of_glyphs());
}

#[test]
#[ignore = "reads %SystemRoot%/Fonts/SegUIVar.ttf"]
fn live_instancing_is_byte_deterministic() {
    let data = font_or_skip!("SegUIVar.ttf");
    let coords = [(*b"wght", 640.0), (*b"opsz", 13.25)];
    let first = oxifont_subset::instance(&data, 0, &coords).expect("instance");
    for _ in 0..2 {
        let again = oxifont_subset::instance(&data, 0, &coords).expect("instance");
        assert_eq!(first, again, "instancing is not byte-deterministic");
    }
}

/// Instructions and their supporting tables must leave together: a glyph
/// program that outlives `cvt `/`fpgm`/`prep` grid-fits against definitions that
/// no longer exist.
#[test]
#[ignore = "reads %SystemRoot%/Fonts/bahnschrift.ttf"]
fn live_instanced_glyphs_carry_no_instructions() {
    let data = font_or_skip!("bahnschrift.ttf");
    let bytes = oxifont_subset::instance(&data, 0, &[(*b"wght", 700.0)]).expect("instance");
    let raw = ttf_parser::RawFace::parse(&bytes, 0).expect("raw face");
    let glyf = raw.table(Tag::from_bytes(b"glyf")).expect("glyf");
    let loca = raw.table(Tag::from_bytes(b"loca")).expect("loca");
    let head = raw.table(Tag::from_bytes(b"head")).expect("head");
    let long_loca = i16::from_be_bytes([head[50], head[51]]) == 1;
    let face = Face::parse(&bytes, 0).expect("face");

    let mut checked = 0usize;
    for gid in 0..face.number_of_glyphs() {
        let i = gid as usize;
        let (start, end) = if long_loca {
            (
                u32::from_be_bytes([
                    loca[i * 4],
                    loca[i * 4 + 1],
                    loca[i * 4 + 2],
                    loca[i * 4 + 3],
                ]) as usize,
                u32::from_be_bytes([
                    loca[i * 4 + 4],
                    loca[i * 4 + 5],
                    loca[i * 4 + 6],
                    loca[i * 4 + 7],
                ]) as usize,
            )
        } else {
            (
                u16::from_be_bytes([loca[i * 2], loca[i * 2 + 1]]) as usize * 2,
                u16::from_be_bytes([loca[i * 2 + 2], loca[i * 2 + 3]]) as usize * 2,
            )
        };
        if start >= end {
            continue;
        }
        let glyph = &glyf[start..end];
        let n_contours = i16::from_be_bytes([glyph[0], glyph[1]]);
        if n_contours > 0 {
            let ends = 10 + n_contours as usize * 2;
            let instr_len = u16::from_be_bytes([glyph[ends], glyph[ends + 1]]);
            assert_eq!(
                instr_len, 0,
                "glyph {gid} kept {instr_len} instruction bytes"
            );
            checked += 1;
        } else {
            let flags = u16::from_be_bytes([glyph[10], glyph[11]]);
            assert_eq!(
                flags & 0x0100,
                0,
                "composite {gid} kept WE_HAVE_INSTRUCTIONS"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 100,
        "expected to check many glyphs, saw {checked}"
    );

    let maxp = raw.table(Tag::from_bytes(b"maxp")).expect("maxp");
    assert_eq!(
        u16::from_be_bytes([maxp[26], maxp[27]]),
        0,
        "maxSizeOfInstructions was not zeroed"
    );
}
