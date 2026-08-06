//! Glyph outlines: `glyf` decoding, IUP, control boxes, and re-serialization.
//!
//! Composites are kept as composites. Flattening them through a pen — what
//! skrifa-based instancers do — would inflate every accent-bearing glyph,
//! discard `ROUND_XY_TO_GRID` / `USE_MY_METRICS`, and round-trip implied
//! on-curve midpoints. Only the component offsets move.

use crate::glyf::{
    ARGS_ARE_XY_VALUES, ARG_1_AND_2_ARE_WORDS, MORE_COMPONENTS, SCALED_COMPONENT_OFFSET,
    UNSCALED_COMPONENT_OFFSET, WE_HAVE_AN_X_AND_Y_SCALE, WE_HAVE_A_SCALE, WE_HAVE_A_TWO_BY_TWO,
    WE_HAVE_INSTRUCTIONS,
};
use crate::tables::{get_i16, get_u16, SubsetError};

// Simple-glyph point flags.
const ON_CURVE_POINT: u8 = 0x01;
const X_SHORT_VECTOR: u8 = 0x02;
const Y_SHORT_VECTOR: u8 = 0x04;
const REPEAT_FLAG: u8 = 0x08;
const X_SAME_OR_POSITIVE: u8 = 0x10;
const Y_SAME_OR_POSITIVE: u8 = 0x20;
const OVERLAP_SIMPLE: u8 = 0x40;

/// Flags carried across instancing unchanged. Everything else describes the
/// *encoding* of the old coordinates and is recomputed from the new ones.
const PRESERVED_POINT_FLAGS: u8 = ON_CURVE_POINT | OVERLAP_SIMPLE;

/// Maximum composite nesting honoured when decomposing for bounding boxes.
const MAX_COMPOSITE_DEPTH: usize = 8;

/// `otRound`: round half **toward +∞**.
///
/// `ot_round(-60.5) == -60`, unlike `f64::round`, which gives `-61`. Every
/// coordinate, advance and bearing in the output goes through this exactly once,
/// after all accumulation has happened in `f64`.
pub(crate) fn ot_round(v: f64) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let r = (v + 0.5).floor();
    if r >= f64::from(i32::MAX) {
        i32::MAX
    } else if r <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        r as i32
    }
}

/// Clamp to the `int16` domain `glyf` coordinates live in.
fn clamp_i16(v: i32) -> i32 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

// ---------------------------------------------------------------------------
// Component transforms
// ---------------------------------------------------------------------------

/// A component's 2×2 transform, in the exact encoding the source used, so it can
/// be written back byte-for-byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scale {
    /// No transform record.
    None,
    /// `WE_HAVE_A_SCALE`: one F2Dot14 used for both axes.
    Uniform(i16),
    /// `WE_HAVE_AN_X_AND_Y_SCALE`: two F2Dot14.
    Xy(i16, i16),
    /// `WE_HAVE_A_TWO_BY_TWO`: `xscale, scale01, scale10, yscale`.
    Matrix(i16, i16, i16, i16),
}

impl Scale {
    /// `(xscale, scale01, scale10, yscale)` as `f64`.
    ///
    /// The transform is row-vector (FreeType's):
    /// `x' = xscale·x + scale10·y + dx`, `y' = scale01·x + yscale·y + dy`.
    /// The transposed convention moves rotated components by up to 250 units.
    fn matrix(self) -> (f64, f64, f64, f64) {
        const S: f64 = 16384.0;
        match self {
            Scale::None => (1.0, 0.0, 0.0, 1.0),
            Scale::Uniform(s) => (f64::from(s) / S, 0.0, 0.0, f64::from(s) / S),
            Scale::Xy(x, y) => (f64::from(x) / S, 0.0, 0.0, f64::from(y) / S),
            Scale::Matrix(a, b, c, d) => (
                f64::from(a) / S,
                f64::from(b) / S,
                f64::from(c) / S,
                f64::from(d) / S,
            ),
        }
    }

    fn write(self, out: &mut Vec<u8>) {
        match self {
            Scale::None => {}
            Scale::Uniform(s) => out.extend_from_slice(&s.to_be_bytes()),
            Scale::Xy(x, y) => {
                out.extend_from_slice(&x.to_be_bytes());
                out.extend_from_slice(&y.to_be_bytes());
            }
            Scale::Matrix(a, b, c, d) => {
                out.extend_from_slice(&a.to_be_bytes());
                out.extend_from_slice(&b.to_be_bytes());
                out.extend_from_slice(&c.to_be_bytes());
                out.extend_from_slice(&d.to_be_bytes());
            }
        }
    }
}

/// One component of a composite glyph.
#[derive(Debug, Clone)]
pub(crate) struct Component {
    /// Source flags. `MORE_COMPONENTS`, `WE_HAVE_INSTRUCTIONS` and
    /// `ARG_1_AND_2_ARE_WORDS` are recomputed at write time; every other bit
    /// (including `ROUND_XY_TO_GRID` and `USE_MY_METRICS`) is preserved.
    pub(crate) flags: u16,
    /// The referenced glyph.
    pub(crate) glyph_index: u16,
    /// `argument1`: an x offset when `ARGS_ARE_XY_VALUES`, else a point number.
    pub(crate) arg1: i32,
    /// `argument2`: a y offset when `ARGS_ARE_XY_VALUES`, else a point number.
    pub(crate) arg2: i32,
    /// The 2×2 transform, if any.
    pub(crate) scale: Scale,
}

impl Component {
    /// Whether this component's arguments are an offset that variation deltas
    /// may move. Point-matched components have no offset to vary, so any delta
    /// aimed at them is discarded.
    pub(crate) fn has_xy_offset(&self) -> bool {
        self.flags & ARGS_ARE_XY_VALUES != 0
    }
}

// ---------------------------------------------------------------------------
// Parsed source glyphs
// ---------------------------------------------------------------------------

/// A simple glyph's decoded outline.
#[derive(Debug, Clone, Default)]
pub(crate) struct SimpleOutline {
    /// `endPtsOfContours`.
    pub(crate) end_pts: Vec<u16>,
    /// Per-point flags, masked to [`PRESERVED_POINT_FLAGS`].
    pub(crate) flags: Vec<u8>,
    /// Absolute x coordinates.
    pub(crate) xs: Vec<i32>,
    /// Absolute y coordinates.
    pub(crate) ys: Vec<i32>,
}

/// A glyph as `glyf` stores it, or as the instancer will store it back.
#[derive(Debug, Clone)]
pub(crate) enum Outline {
    /// Zero bytes in `glyf` (`loca[g] == loca[g+1]`), or zero contours.
    Empty,
    /// A simple glyph.
    Simple(SimpleOutline),
    /// A composite glyph; never empty.
    Composite(Vec<Component>),
}

impl Outline {
    /// The number of *real* points the variation store addresses — outline
    /// points for a simple glyph, one per component for a composite.
    pub(crate) fn real_point_count(&self) -> usize {
        match self {
            Outline::Empty => 0,
            Outline::Simple(s) => s.xs.len(),
            Outline::Composite(c) => c.len(),
        }
    }
}

/// A glyph decoded from `glyf`, with the header box the phantom points need.
pub(crate) struct SourceGlyph {
    /// The outline itself.
    pub(crate) outline: Outline,
    /// `xMin` from the glyph header (`0` for an empty glyph).
    pub(crate) x_min: i32,
    /// `yMax` from the glyph header (`0` for an empty glyph).
    pub(crate) y_max: i32,
}

/// Decode one glyph from its `glyf` slice.
///
/// A zero-length slice is [`Outline::Empty`]; so is a glyph declaring zero
/// contours. `None` means the bytes are structurally broken.
pub(crate) fn parse_glyph(data: &[u8]) -> Option<SourceGlyph> {
    if data.is_empty() {
        return Some(SourceGlyph {
            outline: Outline::Empty,
            x_min: 0,
            y_max: 0,
        });
    }
    let num_contours = get_i16(data, 0)?;
    let x_min = i32::from(get_i16(data, 2)?);
    let y_max = i32::from(get_i16(data, 8)?);

    if num_contours == 0 {
        return Some(SourceGlyph {
            outline: Outline::Empty,
            x_min: 0,
            y_max: 0,
        });
    }
    let outline = if num_contours > 0 {
        Outline::Simple(parse_simple(data, num_contours as usize)?)
    } else {
        Outline::Composite(parse_composite(data)?)
    };
    Some(SourceGlyph {
        outline,
        x_min,
        y_max,
    })
}

fn parse_simple(data: &[u8], num_contours: usize) -> Option<SimpleOutline> {
    let mut pos = 10usize;
    let mut end_pts = Vec::with_capacity(num_contours);
    for _ in 0..num_contours {
        end_pts.push(get_u16(data, pos)?);
        pos += 2;
    }
    let num_points = usize::from(*end_pts.last()?).checked_add(1)?;
    // A contour list must be non-decreasing and stay inside the point count.
    if end_pts.windows(2).any(|w| w[0] > w[1]) {
        return None;
    }

    let instruction_len = usize::from(get_u16(data, pos)?);
    pos = pos.checked_add(2)?.checked_add(instruction_len)?;
    if pos > data.len() {
        return None;
    }

    // Each point needs at least one flag byte, so `num_points` can never exceed
    // what remains — this is the bound on the three allocations below.
    if num_points > data.len().saturating_sub(pos).saturating_add(1) * 8 {
        return None;
    }

    let mut flags: Vec<u8> = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let f = *data.get(pos)?;
        pos += 1;
        flags.push(f);
        if f & REPEAT_FLAG != 0 {
            let repeat = *data.get(pos)?;
            pos += 1;
            for _ in 0..repeat {
                if flags.len() >= num_points {
                    break;
                }
                flags.push(f);
            }
        }
    }

    let mut xs = Vec::with_capacity(num_points);
    let mut acc: i32 = 0;
    for &f in &flags {
        if f & X_SHORT_VECTOR != 0 {
            let v = i32::from(*data.get(pos)?);
            pos += 1;
            acc += if f & X_SAME_OR_POSITIVE != 0 { v } else { -v };
        } else if f & X_SAME_OR_POSITIVE == 0 {
            acc += i32::from(get_i16(data, pos)?);
            pos += 2;
        }
        xs.push(acc);
    }

    let mut ys = Vec::with_capacity(num_points);
    acc = 0;
    for &f in &flags {
        if f & Y_SHORT_VECTOR != 0 {
            let v = i32::from(*data.get(pos)?);
            pos += 1;
            acc += if f & Y_SAME_OR_POSITIVE != 0 { v } else { -v };
        } else if f & Y_SAME_OR_POSITIVE == 0 {
            acc += i32::from(get_i16(data, pos)?);
            pos += 2;
        }
        ys.push(acc);
    }

    for f in &mut flags {
        *f &= PRESERVED_POINT_FLAGS;
    }

    Some(SimpleOutline {
        end_pts,
        flags,
        xs,
        ys,
    })
}

fn parse_composite(data: &[u8]) -> Option<Vec<Component>> {
    let mut pos = 10usize;
    let mut components = Vec::new();
    loop {
        let flags = get_u16(data, pos)?;
        let glyph_index = get_u16(data, pos + 2)?;
        pos += 4;

        let words = flags & ARG_1_AND_2_ARE_WORDS != 0;
        let signed = flags & ARGS_ARE_XY_VALUES != 0;
        let (arg1, arg2) = match (words, signed) {
            (true, true) => {
                let a = i32::from(get_i16(data, pos)?);
                let b = i32::from(get_i16(data, pos + 2)?);
                pos += 4;
                (a, b)
            }
            (true, false) => {
                let a = i32::from(get_u16(data, pos)?);
                let b = i32::from(get_u16(data, pos + 2)?);
                pos += 4;
                (a, b)
            }
            (false, true) => {
                let a = i32::from(*data.get(pos)? as i8);
                let b = i32::from(*data.get(pos + 1)? as i8);
                pos += 2;
                (a, b)
            }
            (false, false) => {
                let a = i32::from(*data.get(pos)?);
                let b = i32::from(*data.get(pos + 1)?);
                pos += 2;
                (a, b)
            }
        };

        let scale = if flags & WE_HAVE_A_SCALE != 0 {
            let s = get_i16(data, pos)?;
            pos += 2;
            Scale::Uniform(s)
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            let x = get_i16(data, pos)?;
            let y = get_i16(data, pos + 2)?;
            pos += 4;
            Scale::Xy(x, y)
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            let a = get_i16(data, pos)?;
            let b = get_i16(data, pos + 2)?;
            let c = get_i16(data, pos + 4)?;
            let d = get_i16(data, pos + 6)?;
            pos += 8;
            Scale::Matrix(a, b, c, d)
        } else {
            Scale::None
        };

        components.push(Component {
            flags,
            glyph_index,
            arg1,
            arg2,
            scale,
        });

        if flags & MORE_COMPONENTS == 0 {
            break;
        }
        // A composite with more components than the table could hold is broken.
        if components.len() > data.len() {
            return None;
        }
    }
    if components.is_empty() {
        return None;
    }
    Some(components)
}

// ---------------------------------------------------------------------------
// IUP — Interpolate Untouched Points
// ---------------------------------------------------------------------------

/// Interpolate one dimension of an unreferenced point between two references.
fn iup_segment_1d(x: f64, x1: f64, d1: f64, x2: f64, d2: f64) -> f64 {
    if x1 == x2 {
        // Degenerate span: agree or contribute nothing. Never "pick one".
        return if d1 == d2 { d1 } else { 0.0 };
    }
    let (lo, lo_d, hi, hi_d) = if x1 < x2 {
        (x1, d1, x2, d2)
    } else {
        (x2, d2, x1, d1)
    };
    if x <= lo {
        return lo_d;
    }
    if x >= hi {
        return hi_d;
    }
    lo_d + (x - lo) * (hi_d - lo_d) / (hi - lo)
}

/// Infer deltas for the unreferenced points of one contour.
///
/// `sparse` holds the explicit (already scaled) deltas; `coords` is the
/// **default** outline for the same span. Reference points pair cyclically
/// within the contour, so the last referenced point pairs with the first.
/// `out` is cleared and filled with one delta per point.
pub(crate) fn iup_contour(
    sparse: &[Option<(f64, f64)>],
    coords: &[(f64, f64)],
    out: &mut Vec<(f64, f64)>,
) {
    let n = sparse.len();
    out.clear();
    out.resize(n, (0.0, 0.0));
    if n == 0 || coords.len() < n {
        return;
    }

    let mut idx: Vec<usize> = Vec::with_capacity(n);
    for (i, s) in sparse.iter().enumerate() {
        if let Some(d) = *s {
            out[i] = d;
            idx.push(i);
        }
    }
    if idx.is_empty() || idx.len() == n {
        // Nothing referenced ⇒ all zeros; everything referenced ⇒ nothing to infer.
        return;
    }
    if idx.len() == 1 {
        // One reference: the whole contour takes its delta.
        let d = out[idx[0]];
        for slot in out.iter_mut() {
            *slot = d;
        }
        return;
    }

    for k in 0..idx.len() {
        let i1 = idx[k];
        let i2 = idx[(k + 1) % idx.len()];
        let (x1, y1) = coords[i1];
        let (x2, y2) = coords[i2];
        let (dx1, dy1) = out[i1];
        let (dx2, dy2) = out[i2];
        let mut p = (i1 + 1) % n;
        while p != i2 {
            let (px, py) = coords[p];
            out[p] = (
                iup_segment_1d(px, x1, dx1, x2, dx2),
                iup_segment_1d(py, y1, dy1, y2, dy2),
            );
            p = (p + 1) % n;
        }
    }
}

/// Build the IUP spans for a glyph: one per real contour, then four
/// single-point spans for the phantoms.
///
/// The phantom singletons are what stop an un-referenced phantom from being
/// interpolated out of the outline points — get this wrong and every advance
/// width is silently corrupted.
pub(crate) fn contour_spans(outline: &Outline, out: &mut Vec<(usize, usize)>) {
    out.clear();
    let n_real = outline.real_point_count();
    match outline {
        Outline::Simple(s) => {
            let mut prev = 0usize;
            for &e in &s.end_pts {
                let end = (usize::from(e) + 1).min(n_real);
                if end > prev {
                    out.push((prev, end));
                }
                prev = end;
            }
        }
        Outline::Composite(_) => {
            // Each component is its own single-point contour, so a component
            // the tuple does not reference gets a zero delta rather than an
            // interpolation from a neighbouring component's offset. Measured:
            // treating the whole run as one contour moves bahnschrift's
            // accented composites by up to 696 units away from what a live
            // variable-font renderer produces (218 of 959 glyphs at wdth 75).
            for i in 0..n_real {
                out.push((i, i + 1));
            }
        }
        Outline::Empty => {}
    }
    for k in 0..4 {
        out.push((n_real + k, n_real + k + 1));
    }
}

// ---------------------------------------------------------------------------
// Control boxes and composite decomposition
// ---------------------------------------------------------------------------

/// The control box of a rounded point list: min/max over the points themselves,
/// **not** the tight curve box.
pub(crate) fn control_box(points: &[(i32, i32)]) -> [i16; 4] {
    let mut it = points.iter();
    let Some(&(x0, y0)) = it.next() else {
        return [0, 0, 0, 0];
    };
    let (mut x_min, mut y_min, mut x_max, mut y_max) = (x0, y0, x0, y0);
    for &(x, y) in it {
        x_min = x_min.min(x);
        y_min = y_min.min(y);
        x_max = x_max.max(x);
        y_max = y_max.max(y);
    }
    [
        clamp_i16(x_min) as i16,
        clamp_i16(y_min) as i16,
        clamp_i16(x_max) as i16,
        clamp_i16(y_max) as i16,
    ]
}

/// Flatten glyph `gid` to absolute rounded points, following component
/// transforms.
///
/// Used only for bounding boxes and the `maxp` composite counters; the emitted
/// `glyf` keeps composites intact. Returns `(points, contour_count)`.
///
/// # Errors
/// [`SubsetError::InvalidFont`] when the component graph loops or nests deeper
/// than eight levels.
pub(crate) fn decompose(
    glyphs: &[Outline],
    gid: u16,
    stack: &mut Vec<u16>,
) -> Result<(Vec<(i32, i32)>, usize), SubsetError> {
    if stack.len() >= MAX_COMPOSITE_DEPTH {
        return Err(SubsetError::InvalidFont(format!(
            "composite glyph {gid} nests deeper than {MAX_COMPOSITE_DEPTH} levels"
        )));
    }
    if stack.contains(&gid) {
        return Err(SubsetError::InvalidFont(format!(
            "composite glyph {gid} references itself"
        )));
    }
    let Some(outline) = glyphs.get(usize::from(gid)) else {
        return Ok((Vec::new(), 0));
    };
    match outline {
        Outline::Empty => Ok((Vec::new(), 0)),
        Outline::Simple(s) => Ok((
            s.xs.iter().copied().zip(s.ys.iter().copied()).collect(),
            s.end_pts.len(),
        )),
        Outline::Composite(components) => {
            stack.push(gid);
            let mut points: Vec<(i32, i32)> = Vec::new();
            let mut contours = 0usize;
            for comp in components {
                let (base, base_contours) = decompose(glyphs, comp.glyph_index, stack)?;
                let (a, b, c, d) = comp.scale.matrix();
                let transformed: Vec<(f64, f64)> = base
                    .iter()
                    .map(|&(x, y)| {
                        let (x, y) = (f64::from(x), f64::from(y));
                        (a * x + c * y, b * x + d * y)
                    })
                    .collect();
                let (dx, dy) = if comp.has_xy_offset() {
                    let (mut dx, mut dy) = (f64::from(comp.arg1), f64::from(comp.arg2));
                    // Neither flag ⇒ unscaled (the Microsoft default).
                    if comp.flags & SCALED_COMPONENT_OFFSET != 0
                        && comp.flags & UNSCALED_COMPONENT_OFFSET == 0
                    {
                        let (sx, sy) = (a * dx + c * dy, b * dx + d * dy);
                        dx = sx;
                        dy = sy;
                    }
                    (dx, dy)
                } else {
                    // Point matching: the component's point `arg2` is placed on
                    // top of the already-composed point `arg1`.
                    let anchor = points.get(comp.arg1 as usize).copied().unwrap_or((0, 0));
                    let moving = transformed
                        .get(comp.arg2 as usize)
                        .copied()
                        .unwrap_or((0.0, 0.0));
                    (
                        f64::from(anchor.0) - moving.0,
                        f64::from(anchor.1) - moving.1,
                    )
                };
                for (x, y) in transformed {
                    points.push((clamp_i16(ot_round(x + dx)), clamp_i16(ot_round(y + dy))));
                }
                contours += base_contours;
            }
            stack.pop();
            Ok((points, contours))
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/// Serialize one glyph, appending to `out`.
///
/// Instruction streams are always dropped: `instructionLength = 0` for simple
/// glyphs and `WE_HAVE_INSTRUCTIONS` cleared for composites. That is not
/// optional — the instancer also deletes `cvt `/`fpgm`/`prep`, and a glyph
/// program that survives without them grid-fits against function and CVT
/// definitions that no longer exist.
pub(crate) fn serialize_glyph(outline: &Outline, bbox: [i16; 4], out: &mut Vec<u8>) {
    match outline {
        Outline::Empty => {}
        Outline::Simple(s) => serialize_simple(s, bbox, out),
        Outline::Composite(components) => serialize_composite(components, bbox, out),
    }
}

fn write_bbox(bbox: [i16; 4], out: &mut Vec<u8>) {
    for v in bbox {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn serialize_simple(s: &SimpleOutline, bbox: [i16; 4], out: &mut Vec<u8>) {
    let n_contours = i16::try_from(s.end_pts.len()).unwrap_or(i16::MAX);
    out.extend_from_slice(&n_contours.to_be_bytes());
    write_bbox(bbox, out);
    for &e in &s.end_pts {
        out.extend_from_slice(&e.to_be_bytes());
    }
    // No instructions, ever.
    out.extend_from_slice(&0u16.to_be_bytes());

    // Encode the coordinate deltas first: the per-point flag depends on them.
    let n = s.xs.len().min(s.ys.len()).min(s.flags.len());
    let mut flags: Vec<u8> = Vec::with_capacity(n);
    let mut x_bytes: Vec<u8> = Vec::with_capacity(n * 2);
    let mut y_bytes: Vec<u8> = Vec::with_capacity(n * 2);
    let mut prev_x = 0i32;
    let mut prev_y = 0i32;
    for i in 0..n {
        let mut flag = s.flags[i] & PRESERVED_POINT_FLAGS;
        let dx = s.xs[i] - prev_x;
        prev_x = s.xs[i];
        if dx == 0 {
            flag |= X_SAME_OR_POSITIVE;
        } else if (-255..=255).contains(&dx) {
            flag |= X_SHORT_VECTOR;
            if dx > 0 {
                flag |= X_SAME_OR_POSITIVE;
            }
            x_bytes.push(dx.unsigned_abs() as u8);
        } else {
            // Defence only: with coordinates clamped to int16 the difference of
            // two of them can in principle exceed int16, which no real font
            // approaches (measured maximum |coordinate| is under 3 000).
            x_bytes.extend_from_slice(&(clamp_i16(dx) as i16).to_be_bytes());
        }
        let dy = s.ys[i] - prev_y;
        prev_y = s.ys[i];
        if dy == 0 {
            flag |= Y_SAME_OR_POSITIVE;
        } else if (-255..=255).contains(&dy) {
            flag |= Y_SHORT_VECTOR;
            if dy > 0 {
                flag |= Y_SAME_OR_POSITIVE;
            }
            y_bytes.push(dy.unsigned_abs() as u8);
        } else {
            y_bytes.extend_from_slice(&(clamp_i16(dy) as i16).to_be_bytes());
        }
        flags.push(flag);
    }

    let mut i = 0usize;
    while i < flags.len() {
        let f = flags[i];
        let mut run = 1usize;
        while i + run < flags.len() && flags[i + run] == f && run < 256 {
            run += 1;
        }
        if run >= 2 {
            out.push(f | REPEAT_FLAG);
            out.push((run - 1) as u8);
        } else {
            out.push(f);
        }
        i += run;
    }
    out.extend_from_slice(&x_bytes);
    out.extend_from_slice(&y_bytes);
}

fn serialize_composite(components: &[Component], bbox: [i16; 4], out: &mut Vec<u8>) {
    out.extend_from_slice(&(-1i16).to_be_bytes());
    write_bbox(bbox, out);

    let last = components.len().saturating_sub(1);
    for (i, comp) in components.iter().enumerate() {
        // The transform bits are derived from the `Scale` variant rather than
        // trusted from the source flags, so the flag word and the bytes that
        // follow it can never disagree.
        let mut flags = comp.flags
            & !(WE_HAVE_INSTRUCTIONS
                | WE_HAVE_A_SCALE
                | WE_HAVE_AN_X_AND_Y_SCALE
                | WE_HAVE_A_TWO_BY_TWO);
        flags |= match comp.scale {
            Scale::None => 0,
            Scale::Uniform(_) => WE_HAVE_A_SCALE,
            Scale::Xy(..) => WE_HAVE_AN_X_AND_Y_SCALE,
            Scale::Matrix(..) => WE_HAVE_A_TWO_BY_TWO,
        };
        if i == last {
            flags &= !MORE_COMPONENTS;
        } else {
            flags |= MORE_COMPONENTS;
        }
        // Recomputed, never inherited: a delta can push a byte-sized offset out
        // of range, and keeping the old flag would then write garbage.
        let needs_words = if comp.has_xy_offset() {
            !(-128..=127).contains(&comp.arg1) || !(-128..=127).contains(&comp.arg2)
        } else {
            !(0..=255).contains(&comp.arg1) || !(0..=255).contains(&comp.arg2)
        };
        if needs_words {
            flags |= ARG_1_AND_2_ARE_WORDS;
        } else {
            flags &= !ARG_1_AND_2_ARE_WORDS;
        }

        out.extend_from_slice(&flags.to_be_bytes());
        out.extend_from_slice(&comp.glyph_index.to_be_bytes());
        match (needs_words, comp.has_xy_offset()) {
            (true, true) => {
                out.extend_from_slice(&(clamp_i16(comp.arg1) as i16).to_be_bytes());
                out.extend_from_slice(&(clamp_i16(comp.arg2) as i16).to_be_bytes());
            }
            (true, false) => {
                out.extend_from_slice(&(comp.arg1.clamp(0, 0xFFFF) as u16).to_be_bytes());
                out.extend_from_slice(&(comp.arg2.clamp(0, 0xFFFF) as u16).to_be_bytes());
            }
            (false, true) => {
                out.push(comp.arg1 as i8 as u8);
                out.push(comp.arg2 as i8 as u8);
            }
            (false, false) => {
                out.push(comp.arg1 as u8);
                out.push(comp.arg2 as u8);
            }
        }
        comp.scale.write(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ot_round_is_half_toward_positive_infinity() {
        assert_eq!(ot_round(0.5), 1);
        assert_eq!(ot_round(-0.5), 0);
        assert_eq!(ot_round(-60.5), -60);
        assert_eq!(ot_round(-60.6), -61);
        assert_eq!(ot_round(2.49), 2);
        // f64::round would give -61 for -60.5; that difference is the whole point.
        assert_ne!(ot_round(-60.5), (-60.5f64).round() as i32);
    }

    #[test]
    fn iup_single_reference_fills_the_contour() {
        let sparse = vec![None, Some((3.0, -1.0)), None, None];
        let coords = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let mut out = Vec::new();
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out, vec![(3.0, -1.0); 4]);
    }

    #[test]
    fn iup_no_reference_is_all_zeros() {
        let sparse = vec![None, None, None];
        let coords = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let mut out = Vec::new();
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out, vec![(0.0, 0.0); 3]);
    }

    #[test]
    fn iup_interpolates_between_two_references() {
        // Points at x = 0, 5, 10; the ends carry deltas 0 and 10.
        let sparse = vec![Some((0.0, 0.0)), None, Some((10.0, 0.0))];
        let coords = vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)];
        let mut out = Vec::new();
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out[1].0, 5.0);
    }

    #[test]
    fn iup_clamps_outside_the_reference_span() {
        // Point 1 sits at x = 20, beyond both references (0 and 10): clamp, do
        // not extrapolate. Cyclic pairing means it is also between 2 and 0.
        let sparse = vec![Some((0.0, 0.0)), None, Some((10.0, 0.0))];
        let coords = vec![(0.0, 0.0), (20.0, 0.0), (10.0, 0.0)];
        let mut out = Vec::new();
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out[1].0, 10.0);
    }

    #[test]
    fn iup_degenerate_span_needs_agreement() {
        // Both references share x; deltas disagree ⇒ zero.
        let sparse = vec![Some((4.0, 0.0)), None, Some((6.0, 0.0))];
        let coords = vec![(1.0, 0.0), (1.0, 5.0), (1.0, 9.0)];
        let mut out = Vec::new();
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out[1].0, 0.0);
        // …and agreement ⇒ that value.
        let sparse = vec![Some((4.0, 0.0)), None, Some((4.0, 0.0))];
        iup_contour(&sparse, &coords, &mut out);
        assert_eq!(out[1].0, 4.0);
    }

    #[test]
    fn simple_glyph_round_trips_through_parse_and_serialize() {
        let outline = SimpleOutline {
            end_pts: vec![3],
            flags: vec![ON_CURVE_POINT, ON_CURVE_POINT, 0, ON_CURVE_POINT],
            xs: vec![0, 500, 500, 0],
            ys: vec![0, 0, 700, 700],
        };
        let bbox = control_box(&[(0, 0), (500, 0), (500, 700), (0, 700)]);
        let mut bytes = Vec::new();
        serialize_glyph(&Outline::Simple(outline.clone()), bbox, &mut bytes);
        let parsed = parse_glyph(&bytes).expect("parse");
        match parsed.outline {
            Outline::Simple(s) => {
                assert_eq!(s.end_pts, outline.end_pts);
                assert_eq!(s.xs, outline.xs);
                assert_eq!(s.ys, outline.ys);
                assert_eq!(s.flags, outline.flags);
            }
            _ => panic!("expected a simple glyph"),
        }
        assert_eq!(parsed.x_min, 0);
        assert_eq!(parsed.y_max, 700);
    }

    #[test]
    fn composite_arguments_promote_to_words_when_they_leave_int8() {
        let comps = vec![Component {
            flags: ARGS_ARE_XY_VALUES,
            glyph_index: 7,
            arg1: -270,
            arg2: 5,
            scale: Scale::None,
        }];
        let mut bytes = Vec::new();
        serialize_glyph(&Outline::Composite(comps), [0, 0, 1, 1], &mut bytes);
        let parsed = parse_glyph(&bytes).expect("parse");
        match parsed.outline {
            Outline::Composite(c) => {
                assert_eq!(c.len(), 1);
                assert!(c[0].flags & ARG_1_AND_2_ARE_WORDS != 0);
                assert_eq!(c[0].arg1, -270);
                assert_eq!(c[0].arg2, 5);
            }
            _ => panic!("expected a composite"),
        }
    }

    #[test]
    fn composite_arguments_demote_to_bytes_when_they_fit() {
        let comps = vec![Component {
            flags: ARGS_ARE_XY_VALUES | ARG_1_AND_2_ARE_WORDS,
            glyph_index: 3,
            arg1: -5,
            arg2: 12,
            scale: Scale::Uniform(16384),
        }];
        let mut bytes = Vec::new();
        serialize_glyph(&Outline::Composite(comps), [0, 0, 1, 1], &mut bytes);
        let parsed = parse_glyph(&bytes).expect("parse");
        match parsed.outline {
            Outline::Composite(c) => {
                assert_eq!(c[0].flags & ARG_1_AND_2_ARE_WORDS, 0);
                assert_eq!(c[0].arg1, -5);
                assert_eq!(c[0].arg2, 12);
                assert_eq!(c[0].scale, Scale::Uniform(16384));
            }
            _ => panic!("expected a composite"),
        }
    }

    #[test]
    fn contour_spans_end_with_four_phantom_singletons() {
        let outline = Outline::Simple(SimpleOutline {
            end_pts: vec![2, 5],
            flags: vec![0; 6],
            xs: vec![0; 6],
            ys: vec![0; 6],
        });
        let mut spans = Vec::new();
        contour_spans(&outline, &mut spans);
        assert_eq!(spans, vec![(0, 3), (3, 6), (6, 7), (7, 8), (8, 9), (9, 10)]);
    }

    #[test]
    fn empty_glyph_has_only_phantom_spans() {
        let mut spans = Vec::new();
        contour_spans(&Outline::Empty, &mut spans);
        assert_eq!(spans, vec![(0, 1), (1, 2), (2, 3), (3, 4)]);
    }

    #[test]
    fn decompose_detects_a_cycle() {
        let glyphs = vec![
            Outline::Composite(vec![Component {
                flags: ARGS_ARE_XY_VALUES,
                glyph_index: 1,
                arg1: 0,
                arg2: 0,
                scale: Scale::None,
            }]),
            Outline::Composite(vec![Component {
                flags: ARGS_ARE_XY_VALUES,
                glyph_index: 0,
                arg1: 0,
                arg2: 0,
                scale: Scale::None,
            }]),
        ];
        let mut stack = Vec::new();
        assert!(decompose(&glyphs, 0, &mut stack).is_err());
    }

    #[test]
    fn decompose_applies_the_row_vector_transform() {
        // A 90° rotation: xscale 0, scale01 1, scale10 -1, yscale 0.
        // Row-vector: x' = 0*x + (-1)*y, y' = 1*x + 0*y.
        let glyphs = vec![
            Outline::Simple(SimpleOutline {
                end_pts: vec![0],
                flags: vec![1],
                xs: vec![10],
                ys: vec![20],
            }),
            Outline::Composite(vec![Component {
                flags: ARGS_ARE_XY_VALUES | WE_HAVE_A_TWO_BY_TWO,
                glyph_index: 0,
                arg1: 100,
                arg2: 200,
                scale: Scale::Matrix(0, 16384, -16384, 0),
            }]),
        ];
        let mut stack = Vec::new();
        let (points, contours) = decompose(&glyphs, 1, &mut stack).expect("decompose");
        assert_eq!(contours, 1);
        assert_eq!(points, vec![(100 - 20, 200 + 10)]);
    }
}
