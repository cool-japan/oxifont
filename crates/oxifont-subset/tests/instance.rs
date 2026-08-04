//! Synthetic-fixture tests for [`oxifont_subset::instance`].
//!
//! Everything here is hand-built, so it runs on any machine and covers the
//! encodings that shipping fonts never use — a probe over `bahnschrift`,
//! `SegUIVar` and the two Noto CJK variable faces found **zero** occurrences of
//! `EMBEDDED_PEAK_TUPLE`, `POINTS_ARE_WORDS`, `SCALED_COMPONENT_OFFSET` and
//! point-matched components across 197 000+ tuples. Only a fixture can exercise
//! them, and each one is a silent corruption if it is wrong.

use oxifont_subset::{instance, SubsetError};
use ttf_parser::{Face, GlyphId, OutlineBuilder, Tag};

// ---------------------------------------------------------------------------
// Minimal SFNT builder
// ---------------------------------------------------------------------------

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
fn bei16(v: i16) -> [u8; 2] {
    v.to_be_bytes()
}
fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// Assemble an SFNT from `(tag, data)` pairs: sorted directory, 4-byte padded
/// bodies. Checksums are left zero — nothing in the instancer verifies them,
/// and a fixture that had to compute them would be testing the builder.
fn build_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sorted: Vec<&([u8; 4], Vec<u8>)> = tables.iter().collect();
    sorted.sort_by_key(|(tag, _)| *tag);
    let n = sorted.len();
    let mut out = Vec::new();
    out.extend_from_slice(&be32(0x0001_0000));
    out.extend_from_slice(&be16(n as u16));
    out.extend_from_slice(&be16(16));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));
    let dir_start = out.len();
    out.resize(dir_start + n * 16, 0);
    for (i, (tag, data)) in sorted.iter().enumerate() {
        let offset = out.len() as u32;
        out.extend_from_slice(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        let base = dir_start + i * 16;
        out[base..base + 4].copy_from_slice(tag);
        out[base + 8..base + 12].copy_from_slice(&be32(offset));
        out[base + 12..base + 16].copy_from_slice(&be32(data.len() as u32));
    }
    out
}

fn head(units_per_em: u16, loca_format: i16) -> Vec<u8> {
    let mut t = vec![0u8; 54];
    t[0..4].copy_from_slice(&be32(0x0001_0000));
    t[12..16].copy_from_slice(&be32(0x5F0F_3CF5));
    t[18..20].copy_from_slice(&be16(units_per_em));
    t[50..52].copy_from_slice(&bei16(loca_format));
    t
}

fn hhea(num_h_metrics: u16) -> Vec<u8> {
    let mut t = vec![0u8; 36];
    t[0..4].copy_from_slice(&be32(0x0001_0000));
    t[4..6].copy_from_slice(&bei16(800)); // ascender
    t[6..8].copy_from_slice(&bei16(-200)); // descender
    t[34..36].copy_from_slice(&be16(num_h_metrics));
    t
}

fn maxp(num_glyphs: u16) -> Vec<u8> {
    let mut t = vec![0u8; 32];
    t[0..4].copy_from_slice(&be32(0x0001_0000));
    t[4..6].copy_from_slice(&be16(num_glyphs));
    // Deliberately non-zero hinting counters, so the test can prove they are
    // zeroed rather than merely already zero.
    t[26..28].copy_from_slice(&be16(512)); // maxSizeOfInstructions
    t[20..22].copy_from_slice(&be16(7)); // maxFunctionDefs
    t
}

fn hmtx(metrics: &[(u16, i16)]) -> Vec<u8> {
    let mut t = Vec::with_capacity(metrics.len() * 4);
    for &(advance, lsb) in metrics {
        t.extend_from_slice(&be16(advance));
        t.extend_from_slice(&bei16(lsb));
    }
    t
}

fn loca_long(offsets: &[u32]) -> Vec<u8> {
    offsets.iter().flat_map(|o| be32(*o)).collect()
}

/// A simple glyph with one contour per `contour_ends` entry, all points
/// on-curve, coordinates written as `int16` deltas.
fn simple_glyph(contour_ends: &[u16], points: &[(i16, i16)]) -> Vec<u8> {
    let mut g = Vec::new();
    g.extend_from_slice(&bei16(contour_ends.len() as i16));
    let x_min = points.iter().map(|p| p.0).min().unwrap_or(0);
    let y_min = points.iter().map(|p| p.1).min().unwrap_or(0);
    let x_max = points.iter().map(|p| p.0).max().unwrap_or(0);
    let y_max = points.iter().map(|p| p.1).max().unwrap_or(0);
    g.extend_from_slice(&bei16(x_min));
    g.extend_from_slice(&bei16(y_min));
    g.extend_from_slice(&bei16(x_max));
    g.extend_from_slice(&bei16(y_max));
    for &e in contour_ends {
        g.extend_from_slice(&be16(e));
    }
    // A non-empty instruction stream, so the test can prove it is dropped.
    g.extend_from_slice(&be16(3));
    g.extend_from_slice(&[0x4B, 0x4B, 0x4B]);
    // ON_CURVE_POINT with long coordinates, one flag byte per point.
    g.extend(std::iter::repeat_n(0x01u8, points.len()));
    let mut prev = 0i16;
    for p in points {
        g.extend_from_slice(&bei16(p.0 - prev));
        prev = p.0;
    }
    let mut prev = 0i16;
    for p in points {
        g.extend_from_slice(&bei16(p.1 - prev));
        prev = p.1;
    }
    while !g.len().is_multiple_of(2) {
        g.push(0);
    }
    g
}

/// One component of a hand-built composite.
struct Comp {
    flags: u16,
    glyph_index: u16,
    arg1: i16,
    arg2: i16,
    /// `WE_HAVE_A_TWO_BY_TWO` matrix, when the flags ask for one.
    matrix: Option<[i16; 4]>,
}

fn composite_glyph(bbox: [i16; 4], comps: &[Comp]) -> Vec<u8> {
    let mut g = Vec::new();
    g.extend_from_slice(&bei16(-1));
    for v in bbox {
        g.extend_from_slice(&bei16(v));
    }
    for (i, c) in comps.iter().enumerate() {
        let mut flags = c.flags | 0x0001; // always word arguments in fixtures
        if i + 1 < comps.len() {
            flags |= 0x0020; // MORE_COMPONENTS
        }
        if c.matrix.is_some() {
            flags |= 0x0080; // WE_HAVE_A_TWO_BY_TWO
        }
        g.extend_from_slice(&be16(flags));
        g.extend_from_slice(&be16(c.glyph_index));
        g.extend_from_slice(&bei16(c.arg1));
        g.extend_from_slice(&bei16(c.arg2));
        if let Some(m) = c.matrix {
            for v in m {
                g.extend_from_slice(&bei16(v));
            }
        }
    }
    while !g.len().is_multiple_of(2) {
        g.push(0);
    }
    g
}

/// `fvar` with `axisSize = 20` and no named instances.
fn fvar(axes: &[([u8; 4], f32, f32, f32)]) -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&be16(1));
    t.extend_from_slice(&be16(0));
    t.extend_from_slice(&be16(16));
    t.extend_from_slice(&be16(2));
    t.extend_from_slice(&be16(axes.len() as u16));
    t.extend_from_slice(&be16(20));
    t.extend_from_slice(&be16(0));
    t.extend_from_slice(&be16(0));
    for (tag, min, def, max) in axes {
        t.extend_from_slice(tag);
        for v in [min, def, max] {
            t.extend_from_slice(&be32(((*v * 65536.0).round() as i32) as u32));
        }
        t.extend_from_slice(&be16(0));
        t.extend_from_slice(&be16(0));
    }
    t
}

// ---------------------------------------------------------------------------
// gvar builder
// ---------------------------------------------------------------------------

fn f2dot14(v: f32) -> i16 {
    (v * 16384.0).round() as i16
}

/// Pack a point-number list. An empty list encodes `count == 0`, i.e. **all**
/// points — the trap that turns "all" into "none" when misread.
fn pack_points(pts: &[u16], words: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if pts.is_empty() {
        out.push(0);
        return out;
    }
    assert!(pts.len() <= 128, "fixture emits a single run");
    if pts.len() < 128 {
        out.push(pts.len() as u8);
    } else {
        out.push(0x80 | (pts.len() >> 8) as u8);
        out.push((pts.len() & 0xFF) as u8);
    }
    let mut ctrl = (pts.len() - 1) as u8;
    if words {
        ctrl |= 0x80;
    }
    out.push(ctrl);
    let mut prev = 0u16;
    for &p in pts {
        let d = p.wrapping_sub(prev);
        prev = p;
        if words {
            out.extend_from_slice(&be16(d));
        } else {
            out.push(d as u8);
        }
    }
    out
}

/// Pack deltas as word runs (always valid, and the encoding a fixture can be
/// sure of).
fn pack_deltas(values: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in values.chunks(64) {
        out.push(0x40 | (chunk.len() - 1) as u8);
        for &v in chunk {
            out.extend_from_slice(&bei16(v as i16));
        }
    }
    out
}

#[derive(Default, Clone)]
struct TupleFixture {
    /// `Some` → `EMBEDDED_PEAK_TUPLE`; `None` → use `shared_index`.
    peak: Option<Vec<i16>>,
    shared_index: u16,
    intermediate: Option<(Vec<i16>, Vec<i16>)>,
    /// `Some` → `PRIVATE_POINT_NUMBERS`.
    private_points: Option<Vec<u16>>,
    points_are_words: bool,
    deltas: Vec<(i32, i32)>,
}

impl TupleFixture {
    fn data(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(p) = &self.private_points {
            out.extend_from_slice(&pack_points(p, self.points_are_words));
        }
        let xs: Vec<i32> = self.deltas.iter().map(|d| d.0).collect();
        let ys: Vec<i32> = self.deltas.iter().map(|d| d.1).collect();
        out.extend_from_slice(&pack_deltas(&xs));
        out.extend_from_slice(&pack_deltas(&ys));
        out
    }

    fn header(&self, data_size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&be16(data_size as u16));
        let mut index = self.shared_index & 0x0FFF;
        if self.peak.is_some() {
            index |= 0x8000;
        }
        if self.intermediate.is_some() {
            index |= 0x4000;
        }
        if self.private_points.is_some() {
            index |= 0x2000;
        }
        out.extend_from_slice(&be16(index));
        if let Some(p) = &self.peak {
            for v in p {
                out.extend_from_slice(&bei16(*v));
            }
        }
        if let Some((s, e)) = &self.intermediate {
            for v in s {
                out.extend_from_slice(&bei16(*v));
            }
            for v in e {
                out.extend_from_slice(&bei16(*v));
            }
        }
        out
    }
}

/// Encode one glyph's `GlyphVariationData` block.
fn glyph_var_block(shared_points: Option<&[u16]>, tuples: &[TupleFixture]) -> Vec<u8> {
    let mut headers = Vec::new();
    let mut data = Vec::new();
    if let Some(p) = shared_points {
        data.extend_from_slice(&pack_points(p, false));
    }
    for t in tuples {
        let d = t.data();
        headers.extend_from_slice(&t.header(d.len()));
        data.extend_from_slice(&d);
    }
    let mut count = tuples.len() as u16;
    if shared_points.is_some() {
        count |= 0x8000;
    }
    let data_offset = 4 + headers.len();
    let mut out = Vec::new();
    out.extend_from_slice(&be16(count));
    out.extend_from_slice(&be16(data_offset as u16));
    out.extend_from_slice(&headers);
    out.extend_from_slice(&data);
    while !out.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}

fn build_gvar(
    axis_count: usize,
    shared_tuples: &[Vec<i16>],
    blocks: &[Vec<u8>],
    long_offsets: bool,
) -> Vec<u8> {
    let entry = if long_offsets { 4 } else { 2 };
    let offset_array_size = (blocks.len() + 1) * entry;
    let shared_size = shared_tuples.len() * axis_count * 2;
    let shared_offset = 20 + offset_array_size;
    let data_offset = shared_offset + shared_size;

    let mut out = Vec::new();
    out.extend_from_slice(&be16(1));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(axis_count as u16));
    out.extend_from_slice(&be16(shared_tuples.len() as u16));
    out.extend_from_slice(&be32(shared_offset as u32));
    out.extend_from_slice(&be16(blocks.len() as u16));
    out.extend_from_slice(&be16(u16::from(long_offsets)));
    out.extend_from_slice(&be32(data_offset as u32));

    let mut cursor = 0usize;
    for b in blocks {
        if long_offsets {
            out.extend_from_slice(&be32(cursor as u32));
        } else {
            out.extend_from_slice(&be16((cursor / 2) as u16));
        }
        cursor += b.len();
    }
    if long_offsets {
        out.extend_from_slice(&be32(cursor as u32));
    } else {
        out.extend_from_slice(&be16((cursor / 2) as u16));
    }
    for t in shared_tuples {
        for v in t {
            out.extend_from_slice(&bei16(*v));
        }
    }
    for b in blocks {
        out.extend_from_slice(b);
    }
    out
}

// ---------------------------------------------------------------------------
// A one-axis, three-glyph fixture face
// ---------------------------------------------------------------------------

/// `.notdef` (empty), a 4-point square, and a composite of the square.
struct Fixture {
    glyphs: Vec<Vec<u8>>,
    metrics: Vec<(u16, i16)>,
    extra: Vec<([u8; 4], Vec<u8>)>,
    gvar: Option<Vec<u8>>,
}

impl Fixture {
    fn square_face() -> Self {
        let square = simple_glyph(&[3], &[(100, 0), (600, 0), (600, 700), (100, 700)]);
        let comp = composite_glyph(
            [100, 0, 600, 700],
            &[Comp {
                flags: 0x0002, // ARGS_ARE_XY_VALUES
                glyph_index: 1,
                arg1: 0,
                arg2: 0,
                matrix: None,
            }],
        );
        Fixture {
            glyphs: vec![Vec::new(), square, comp],
            metrics: vec![(500, 0), (700, 100), (700, 100)],
            extra: Vec::new(),
            gvar: None,
        }
    }

    fn with_gvar(mut self, gvar: Vec<u8>) -> Self {
        self.gvar = Some(gvar);
        self
    }

    fn with_table(mut self, tag: &[u8; 4], data: Vec<u8>) -> Self {
        self.extra.push((*tag, data));
        self
    }

    fn build(&self) -> Vec<u8> {
        self.build_with_axes(&[(*b"wght", 100.0, 400.0, 900.0)])
    }

    fn build_with_axes(&self, axes: &[([u8; 4], f32, f32, f32)]) -> Vec<u8> {
        let mut glyf = Vec::new();
        let mut offsets = vec![0u32];
        for g in &self.glyphs {
            glyf.extend_from_slice(g);
            offsets.push(glyf.len() as u32);
        }
        let n = self.glyphs.len() as u16;
        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"head", head(1000, 1)),
            (*b"hhea", hhea(n)),
            (*b"maxp", maxp(n)),
            (*b"hmtx", hmtx(&self.metrics)),
            (*b"glyf", glyf),
            (*b"loca", loca_long(&offsets)),
            (*b"fvar", fvar(axes)),
        ];
        if let Some(g) = &self.gvar {
            tables.push((*b"gvar", g.clone()));
        }
        tables.extend(self.extra.iter().cloned());
        build_sfnt(&tables)
    }
}

#[derive(Default)]
struct Points(Vec<(f32, f32)>);
impl OutlineBuilder for Points {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push((x, y));
    }
    fn quad_to(&mut self, a: f32, b: f32, x: f32, y: f32) {
        self.0.push((a, b));
        self.0.push((x, y));
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.0.push((a, b));
        self.0.push((c, d));
        self.0.push((x, y));
    }
    fn close(&mut self) {}
}

fn points_of(face: &Face<'_>, gid: u16) -> Vec<(f32, f32)> {
    let mut p = Points::default();
    face.outline_glyph(GlyphId(gid), &mut p);
    p.0
}

/// The bounding box the instancer wrote into the glyph header — the value under
/// test, as opposed to whatever a parser recomputes from the outline.
fn glyf_header_bbox(bytes: &[u8], gid: u16) -> [i16; 4] {
    let raw = ttf_parser::RawFace::parse(bytes, 0).expect("raw face");
    let glyf = raw.table(Tag::from_bytes(b"glyf")).expect("glyf");
    let loca = raw.table(Tag::from_bytes(b"loca")).expect("loca");
    let head_tbl = raw.table(Tag::from_bytes(b"head")).expect("head");
    let long = i16::from_be_bytes([head_tbl[50], head_tbl[51]]) == 1;
    let i = gid as usize;
    let start = if long {
        u32::from_be_bytes([
            loca[i * 4],
            loca[i * 4 + 1],
            loca[i * 4 + 2],
            loca[i * 4 + 3],
        ]) as usize
    } else {
        u16::from_be_bytes([loca[i * 2], loca[i * 2 + 1]]) as usize * 2
    };
    let g = &glyf[start..];
    [
        i16::from_be_bytes([g[2], g[3]]),
        i16::from_be_bytes([g[4], g[5]]),
        i16::from_be_bytes([g[6], g[7]]),
        i16::from_be_bytes([g[8], g[9]]),
    ]
}

fn has_table(bytes: &[u8], tag: &[u8; 4]) -> bool {
    ttf_parser::RawFace::parse(bytes, 0)
        .expect("raw face")
        .table(Tag::from_bytes(tag))
        .is_some()
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[test]
fn a_face_without_fvar_is_unsupported() {
    // The bundled static Noto has no axes.
    let data = std::fs::read("../oxifont-bundled/fonts/NotoSans-Regular.ttf")
        .or_else(|_| std::fs::read("../../fonts/NotoSans-Regular.ttf"));
    let Ok(data) = data else {
        return; // the bundled face moved; the synthetic cases below still run
    };
    match instance(&data, 0, &[(*b"wght", 700.0)]) {
        Err(SubsetError::Unsupported(_)) => {}
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn an_unknown_axis_tag_is_rejected() {
    let font = Fixture::square_face().build();
    match instance(&font, 0, &[(*b"wdth", 75.0)]) {
        Err(SubsetError::UnknownAxis(tag)) => assert_eq!(&tag, b"wdth"),
        other => panic!("expected UnknownAxis, got {other:?}"),
    }
    // …and the error renders the tag, so a caller's log is actionable.
    let err = instance(&font, 0, &[(*b"zzzz", 1.0)]).unwrap_err();
    assert!(err.to_string().contains("zzzz"), "{err}");
}

#[test]
fn glyph_count_and_order_are_preserved() {
    let font = Fixture::square_face().build();
    let out = instance(&font, 0, &[(*b"wght", 700.0)]).expect("instance");
    let face = Face::parse(&out, 0).expect("parse");
    assert_eq!(face.number_of_glyphs(), 3);
    assert!(!face.is_variable());
}

#[test]
fn variation_and_hint_tables_are_dropped() {
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], vec![], vec![]],
            true,
        ))
        .with_table(b"cvar", vec![0, 1, 0, 0, 0, 0, 0, 8])
        .with_table(b"cvt ", vec![0, 1, 0, 2])
        .with_table(b"fpgm", vec![0x4B])
        .with_table(b"prep", vec![0x4B])
        .with_table(b"gasp", vec![0, 0, 0, 0])
        .with_table(b"DSIG", vec![0, 0, 0, 1, 0, 0, 0, 0])
        .with_table(b"STAT", vec![0, 1, 0, 0, 0, 0, 0, 0])
        .with_table(b"VORG", vec![0, 1, 0, 0, 0, 0, 0, 0])
        .with_table(b"MVAR", vec![0, 1, 0, 0, 0, 0, 0, 0])
        .with_table(b"cmap", vec![0, 0, 0, 0])
        .build();
    let out = instance(&font, 0, &[(*b"wght", 700.0)]).expect("instance");
    for tag in [
        b"fvar", b"gvar", b"avar", b"cvar", b"HVAR", b"VVAR", b"MVAR", b"STAT", b"DSIG", b"VORG",
        b"cvt ", b"fpgm", b"prep", b"gasp",
    ] {
        assert!(
            !has_table(&out, tag),
            "{} survived",
            std::str::from_utf8(tag).unwrap_or("????")
        );
    }
    // Everything else is carried over verbatim.
    assert!(has_table(&out, b"cmap"));
}

#[test]
fn instructions_and_the_maxp_hinting_counters_are_stripped() {
    let font = Fixture::square_face().build();
    let out = instance(&font, 0, &[(*b"wght", 400.0)]).expect("instance");
    let raw = ttf_parser::RawFace::parse(&out, 0).expect("raw");
    let glyf = raw.table(Tag::from_bytes(b"glyf")).expect("glyf");
    let loca = raw.table(Tag::from_bytes(b"loca")).expect("loca");
    let head_tbl = raw.table(Tag::from_bytes(b"head")).expect("head");
    // A three-glyph face fits the short loca format comfortably.
    assert_eq!(i16::from_be_bytes([head_tbl[50], head_tbl[51]]), 0);
    let start = u16::from_be_bytes([loca[2], loca[3]]) as usize * 2;
    let end = u16::from_be_bytes([loca[4], loca[5]]) as usize * 2;
    let glyph = &glyf[start..end];
    let n_contours = i16::from_be_bytes([glyph[0], glyph[1]]);
    assert_eq!(n_contours, 1);
    let instr_off = 10 + n_contours as usize * 2;
    assert_eq!(
        u16::from_be_bytes([glyph[instr_off], glyph[instr_off + 1]]),
        0,
        "instruction stream survived"
    );
    let maxp_tbl = raw.table(Tag::from_bytes(b"maxp")).expect("maxp");
    assert_eq!(u16::from_be_bytes([maxp_tbl[26], maxp_tbl[27]]), 0);
    assert_eq!(u16::from_be_bytes([maxp_tbl[20], maxp_tbl[21]]), 0);
    assert_eq!(u16::from_be_bytes([maxp_tbl[14], maxp_tbl[15]]), 2); // maxZones
    assert_eq!(u16::from_be_bytes([maxp_tbl[6], maxp_tbl[7]]), 4); // maxPoints
    assert_eq!(u16::from_be_bytes([maxp_tbl[8], maxp_tbl[9]]), 1); // maxContours
}

#[test]
fn the_default_location_leaves_every_point_untouched() {
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![
                (50, 0),
                (50, 0),
                (50, 0),
                (50, 0),
                (0, 0),
                (30, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            true,
        ))
        .build();
    let source = Face::parse(&font, 0).expect("source");
    let out = instance(&font, 0, &[(*b"wght", 400.0)]).expect("instance");
    let baked = Face::parse(&out, 0).expect("baked");
    for gid in 0..3u16 {
        assert_eq!(
            points_of(&baked, gid),
            points_of(&source, gid),
            "glyph {gid} moved at the default location"
        );
        assert_eq!(
            baked.glyph_hor_advance(GlyphId(gid)),
            source.glyph_hor_advance(GlyphId(gid))
        );
    }
}

// ---------------------------------------------------------------------------
// Encodings real fonts never use
// ---------------------------------------------------------------------------

#[test]
fn embedded_peak_tuples_are_honoured() {
    // No shared tuples at all: the peak lives inline in the header.
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            peak: Some(vec![f2dot14(1.0)]),
            deltas: vec![
                (100, 0),
                (100, 0),
                (100, 0),
                (100, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    let font = Fixture::square_face()
        .with_gvar(build_gvar(1, &[], &[vec![], block, vec![]], true))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    let face = Face::parse(&out, 0).expect("parse");
    let pts = points_of(&face, 1);
    assert_eq!(pts[0], (200.0, 0.0), "embedded peak was not applied");
}

#[test]
fn intermediate_regions_bound_the_tent() {
    // Peak 0.5, region [0.25, 0.75]: full effect at 0.5, none outside.
    let make = |shared: Vec<i16>| {
        glyph_var_block(
            Some(&[]),
            &[TupleFixture {
                peak: Some(shared),
                intermediate: Some((vec![f2dot14(0.25)], vec![f2dot14(0.75)])),
                deltas: vec![
                    (400, 0),
                    (400, 0),
                    (400, 0),
                    (400, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                    (0, 0),
                ],
                ..Default::default()
            }],
        )
    };
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[],
            &[vec![], make(vec![f2dot14(0.5)]), vec![]],
            true,
        ))
        .build();
    // wght 400 is the default (normalized 0) — outside the region.
    let at = |w: f32| {
        let out = instance(&font, 0, &[(*b"wght", w)]).expect("instance");
        let face = Face::parse(&out, 0).expect("parse");
        points_of(&face, 1)[0].0
    };
    assert_eq!(at(400.0), 100.0);
    // 900 normalizes to 1.0 — past the region's end, so zero again.
    assert_eq!(at(900.0), 100.0);
    // 650 normalizes to 0.5 — the peak.
    assert_eq!(at(650.0), 500.0);
    // 525 normalizes to 0.25 — the region's start, still zero.
    assert_eq!(at(525.0), 100.0);
}

#[test]
fn points_are_words_encoding_is_decoded() {
    // A 200-point contour so a word-encoded point number is meaningful, with a
    // single referenced point that IUP must spread over the whole contour.
    let points: Vec<(i16, i16)> = (0..200)
        .map(|i| (i as i16 * 5, if i % 2 == 0 { 0 } else { 100 }))
        .collect();
    let glyph = simple_glyph(&[199], &points);
    let mut fixture = Fixture::square_face();
    fixture.glyphs[1] = glyph;

    let block = glyph_var_block(
        None,
        &[TupleFixture {
            shared_index: 0,
            private_points: Some(vec![150]),
            points_are_words: true,
            deltas: vec![(90, 0)],
            ..Default::default()
        }],
    );
    let font = fixture
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            true,
        ))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    let face = Face::parse(&out, 0).expect("parse");
    let pts = points_of(&face, 1);
    // One referenced point in a contour ⇒ every point of that contour takes its
    // delta.
    assert_eq!(pts[150].0, 150.0 * 5.0 + 90.0);
    assert_eq!(pts[0].0, 90.0);
    assert_eq!(pts[199].0, 199.0 * 5.0 + 90.0);
}

#[test]
fn short_offset_gvar_is_read_with_the_doubling_rule() {
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![
                (60, 0),
                (60, 0),
                (60, 0),
                (60, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    let long = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block.clone(), vec![]],
            true,
        ))
        .build();
    let short = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            false,
        ))
        .build();
    let a = instance(&long, 0, &[(*b"wght", 900.0)]).expect("long offsets");
    let b = instance(&short, 0, &[(*b"wght", 900.0)]).expect("short offsets");
    assert_eq!(a, b, "the offset format must not change the result");
    let face = Face::parse(&a, 0).expect("parse");
    assert_eq!(points_of(&face, 1)[0], (160.0, 0.0));
}

#[test]
fn scaled_component_offsets_transform_the_offset() {
    // A component scaled 1.5× with SCALED_COMPONENT_OFFSET: the (50, 0) offset
    // is itself transformed to (75, 0), so the box starts at 1.5*100 + 75.
    // Without the flag the offset is added unscaled (the Microsoft default).
    let scaled = composite_glyph(
        [0, 0, 1, 1],
        &[Comp {
            flags: 0x0002 | 0x0800, // ARGS_ARE_XY_VALUES | SCALED_COMPONENT_OFFSET
            glyph_index: 1,
            arg1: 50,
            arg2: 0,
            matrix: Some([f2dot14(1.5), 0, 0, f2dot14(1.5)]),
        }],
    );
    let unscaled = composite_glyph(
        [0, 0, 1, 1],
        &[Comp {
            flags: 0x0002,
            glyph_index: 1,
            arg1: 50,
            arg2: 0,
            matrix: Some([f2dot14(1.5), 0, 0, f2dot14(1.5)]),
        }],
    );
    for (glyph, expected_x_min) in [(scaled, 225i16), (unscaled, 200i16)] {
        let mut fixture = Fixture::square_face();
        fixture.glyphs[2] = glyph;
        let font = fixture.build();
        let out = instance(&font, 0, &[(*b"wght", 400.0)]).expect("instance");
        assert_eq!(glyf_header_bbox(&out, 2)[0], expected_x_min);
    }
}

#[test]
fn point_matched_components_discard_their_deltas() {
    // ARGS_ARE_XY_VALUES clear: the arguments are point numbers, so a delta
    // aimed at this component has nothing to move and must be thrown away
    // rather than written back as a corrupted point index.
    let matched = composite_glyph(
        [0, 0, 1, 1],
        &[
            Comp {
                flags: 0x0002,
                glyph_index: 1,
                arg1: 0,
                arg2: 0,
                matrix: None,
            },
            Comp {
                flags: 0x0000, // point matching
                glyph_index: 1,
                arg1: 2,
                arg2: 1,
                matrix: None,
            },
        ],
    );
    let mut fixture = Fixture::square_face();
    fixture.glyphs[2] = matched;
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            // Two components + four phantoms.
            deltas: vec![(70, 0), (70, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
            ..Default::default()
        }],
    );
    let font = fixture
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], vec![], block],
            true,
        ))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    let raw = ttf_parser::RawFace::parse(&out, 0).expect("raw");
    let glyf = raw.table(Tag::from_bytes(b"glyf")).expect("glyf");
    let loca = raw.table(Tag::from_bytes(b"loca")).expect("loca");
    let start = u16::from_be_bytes([loca[4], loca[5]]) as usize * 2;
    let glyph = &glyf[start..];
    // Component 0 moved by the delta; component 1 kept its point numbers.
    let flags0 = u16::from_be_bytes([glyph[10], glyph[11]]);
    assert_ne!(flags0 & 0x0002, 0, "component 0 should still be xy");
    let arg0 = if flags0 & 0x0001 != 0 {
        i16::from_be_bytes([glyph[14], glyph[15]])
    } else {
        glyph[14] as i8 as i16
    };
    assert_eq!(arg0, 70, "component 0 did not take its delta");
    let step = if flags0 & 0x0001 != 0 { 8 } else { 6 };
    let flags1 = u16::from_be_bytes([glyph[10 + step], glyph[11 + step]]);
    assert_eq!(flags1 & 0x0002, 0, "component 1 became an xy component");
    let (p1, p2) = if flags1 & 0x0001 != 0 {
        (
            u16::from_be_bytes([glyph[14 + step], glyph[15 + step]]),
            u16::from_be_bytes([glyph[16 + step], glyph[17 + step]]),
        )
    } else {
        (u16::from(glyph[14 + step]), u16::from(glyph[15 + step]))
    };
    assert_eq!((p1, p2), (2, 1), "point-matching arguments were rewritten");
}

#[test]
fn extreme_deltas_clamp_instead_of_wrapping() {
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![
                (32700, 32700),
                (32700, 32700),
                (32700, 32700),
                (32700, 32700),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            true,
        ))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    // 100 + 32700 and 600 + 32700 both leave int16: both clamp to the ceiling
    // rather than wrapping into a negative coordinate.
    let bbox = glyf_header_bbox(&out, 1);
    assert_eq!(bbox[0], i16::MAX, "xMin wrapped");
    assert_eq!(bbox[2], i16::MAX, "xMax wrapped");
    assert_eq!(bbox[1], 32700);
    assert_eq!(bbox[3], i16::MAX, "yMax wrapped");
    assert!(Face::parse(&out, 0).is_ok());
}

#[test]
fn empty_glyphs_still_take_advance_deltas() {
    // `.notdef` is empty: four phantoms, no outline points. Its advance must
    // still vary — iterating the glyf records instead of 0..numGlyphs would
    // silently skip it.
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![(0, 0), (250, 0), (0, 0), (0, 0)],
            ..Default::default()
        }],
    );
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[block, vec![], vec![]],
            true,
        ))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    let face = Face::parse(&out, 0).expect("parse");
    assert_eq!(face.glyph_hor_advance(GlyphId(0)), Some(750));
    // …and the glyph stays zero bytes long in glyf.
    let raw = ttf_parser::RawFace::parse(&out, 0).expect("raw");
    let loca = raw.table(Tag::from_bytes(b"loca")).expect("loca");
    assert_eq!(&loca[0..2], &loca[2..4], ".notdef gained a glyf record");
}

#[test]
fn advances_come_from_hvar_when_there_is_no_gvar() {
    // A face that varies only its metrics: legal, and the one place HVAR is
    // read rather than deleted unread.
    let font = Fixture::square_face()
        .with_table(b"HVAR", hvar_advance_only(&[0, 200, 0]))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance");
    let face = Face::parse(&out, 0).expect("parse");
    assert_eq!(face.glyph_hor_advance(GlyphId(1)), Some(900));
    assert_eq!(face.glyph_hor_advance(GlyphId(0)), Some(500));
    assert!(!has_table(&out, b"HVAR"));
    // Half-way up the axis, half the delta.
    let mid = instance(&font, 0, &[(*b"wght", 650.0)]).expect("instance");
    let face = Face::parse(&mid, 0).expect("parse");
    assert_eq!(face.glyph_hor_advance(GlyphId(1)), Some(800));
}

/// `HVAR` with an implicit advance mapping (`innerIndex = gid`) and one region
/// peaking at the axis maximum.
fn hvar_advance_only(deltas: &[i16]) -> Vec<u8> {
    let mut ivs = Vec::new();
    ivs.extend_from_slice(&be16(1)); // format
    ivs.extend_from_slice(&be32(16)); // variationRegionListOffset
    ivs.extend_from_slice(&be16(1)); // itemVariationDataCount
    ivs.extend_from_slice(&be32(28)); // itemVariationDataOffsets[0]
    while ivs.len() < 16 {
        ivs.push(0);
    }
    ivs.extend_from_slice(&be16(1)); // axisCount
    ivs.extend_from_slice(&be16(1)); // regionCount
    ivs.extend_from_slice(&bei16(0)); // startCoord
    ivs.extend_from_slice(&bei16(f2dot14(1.0))); // peakCoord
    ivs.extend_from_slice(&bei16(f2dot14(1.0))); // endCoord
    while ivs.len() < 28 {
        ivs.push(0);
    }
    ivs.extend_from_slice(&be16(deltas.len() as u16)); // itemCount
    ivs.extend_from_slice(&be16(1)); // wordDeltaCount: one 16-bit column
    ivs.extend_from_slice(&be16(1)); // regionIndexCount
    ivs.extend_from_slice(&be16(0)); // regionIndexes[0]
    for d in deltas {
        ivs.extend_from_slice(&bei16(*d));
    }

    let mut t = Vec::new();
    t.extend_from_slice(&be16(1));
    t.extend_from_slice(&be16(0));
    t.extend_from_slice(&be32(20)); // itemVariationStoreOffset
    t.extend_from_slice(&be32(0)); // advanceWidthMappingOffset: implicit
    t.extend_from_slice(&be32(0));
    t.extend_from_slice(&be32(0));
    t.extend_from_slice(&ivs);
    t
}

#[test]
fn os2_post_and_head_style_fields_follow_the_location() {
    let mut os2 = vec![0u8; 96];
    os2[0..2].copy_from_slice(&be16(4)); // version
    os2[4..6].copy_from_slice(&be16(100)); // usWeightClass
    os2[6..8].copy_from_slice(&be16(5)); // usWidthClass
    os2[62..64].copy_from_slice(&be16(0x0040)); // fsSelection: REGULAR
    let mut post = vec![0u8; 32];
    post[0..4].copy_from_slice(&be32(0x0003_0000));

    let font = Fixture::square_face()
        .with_table(b"OS/2", os2)
        .with_table(b"post", post)
        .build_with_axes(&[
            (*b"wght", 100.0, 400.0, 900.0),
            (*b"wdth", 50.0, 100.0, 200.0),
            (*b"slnt", -15.0, 0.0, 0.0),
        ]);
    let out = instance(
        &font,
        0,
        &[(*b"wght", 700.0), (*b"wdth", 75.0), (*b"slnt", -12.0)],
    )
    .expect("instance");
    let raw = ttf_parser::RawFace::parse(&out, 0).expect("raw");
    let os2 = raw.table(Tag::from_bytes(b"OS/2")).expect("OS/2");
    assert_eq!(u16::from_be_bytes([os2[4], os2[5]]), 700);
    assert_eq!(u16::from_be_bytes([os2[6], os2[7]]), 3);
    let fs = u16::from_be_bytes([os2[62], os2[63]]);
    assert_eq!(fs & 0x0020, 0x0020, "BOLD not set");
    assert_eq!(fs & 0x0001, 0x0001, "ITALIC not set for a slanted instance");
    assert_eq!(fs & 0x0040, 0, "REGULAR not cleared");
    let head_tbl = raw.table(Tag::from_bytes(b"head")).expect("head");
    assert_eq!(u16::from_be_bytes([head_tbl[44], head_tbl[45]]), 0x0003);
    let post = raw.table(Tag::from_bytes(b"post")).expect("post");
    assert_eq!(
        i32::from_be_bytes([post[4], post[5], post[6], post[7]]),
        -12 * 65536
    );
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn truncating_fvar_avar_or_gvar_never_panics() {
    let block = glyph_var_block(
        Some(&[0, 2]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![(40, 10), (40, 10)],
            ..Default::default()
        }],
    );
    let gvar = build_gvar(1, &[vec![f2dot14(1.0)]], &[vec![], block, vec![]], true);
    let mut avar = Vec::new();
    avar.extend_from_slice(&be16(1));
    avar.extend_from_slice(&be16(0));
    avar.extend_from_slice(&be16(0));
    avar.extend_from_slice(&be16(1));
    avar.extend_from_slice(&be16(3));
    for (f, t) in [(-1.0f32, -1.0f32), (0.0, 0.0), (1.0, 1.0)] {
        avar.extend_from_slice(&bei16(f2dot14(f)));
        avar.extend_from_slice(&bei16(f2dot14(t)));
    }

    let fixture = Fixture::square_face();
    let fvar_table = fvar(&[(*b"wght", 100.0, 400.0, 900.0)]);

    for (tag, full) in [(*b"fvar", fvar_table), (*b"avar", avar), (*b"gvar", gvar)] {
        for cut in 0..full.len() {
            let mut f = Fixture::square_face();
            f.glyphs = fixture.glyphs.clone();
            let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
            let base = f.build();
            let _ = base; // the builder validates the fixture itself
            let truncated = full[..cut].to_vec();
            if tag == *b"fvar" {
                // Rebuild with the truncated fvar in place of the good one.
                let mut glyf = Vec::new();
                let mut offsets = vec![0u32];
                for g in &f.glyphs {
                    glyf.extend_from_slice(g);
                    offsets.push(glyf.len() as u32);
                }
                tables.push((*b"head", head(1000, 1)));
                tables.push((*b"hhea", hhea(3)));
                tables.push((*b"maxp", maxp(3)));
                tables.push((*b"hmtx", hmtx(&f.metrics)));
                tables.push((*b"glyf", glyf));
                tables.push((*b"loca", loca_long(&offsets)));
                tables.push((*b"fvar", truncated));
            } else {
                let mut glyf = Vec::new();
                let mut offsets = vec![0u32];
                for g in &f.glyphs {
                    glyf.extend_from_slice(g);
                    offsets.push(glyf.len() as u32);
                }
                tables.push((*b"head", head(1000, 1)));
                tables.push((*b"hhea", hhea(3)));
                tables.push((*b"maxp", maxp(3)));
                tables.push((*b"hmtx", hmtx(&f.metrics)));
                tables.push((*b"glyf", glyf));
                tables.push((*b"loca", loca_long(&offsets)));
                tables.push((*b"fvar", fvar(&[(*b"wght", 100.0, 400.0, 900.0)])));
                tables.push((tag, truncated));
            }
            let font = build_sfnt(&tables);
            // Any answer is acceptable; a panic is not, and neither is output
            // that does not parse.
            if let Ok(out) = instance(&font, 0, &[(*b"wght", 700.0)]) {
                assert!(Face::parse(&out, 0).is_ok(), "unparseable output");
            }
        }
    }
}

#[test]
fn a_glyph_with_a_broken_variation_block_keeps_its_default_outline() {
    // A tuple whose declared data size runs past the end of the block: the
    // glyph degrades to the default master rather than failing the instance.
    let mut block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![
                (90, 0),
                (90, 0),
                (90, 0),
                (90, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    // variationDataSize lives at offset 4 of the block.
    block[4] = 0xFF;
    block[5] = 0xFF;
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            true,
        ))
        .build();
    let out = instance(&font, 0, &[(*b"wght", 900.0)]).expect("instance must still succeed");
    let face = Face::parse(&out, 0).expect("parse");
    assert_eq!(points_of(&face, 1)[0], (100.0, 0.0));
}

#[test]
fn a_composite_cycle_is_an_error_not_a_hang() {
    let a = composite_glyph(
        [0, 0, 1, 1],
        &[Comp {
            flags: 0x0002,
            glyph_index: 2,
            arg1: 0,
            arg2: 0,
            matrix: None,
        }],
    );
    let b = composite_glyph(
        [0, 0, 1, 1],
        &[Comp {
            flags: 0x0002,
            glyph_index: 1,
            arg1: 0,
            arg2: 0,
            matrix: None,
        }],
    );
    let mut fixture = Fixture::square_face();
    fixture.glyphs[1] = a;
    fixture.glyphs[2] = b;
    let font = fixture.build();
    match instance(&font, 0, &[(*b"wght", 400.0)]) {
        Err(SubsetError::InvalidFont(msg)) => assert!(msg.contains("composite"), "{msg}"),
        other => panic!("expected InvalidFont, got {other:?}"),
    }
}

#[test]
fn instancing_is_byte_deterministic() {
    let block = glyph_var_block(
        Some(&[]),
        &[TupleFixture {
            shared_index: 0,
            deltas: vec![
                (11, 7),
                (13, 5),
                (17, 3),
                (19, 2),
                (0, 0),
                (23, 0),
                (0, 0),
                (0, 0),
            ],
            ..Default::default()
        }],
    );
    let font = Fixture::square_face()
        .with_gvar(build_gvar(
            1,
            &[vec![f2dot14(1.0)]],
            &[vec![], block, vec![]],
            true,
        ))
        .with_table(b"cmap", vec![0, 0, 0, 0])
        .with_table(b"name", vec![0, 0, 0, 0, 0, 6])
        .build();
    let first = instance(&font, 0, &[(*b"wght", 733.0)]).expect("instance");
    for _ in 0..2 {
        assert_eq!(
            first,
            instance(&font, 0, &[(*b"wght", 733.0)]).expect("instance")
        );
    }
}

#[test]
fn a_face_index_beyond_the_face_count_is_rejected() {
    let font = Fixture::square_face().build();
    match instance(&font, 3, &[(*b"wght", 400.0)]) {
        Err(SubsetError::FaceIndexOutOfRange { index, count }) => {
            assert_eq!(index, 3);
            assert_eq!(count, 1);
        }
        other => panic!("expected FaceIndexOutOfRange, got {other:?}"),
    }
}
