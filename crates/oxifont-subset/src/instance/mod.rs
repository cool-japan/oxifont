//! Static instancing: pin a variable face at one design location.
//!
//! [`instance()`] evaluates a `glyf`-flavoured variable font's outlines and
//! metrics at a fully pinned location and emits a complete **static** SFNT with
//! every variation table removed. Glyph IDs do not move, so the result feeds
//! straight into the ordinary subsetting entry points and every glyph ID the
//! caller already holds stays valid.
//!
//! The module is split along the algorithm's seams: `coords` is the
//! `fvar`/`avar` coordinate pipeline, `tuples` the `gvar` tuple variation
//! store, `outline` the glyph geometry (deltas, IUP, re-serialization),
//! `metrics` the metric and header tables, and `ivs` the `ItemVariationStore`
//! evaluation the `fvar`-without-`gvar` carve-out needs.

mod coords;
mod ivs;
mod metrics;
mod outline;
mod tuples;

use std::borrow::Cow;
use std::collections::HashMap;

use crate::glyf::{build_loca, loca_entry};
use crate::tables::{build_sfnt, get_i16, get_u16, read_table_directory_at_face, SubsetError};

use coords::{parse_avar, parse_fvar, resolve_location};
use ivs::AdvanceVariations;
use metrics::{
    build_metrics_table, patch_head, patch_hhea, patch_maxp, patch_os2, patch_post, read_metric,
    style_flags, GlyphMetrics,
};
use outline::{
    contour_spans, control_box, decompose, ot_round, parse_glyph, serialize_glyph, Component,
    Outline, SimpleOutline,
};
use tuples::{GvarStore, TupleScratch};

/// Tables deleted outright by instancing.
///
/// Every one of these is either fully consumed (`gvar`, `cvar`), meaningless
/// without `fvar` (`avar`, `STAT`), or would double-apply against a location
/// that no longer exists (`HVAR`, `VVAR`, `MVAR`). `DSIG` is a signature over
/// bytes that have just been rewritten, so it is invalid by construction; the
/// hint tables go because the per-glyph instruction streams do, and a glyph
/// program without its `cvt `/`fpgm` is worse than no hinting at all.
const DROPPED_TABLES: &[&[u8; 4]] = &[
    b"fvar", b"gvar", b"avar", b"cvar", b"HVAR", b"VVAR", b"MVAR", b"STAT", b"DSIG", b"VORG",
    b"cvt ", b"fpgm", b"prep", b"gasp",
];

/// Tables the instancer rebuilds, and therefore must not copy verbatim.
const REBUILT_TABLES: &[&[u8; 4]] = &[
    b"glyf", b"loca", b"hmtx", b"hhea", b"head", b"maxp", b"vmtx", b"vhea", b"OS/2", b"post",
];

/// Produce a **static** font pinned at `coords` — the variable face's outlines
/// and metrics evaluated at one location, with every variation table removed.
///
/// `font_data` may be a plain per-face SFNT or a `ttcf` collection;
/// `face_index` selects the face exactly as [`crate::subset_font_at_face`] does.
/// The result is always a single-face SFNT at offset 0, so a later
/// [`crate::subset_with_gid_set_at_face_mapped`] over it uses `face_index = 0`.
///
/// `coords` is `(axis tag, value in USER units)` — the same scale `fvar` records
/// (`wght = 700.0`, `wdth = 87.5`), **not** normalised F2Dot14. Values outside
/// an axis's `[min, max]` are clamped, per the OpenType instancing model. Axes
/// absent from `coords` are pinned at their `fvar` default; a repeated tag takes
/// its last value.
///
/// # Glyph-ID identity
/// The output has the **same glyph count and the same glyph IDs** as the input
/// face. Nothing is renumbered, so `cmap`, `GSUB`, `GPOS`, `GDEF`, `kern`,
/// `COLR`, `MATH`, `sbix`, … are carried over verbatim and stay valid.
///
/// # What is dropped
/// `fvar`, `avar`, `gvar`, `cvar`, `HVAR`, `VVAR`, `MVAR`, `STAT` — the face is
/// no longer variable — plus `DSIG` (a signature over rewritten bytes) and
/// `cvt `/`fpgm`/`prep`/`gasp`, whose hinting programs were tuned against the
/// default master. Per-glyph instruction streams go with them: keeping glyph
/// programs while deleting the tables they call into produces garbage under a
/// hinting rasteriser.
///
/// # Example
/// ```no_run
/// use std::collections::BTreeSet;
///
/// let font_data = std::fs::read("SomeVariable.ttf")?;
/// // Pin Bold, then subset the static bytes as usual (face 0 either way).
/// let bold = oxifont_subset::instance(&font_data, 0, &[(*b"wght", 700.0)])?;
/// let gids: BTreeSet<u16> = [0u16, 36, 37].into_iter().collect();
/// let subset = oxifont_subset::subset_by_gids(&bold, &gids)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// - [`SubsetError::Unsupported`] — the face has no `fvar` axes (call the
///   subsetting entry points directly), or its outlines are `CFF`/`CFF2`.
/// - [`SubsetError::UnknownAxis`] — a tag in `coords` has no `fvar` axis.
/// - [`SubsetError::FaceIndexOutOfRange`] — as [`crate::face_count`].
/// - [`SubsetError::TableMissing`] / [`SubsetError::InvalidFont`] — a required
///   table is absent, or `fvar`/`avar`/`gvar`/`glyf`/`loca` is malformed.
pub fn instance(
    font_data: &[u8],
    face_index: u32,
    coords: &[([u8; 4], f32)],
) -> Result<Vec<u8>, SubsetError> {
    let tables = read_table_directory_at_face(font_data, face_index)?;
    instance_tables(&tables, coords)
}

/// Everything [`instance`] does once the table directory has been read.
fn instance_tables(
    tables: &HashMap<[u8; 4], &[u8]>,
    coords: &[([u8; 4], f32)],
) -> Result<Vec<u8>, SubsetError> {
    let get = |tag: &[u8; 4]| -> Result<&[u8], SubsetError> {
        tables
            .get(tag)
            .copied()
            .ok_or(SubsetError::TableMissing(*tag))
    };

    let fvar = tables
        .get(b"fvar")
        .copied()
        .ok_or(SubsetError::Unsupported("face has no fvar axes"))?;
    if tables.contains_key(b"CFF2") {
        return Err(SubsetError::Unsupported(
            "CFF2 charstrings cannot be instanced",
        ));
    }
    if !tables.contains_key(b"glyf") && tables.contains_key(b"CFF ") {
        return Err(SubsetError::Unsupported(
            "CFF charstrings cannot be instanced",
        ));
    }

    let axes = parse_fvar(fvar)?;
    let avar = match tables.get(b"avar") {
        Some(&data) => Some(parse_avar(data)?),
        None => None,
    };
    let location = resolve_location(axes, avar.as_ref(), coords)?;

    let head = get(b"head")?;
    if head.len() < 54 {
        return Err(SubsetError::InvalidFont("head table too short".into()));
    }
    let loca_format = get_i16(head, 50)
        .ok_or_else(|| SubsetError::InvalidFont("head.indexToLocFormat missing".into()))?;
    let maxp = get(b"maxp")?;
    let num_glyphs = get_u16(maxp, 4)
        .ok_or_else(|| SubsetError::InvalidFont("maxp.numGlyphs missing".into()))?;
    let hhea = get(b"hhea")?;
    let hmtx = get(b"hmtx")?;
    let glyf = get(b"glyf")?;
    let loca = get(b"loca")?;

    let num_h_metrics = usize::from(get_u16(hhea, 34).unwrap_or(0));
    let vmtx = tables.get(b"vmtx").copied();
    let vhea = tables.get(b"vhea").copied();
    let num_v_metrics = vhea
        .and_then(|v| get_u16(v, 34))
        .map(usize::from)
        .unwrap_or(0);
    let ascender = f64::from(get_i16(hhea, 4).unwrap_or(0));
    let descender = f64::from(get_i16(hhea, 6).unwrap_or(0));

    let gvar = match tables.get(b"gvar") {
        Some(&data) => Some(GvarStore::parse(data, location.axes.len())?),
        None => None,
    };
    // The one place HVAR/VVAR are read rather than deleted unread: without gvar
    // there are no phantom points to derive advances from.
    let (hvar, vvar) = if gvar.is_none() {
        (
            tables
                .get(b"HVAR")
                .and_then(|&d| AdvanceVariations::parse(d)),
            tables
                .get(b"VVAR")
                .and_then(|&d| AdvanceVariations::parse(d)),
        )
    } else {
        (None, None)
    };

    // ---------------------------------------------------------------------
    // Pass 1 — instance every glyph's geometry and remember its phantoms.
    // Iterate 0..numGlyphs, never the glyf records: an empty glyph carries no
    // outline but its advance still varies.
    // ---------------------------------------------------------------------
    let glyph_count = usize::from(num_glyphs);
    let mut outlines: Vec<Outline> = Vec::with_capacity(glyph_count);
    let mut phantoms: Vec<[(f64, f64); 4]> = Vec::with_capacity(glyph_count);
    let mut source_metrics: Vec<(u16, i16, u16, i16)> = Vec::with_capacity(glyph_count);

    let mut scratch = TupleScratch::default();
    let mut default_pts: Vec<(f64, f64)> = Vec::new();
    let mut acc: Vec<(f64, f64)> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();

    for gid in 0..num_glyphs {
        let (start, end) = loca_entry(loca, loca_format, gid)
            .ok_or_else(|| SubsetError::InvalidFont(format!("loca entry {gid} out of range")))?;
        let glyph_bytes = if start >= end {
            &[][..]
        } else {
            glyf.get(start..end).ok_or_else(|| {
                SubsetError::InvalidFont(format!("glyf entry for glyph {gid} leaves the table"))
            })?
        };
        let source = parse_glyph(glyph_bytes)
            .ok_or_else(|| SubsetError::InvalidFont(format!("glyph {gid} is malformed")))?;

        let (advance, lsb) = read_metric(hmtx, num_h_metrics, usize::from(gid));
        let (advance_height, tsb) = match vmtx {
            Some(v) => read_metric(v, num_v_metrics, usize::from(gid)),
            None => {
                // Synthesized so that pp3.y = ascender and pp4.y = descender.
                let ah = ot_round(ascender - descender).clamp(0, i32::from(u16::MAX)) as u16;
                let tsb = ot_round(ascender - f64::from(source.y_max));
                (
                    ah,
                    tsb.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                )
            }
        };
        source_metrics.push((advance, lsb, advance_height, tsb));

        let n_real = source.outline.real_point_count();
        default_pts.clear();
        default_pts.reserve(n_real + 4);
        match &source.outline {
            Outline::Simple(s) => {
                for (x, y) in s.xs.iter().zip(s.ys.iter()) {
                    default_pts.push((f64::from(*x), f64::from(*y)));
                }
            }
            Outline::Composite(components) => {
                for comp in components {
                    if comp.has_xy_offset() {
                        default_pts.push((f64::from(comp.arg1), f64::from(comp.arg2)));
                    } else {
                        // Point-matched: no offset to vary, and any delta aimed
                        // at it is discarded when the component is written back.
                        default_pts.push((0.0, 0.0));
                    }
                }
            }
            Outline::Empty => {}
        }
        let pp1 = (f64::from(source.x_min - i32::from(lsb)), 0.0);
        let pp2 = (pp1.0 + f64::from(advance), 0.0);
        let pp3 = (0.0, f64::from(source.y_max + i32::from(tsb)));
        let pp4 = (0.0, pp3.1 - f64::from(advance_height));
        default_pts.push(pp1);
        default_pts.push(pp2);
        default_pts.push(pp3);
        default_pts.push(pp4);

        acc.clear();
        acc.resize(default_pts.len(), (0.0, 0.0));
        if let Some(store) = &gvar {
            contour_spans(&source.outline, &mut spans);
            // A malformed block leaves `acc` at zero: this glyph keeps its
            // default outline, which is far better than failing the instance.
            store.accumulate(
                gid,
                &location.normalized,
                &default_pts,
                &spans,
                &mut acc,
                &mut scratch,
            );
        }

        let instanced = apply_deltas(&source.outline, &default_pts, &acc, n_real);
        let mut pp = [(0.0, 0.0); 4];
        for (slot, (d, a)) in pp
            .iter_mut()
            .zip(default_pts.iter().skip(n_real).zip(acc.iter().skip(n_real)))
        {
            *slot = (d.0 + a.0, d.1 + a.1);
        }
        outlines.push(instanced);
        phantoms.push(pp);
    }

    // ---------------------------------------------------------------------
    // Pass 2 — bounding boxes, `maxp` counters, and metrics.
    // Composite boxes need the *instanced* component glyphs, so they cannot be
    // computed until every glyph has been through pass 1.
    // ---------------------------------------------------------------------
    let mut boxes: Vec<[i16; 4]> = Vec::with_capacity(glyph_count);
    let mut empty: Vec<bool> = Vec::with_capacity(glyph_count);
    let mut max_points = 0u16;
    let mut max_contours = 0u16;
    let mut max_composite_points = 0u16;
    let mut max_composite_contours = 0u16;
    let mut stack: Vec<u16> = Vec::with_capacity(8);

    for gid in 0..num_glyphs {
        let Some(outline) = outlines.get(usize::from(gid)) else {
            break;
        };
        match outline {
            Outline::Empty => {
                boxes.push([0, 0, 0, 0]);
                empty.push(true);
            }
            Outline::Simple(s) => {
                let points: Vec<(i32, i32)> =
                    s.xs.iter().copied().zip(s.ys.iter().copied()).collect();
                boxes.push(control_box(&points));
                empty.push(false);
                max_points = max_points.max(u16::try_from(points.len()).unwrap_or(u16::MAX));
                max_contours = max_contours.max(u16::try_from(s.end_pts.len()).unwrap_or(u16::MAX));
            }
            Outline::Composite(_) => {
                stack.clear();
                let (points, contours) = decompose(&outlines, gid, &mut stack)?;
                boxes.push(control_box(&points));
                empty.push(points.is_empty());
                max_composite_points =
                    max_composite_points.max(u16::try_from(points.len()).unwrap_or(u16::MAX));
                max_composite_contours =
                    max_composite_contours.max(u16::try_from(contours).unwrap_or(u16::MAX));
            }
        }
    }

    let mut glyph_metrics: Vec<GlyphMetrics> = Vec::with_capacity(glyph_count);
    for (gid, ((pp, bbox), source)) in phantoms
        .iter()
        .zip(boxes.iter())
        .zip(source_metrics.iter())
        .enumerate()
    {
        let gid = u16::try_from(gid).unwrap_or(u16::MAX);
        // Round the *difference*, once — not the difference of two roundings.
        let mut advance = ot_round(pp[1].0 - pp[0].0).max(0);
        let mut advance_height = ot_round(pp[2].1 - pp[3].1).max(0);
        // With no `gvar` there are no phantoms to move, so the advance deltas
        // come from `HVAR`/`VVAR` instead — the only place either is read.
        if let Some(hvar) = &hvar {
            let base = f64::from(source.0);
            advance = ot_round(base + hvar.advance_delta(gid, &location.normalized)).max(0);
        }
        if let Some(vvar) = &vvar {
            let base = f64::from(source.2);
            advance_height = ot_round(base + vvar.advance_delta(gid, &location.normalized)).max(0);
        }
        let lsb = i32::from(bbox[0]) - ot_round(pp[0].0);
        let tsb = ot_round(pp[2].1) - i32::from(bbox[3]);
        glyph_metrics.push(GlyphMetrics {
            advance: advance.clamp(0, i32::from(u16::MAX)) as u16,
            lsb: lsb.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            advance_height: advance_height.clamp(0, i32::from(u16::MAX)) as u16,
            tsb: tsb.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        });
    }

    // ---------------------------------------------------------------------
    // Pass 3 — serialize `glyf`/`loca` and the header tables.
    // ---------------------------------------------------------------------
    let mut new_glyf: Vec<u8> = Vec::with_capacity(glyf.len());
    let mut offsets: Vec<u32> = Vec::with_capacity(glyph_count + 1);
    for (outline, bbox) in outlines.iter().zip(boxes.iter()) {
        offsets.push(new_glyf.len() as u32);
        serialize_glyph(outline, *bbox, &mut new_glyf);
        // `glyf` entries stay word-aligned so the short `loca` format's
        // offset/2 encoding remains exact rather than truncating.
        if !new_glyf.len().is_multiple_of(2) {
            new_glyf.push(0);
        }
    }
    offsets.push(new_glyf.len() as u32);
    let (new_loca, new_loca_format) = build_loca(&offsets);

    let font_box = union_box(&boxes, &empty);
    let (bold, italic) = style_flags(&location);

    let new_head = patch_head(head, font_box, new_loca_format, bold, italic);
    let new_hhea = patch_hhea(hhea, &glyph_metrics, &boxes, &empty, false);
    let new_hmtx = build_metrics_table(&glyph_metrics, false);
    let new_maxp = patch_maxp(
        maxp,
        max_points,
        max_contours,
        max_composite_points,
        max_composite_contours,
    );
    let new_vhea = vhea.map(|v| patch_hhea(v, &glyph_metrics, &boxes, &empty, true));
    let new_vmtx = vmtx.map(|_| build_metrics_table(&glyph_metrics, true));
    let new_os2 = tables
        .get(b"OS/2")
        .map(|&d| patch_os2(d, &location, bold, italic));
    let new_post = tables.get(b"post").map(|&d| patch_post(d, &location));

    // ---------------------------------------------------------------------
    // Output assembly. Tags are visited in sorted order so the result is a pure
    // function of the inputs even though the directory arrives in a HashMap.
    // ---------------------------------------------------------------------
    let mut verbatim_tags: Vec<[u8; 4]> = tables
        .keys()
        .copied()
        .filter(|tag| !DROPPED_TABLES.contains(&tag) && !REBUILT_TABLES.contains(&tag))
        .collect();
    verbatim_tags.sort_unstable();

    let mut output: Vec<([u8; 4], Cow<'_, [u8]>)> = Vec::with_capacity(verbatim_tags.len() + 10);
    for tag in verbatim_tags {
        if let Some(&data) = tables.get(&tag) {
            output.push((tag, Cow::Borrowed(data)));
        }
    }
    output.push((*b"glyf", Cow::Owned(new_glyf)));
    output.push((*b"loca", Cow::Owned(new_loca)));
    output.push((*b"hmtx", Cow::Owned(new_hmtx)));
    output.push((*b"hhea", Cow::Owned(new_hhea)));
    output.push((*b"head", Cow::Owned(new_head)));
    output.push((*b"maxp", Cow::Owned(new_maxp)));
    if let Some(v) = new_vhea {
        output.push((*b"vhea", Cow::Owned(v)));
    }
    if let Some(v) = new_vmtx {
        output.push((*b"vmtx", Cow::Owned(v)));
    }
    if let Some(v) = new_os2 {
        output.push((*b"OS/2", Cow::Owned(v)));
    }
    if let Some(v) = new_post {
        output.push((*b"post", Cow::Owned(v)));
    }

    Ok(build_sfnt(&output))
}

/// Add the accumulated deltas to the default outline and round, once.
fn apply_deltas(
    source: &Outline,
    default_pts: &[(f64, f64)],
    acc: &[(f64, f64)],
    n_real: usize,
) -> Outline {
    match source {
        Outline::Empty => Outline::Empty,
        Outline::Simple(s) => {
            let mut xs = Vec::with_capacity(n_real);
            let mut ys = Vec::with_capacity(n_real);
            for (d, a) in default_pts.iter().zip(acc.iter()).take(n_real) {
                xs.push(clamp_coord(ot_round(d.0 + a.0)));
                ys.push(clamp_coord(ot_round(d.1 + a.1)));
            }
            Outline::Simple(SimpleOutline {
                end_pts: s.end_pts.clone(),
                flags: s.flags.clone(),
                xs,
                ys,
            })
        }
        Outline::Composite(components) => {
            let mut out = Vec::with_capacity(components.len());
            for (i, comp) in components.iter().enumerate() {
                let moved = default_pts
                    .get(i)
                    .zip(acc.get(i))
                    .map(|(d, a)| (ot_round(d.0 + a.0), ot_round(d.1 + a.1)));
                let (arg1, arg2) = if let (true, Some(moved)) = (comp.has_xy_offset(), moved) {
                    moved
                } else {
                    // Point-matched components have no offset: the delta is
                    // discarded, not written back as a bogus point index.
                    (comp.arg1, comp.arg2)
                };
                out.push(Component {
                    flags: comp.flags,
                    glyph_index: comp.glyph_index,
                    arg1,
                    arg2,
                    scale: comp.scale,
                });
            }
            Outline::Composite(out)
        }
    }
}

fn clamp_coord(v: i32) -> i32 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

/// `head`'s font box: the union over non-empty glyphs, `[0; 4]` when there are
/// none.
fn union_box(boxes: &[[i16; 4]], empty: &[bool]) -> [i16; 4] {
    let mut result: Option<[i16; 4]> = None;
    for (i, b) in boxes.iter().enumerate() {
        if empty.get(i).copied().unwrap_or(true) {
            continue;
        }
        result = Some(match result {
            None => *b,
            Some(r) => [
                r[0].min(b[0]),
                r[1].min(b[1]),
                r[2].max(b[2]),
                r[3].max(b[3]),
            ],
        });
    }
    result.unwrap_or([0, 0, 0, 0])
}
