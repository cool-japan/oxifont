//! Metric and header tables rebuilt from the instanced glyphs.
//!
//! `hmtx`/`vmtx` come from the phantom points, not from `HVAR`/`VVAR`: the
//! outline walk produces them for free, and the two sources are independently
//! authored, so consulting `HVAR` after having moved the phantoms would let a
//! font disagree with itself. `HVAR` is read in exactly one place — the
//! `fvar`-without-`gvar` carve-out, where there are no phantoms to consult.

use super::coords::Location;
use super::outline::ot_round;
use crate::tables::{get_i16, get_u16, set_i16, set_u16};

/// `head` field offsets.
const HEAD_X_MIN: usize = 36;
const HEAD_MAC_STYLE: usize = 44;
const HEAD_INDEX_TO_LOC_FORMAT: usize = 50;

/// `hhea` / `vhea` field offsets (identical layouts).
const HHEA_ADVANCE_MAX: usize = 10;
const HHEA_MIN_LEADING_SB: usize = 12;
const HHEA_MIN_TRAILING_SB: usize = 14;
const HHEA_MAX_EXTENT: usize = 16;
const HHEA_NUM_METRICS: usize = 34;

/// `maxp` version 1.0 field offsets.
const MAXP_MAX_POINTS: usize = 6;
const MAXP_MAX_CONTOURS: usize = 8;
const MAXP_MAX_COMPOSITE_POINTS: usize = 10;
const MAXP_MAX_COMPOSITE_CONTOURS: usize = 12;
const MAXP_MAX_ZONES: usize = 14;
const MAXP_MAX_TWILIGHT_POINTS: usize = 16;
const MAXP_MAX_STORAGE: usize = 18;
const MAXP_MAX_FUNCTION_DEFS: usize = 20;
const MAXP_MAX_INSTRUCTION_DEFS: usize = 22;
const MAXP_MAX_STACK_ELEMENTS: usize = 24;
const MAXP_MAX_SIZE_OF_INSTRUCTIONS: usize = 26;

/// `OS/2` field offsets.
const OS2_WEIGHT_CLASS: usize = 4;
const OS2_WIDTH_CLASS: usize = 6;
const OS2_FS_SELECTION: usize = 62;

/// `fsSelection` / `macStyle` bits.
const FS_SELECTION_ITALIC: u16 = 0x0001;
const FS_SELECTION_BOLD: u16 = 0x0020;
const FS_SELECTION_REGULAR: u16 = 0x0040;
const MAC_STYLE_BOLD: u16 = 0x0001;
const MAC_STYLE_ITALIC: u16 = 0x0002;

/// One glyph's instanced metrics.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GlyphMetrics {
    /// `hmtx.advanceWidth`.
    pub(crate) advance: u16,
    /// `hmtx.lsb`.
    pub(crate) lsb: i16,
    /// `vmtx.advanceHeight`.
    pub(crate) advance_height: u16,
    /// `vmtx.tsb`.
    pub(crate) tsb: i16,
}

/// Read `(advance, sideBearing)` for `gid` out of an `hmtx`/`vmtx` table.
///
/// Glyphs at or beyond `num_long` share the last long record's advance and take
/// their bearing from the trailing array.
pub(crate) fn read_metric(table: &[u8], num_long: usize, gid: usize) -> (u16, i16) {
    if num_long == 0 {
        return (0, 0);
    }
    if gid < num_long {
        let advance = get_u16(table, gid * 4).unwrap_or(0);
        let sb = get_i16(table, gid * 4 + 2).unwrap_or(0);
        (advance, sb)
    } else {
        let advance = get_u16(table, (num_long - 1) * 4).unwrap_or(0);
        let sb = get_i16(table, num_long * 4 + (gid - num_long) * 2).unwrap_or(0);
        (advance, sb)
    }
}

/// Serialize a full-length `hmtx` (or `vmtx`): one long record per glyph.
///
/// No trailing-run compression: the subsetting pass that normally follows
/// rewrites the table and sets `numberOfHMetrics` unconditionally, so any
/// compression here would be thrown away, and a full-length table is trivially
/// indexable.
pub(crate) fn build_metrics_table(metrics: &[GlyphMetrics], vertical: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(metrics.len() * 4);
    for m in metrics {
        let (advance, sb) = if vertical {
            (m.advance_height, m.tsb)
        } else {
            (m.advance, m.lsb)
        };
        out.extend_from_slice(&advance.to_be_bytes());
        out.extend_from_slice(&sb.to_be_bytes());
    }
    out
}

/// Rebuild `hhea`/`vhea` from the instanced metrics and boxes.
///
/// `ascender`/`descender`/`lineGap` and the caret fields are kept verbatim:
/// `gvar` does not vary them (`MVAR` does, and `MVAR` is dropped unapplied).
/// The four extent fields are recomputed because they are wrong by
/// construction after instancing and the subsetting pass copies `hhea` through.
pub(crate) fn patch_hhea(
    source: &[u8],
    metrics: &[GlyphMetrics],
    boxes: &[[i16; 4]],
    empty: &[bool],
    vertical: bool,
) -> Vec<u8> {
    let mut out = source.to_vec();
    let num_glyphs = u16::try_from(metrics.len()).unwrap_or(u16::MAX);
    set_u16(&mut out, HHEA_NUM_METRICS, num_glyphs);

    let mut advance_max: u16 = 0;
    let mut min_leading: i32 = i32::MAX;
    let mut min_trailing: i32 = i32::MAX;
    let mut max_extent: i32 = i32::MIN;

    for (i, m) in metrics.iter().enumerate() {
        let (advance, sb) = if vertical {
            (m.advance_height, m.tsb)
        } else {
            (m.advance, m.lsb)
        };
        advance_max = advance_max.max(advance);
        if empty.get(i).copied().unwrap_or(true) {
            continue;
        }
        let bbox = boxes.get(i).copied().unwrap_or([0, 0, 0, 0]);
        let extent_span = if vertical {
            i32::from(bbox[3]) - i32::from(bbox[1])
        } else {
            i32::from(bbox[2]) - i32::from(bbox[0])
        };
        let sb = i32::from(sb);
        min_leading = min_leading.min(sb);
        min_trailing = min_trailing.min(i32::from(advance) - (sb + extent_span));
        max_extent = max_extent.max(sb + extent_span);
    }

    if min_leading == i32::MAX {
        min_leading = 0;
        min_trailing = 0;
        max_extent = 0;
    }
    set_u16(&mut out, HHEA_ADVANCE_MAX, advance_max);
    set_i16(&mut out, HHEA_MIN_LEADING_SB, clamp_i16(min_leading));
    set_i16(&mut out, HHEA_MIN_TRAILING_SB, clamp_i16(min_trailing));
    set_i16(&mut out, HHEA_MAX_EXTENT, clamp_i16(max_extent));
    out
}

/// Update `head`: the font box, `indexToLocFormat`, `macStyle`, and a zeroed
/// `checkSumAdjustment` for the sfnt writer to repair.
pub(crate) fn patch_head(
    source: &[u8],
    font_box: [i16; 4],
    loca_format: i16,
    bold: bool,
    italic: bool,
) -> Vec<u8> {
    let mut out = source.to_vec();
    if out.len() >= 12 {
        out[8..12].fill(0);
    }
    for (i, v) in font_box.iter().enumerate() {
        set_i16(&mut out, HEAD_X_MIN + i * 2, *v);
    }
    set_i16(&mut out, HEAD_INDEX_TO_LOC_FORMAT, loca_format);
    let mut mac_style = get_u16(&out, HEAD_MAC_STYLE).unwrap_or(0);
    mac_style &= !(MAC_STYLE_BOLD | MAC_STYLE_ITALIC);
    if bold {
        mac_style |= MAC_STYLE_BOLD;
    }
    if italic {
        mac_style |= MAC_STYLE_ITALIC;
    }
    set_u16(&mut out, HEAD_MAC_STYLE, mac_style);
    out
}

/// Recompute the `maxp` glyph-shape counters and zero every hinting counter,
/// which must move together with the dropped instruction streams.
pub(crate) fn patch_maxp(
    source: &[u8],
    max_points: u16,
    max_contours: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
) -> Vec<u8> {
    let mut out = source.to_vec();
    // Version 0.5 (`maxp` for CFF outlines) stops after numGlyphs.
    if out.len() < 32 {
        return out;
    }
    set_u16(&mut out, MAXP_MAX_POINTS, max_points);
    set_u16(&mut out, MAXP_MAX_CONTOURS, max_contours);
    set_u16(&mut out, MAXP_MAX_COMPOSITE_POINTS, max_composite_points);
    set_u16(
        &mut out,
        MAXP_MAX_COMPOSITE_CONTOURS,
        max_composite_contours,
    );
    set_u16(&mut out, MAXP_MAX_ZONES, 2);
    set_u16(&mut out, MAXP_MAX_TWILIGHT_POINTS, 0);
    set_u16(&mut out, MAXP_MAX_STORAGE, 0);
    set_u16(&mut out, MAXP_MAX_FUNCTION_DEFS, 0);
    set_u16(&mut out, MAXP_MAX_INSTRUCTION_DEFS, 0);
    set_u16(&mut out, MAXP_MAX_STACK_ELEMENTS, 0);
    set_u16(&mut out, MAXP_MAX_SIZE_OF_INSTRUCTIONS, 0);
    out
}

/// The style the pinned location describes: `(bold, italic)`.
///
/// `slnt` is negative for a forward lean, which matches `post.italicAngle`'s
/// sign convention — it is not negated anywhere.
pub(crate) fn style_flags(location: &Location) -> (bool, bool) {
    let bold = location
        .user_value(b"wght")
        .is_some_and(|w| weight_class(w) >= 600);
    let italic = location.user_value(b"ital").is_some_and(|v| v >= 0.5)
        || location.user_value(b"slnt").is_some_and(|v| v != 0.0);
    (bold, italic)
}

fn weight_class(wght: f32) -> u16 {
    let rounded = ot_round(f64::from(wght));
    rounded.clamp(1, 1000) as u16
}

/// `usWidthClass` from a `wdth` percentage, by the fixed piecewise-linear table
/// fontTools uses (`50 → 1`, `100 → 5`, `200 → 9`).
fn width_class(wdth: f32) -> u16 {
    const POINTS: [(f32, f32); 9] = [
        (50.0, 1.0),
        (62.5, 2.0),
        (75.0, 3.0),
        (87.5, 4.0),
        (100.0, 5.0),
        (112.5, 6.0),
        (125.0, 7.0),
        (150.0, 8.0),
        (200.0, 9.0),
    ];
    if wdth <= POINTS[0].0 {
        return 1;
    }
    for w in POINTS.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if wdth <= x1 {
            let t = f64::from((wdth - x0) / (x1 - x0));
            let v = f64::from(y0) + t * f64::from(y1 - y0);
            return ot_round(v).clamp(1, 9) as u16;
        }
    }
    9
}

/// Update `OS/2` weight/width class and `fsSelection` from the pinned location.
///
/// Hygiene rather than a behaviour change for a PDF embed, but a
/// `usWeightClass` of 100 on a font whose outlines are Bold is exactly the
/// defect static instancing exists to remove.
pub(crate) fn patch_os2(source: &[u8], location: &Location, bold: bool, italic: bool) -> Vec<u8> {
    let mut out = source.to_vec();
    if let Some(wght) = location.user_value(b"wght") {
        set_u16(&mut out, OS2_WEIGHT_CLASS, weight_class(wght));
    }
    if let Some(wdth) = location.user_value(b"wdth") {
        set_u16(&mut out, OS2_WIDTH_CLASS, width_class(wdth));
    }
    if let Some(mut fs) = get_u16(&out, OS2_FS_SELECTION) {
        fs &= !(FS_SELECTION_BOLD | FS_SELECTION_ITALIC);
        if bold {
            fs |= FS_SELECTION_BOLD;
        }
        if italic {
            fs |= FS_SELECTION_ITALIC;
        }
        if fs & (FS_SELECTION_BOLD | FS_SELECTION_ITALIC) != 0 {
            fs &= !FS_SELECTION_REGULAR;
        }
        set_u16(&mut out, OS2_FS_SELECTION, fs);
    }
    out
}

/// Update `post.italicAngle` from a pinned `slnt` axis.
pub(crate) fn patch_post(source: &[u8], location: &Location) -> Vec<u8> {
    let mut out = source.to_vec();
    if let Some(slnt) = location.user_value(b"slnt") {
        let angle = slnt.clamp(-90.0, 90.0);
        let fixed = ot_round(f64::from(angle) * 65536.0);
        if let Some(slot) = out.get_mut(4..8) {
            slot.copy_from_slice(&fixed.to_be_bytes());
        }
    }
    out
}

fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_class_table_endpoints_and_interpolation() {
        assert_eq!(width_class(50.0), 1);
        assert_eq!(width_class(20.0), 1);
        assert_eq!(width_class(75.0), 3);
        assert_eq!(width_class(100.0), 5);
        assert_eq!(width_class(200.0), 9);
        assert_eq!(width_class(500.0), 9);
        // Halfway between 87.5 → 4 and 100 → 5 rounds up (half toward +∞).
        assert_eq!(width_class(93.75), 5);
    }

    #[test]
    fn weight_class_clamps_to_the_os2_domain() {
        assert_eq!(weight_class(700.0), 700);
        assert_eq!(weight_class(0.0), 1);
        assert_eq!(weight_class(5000.0), 1000);
        assert_eq!(weight_class(399.5), 400);
    }

    #[test]
    fn short_hmtx_reads_do_not_panic() {
        assert_eq!(read_metric(&[], 0, 5), (0, 0));
        assert_eq!(read_metric(&[0, 10, 0, 1], 1, 0), (10, 1));
        // Beyond the long records: the advance repeats, the bearing comes from
        // the trailing array (absent here, so zero).
        assert_eq!(read_metric(&[0, 10, 0, 1], 1, 9), (10, 0));
    }
}
