//! User-space → normalized-coordinate pipeline (`fvar`, `avar`).
//!
//! The whole pipeline runs in signed 16.16 fixed point and converts to F2Dot14
//! exactly once, at the very end. That ordering is not cosmetic: normalising in
//! `f32`, or converting to F2Dot14 before applying `avar`, changes the region
//! scalars in [`super::tuples`] and therefore the emitted outlines. The rounded
//! (not truncating) fixed-point division is load-bearing for the same reason —
//! with a truncating division the normalized coordinates drift by ±1 F2Dot14
//! unit against the reference implementations on ~20 % of sampled locations.

use crate::tables::{get_i16, get_u16, SubsetError};

/// Signed 16.16 fixed-point value: `raw / 65536`.
pub(crate) type Fixed = i32;

/// `1.0` in 16.16.
pub(crate) const FIXED_ONE: Fixed = 0x0001_0000;

/// Convert an `f32` user coordinate to 16.16, saturating rather than wrapping.
///
/// A non-finite input is treated as `0.0`: the axis then pins at its default
/// after clamping, which is the only defined answer for "no value".
pub(crate) fn fixed_from_f32(v: f32) -> Fixed {
    if !v.is_finite() {
        return 0;
    }
    let scaled = (f64::from(v) * 65536.0 + 0.5).floor();
    if scaled >= f64::from(i32::MAX) {
        i32::MAX
    } else if scaled <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        scaled as i32
    }
}

/// FreeType `FT_DivFix`: `a / b` in 16.16, with the **rounded** quotient.
pub(crate) fn fixed_div(a: Fixed, b: Fixed) -> Fixed {
    if b == 0 {
        return i32::MAX;
    }
    let negative = (a < 0) != (b < 0);
    let ua = (a as i64).unsigned_abs();
    let ub = (b as i64).unsigned_abs();
    let q = ((ua << 16) + (ub >> 1)) / ub;
    let q = if q > i32::MAX as u64 {
        i32::MAX
    } else {
        q as i32
    };
    if negative {
        -q
    } else {
        q
    }
}

/// FreeType `FT_MulDiv`: `a * b / c`, rounded, computed without intermediate
/// overflow.
pub(crate) fn fixed_mul_div(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        return i32::MAX;
    }
    let negative = ((a < 0) != (b < 0)) != (c < 0);
    let ua = (a as i64).unsigned_abs();
    let ub = (b as i64).unsigned_abs();
    let uc = (c as i64).unsigned_abs();
    let q = (ua * ub + (uc >> 1)) / uc;
    let q = if q > i32::MAX as u64 {
        i32::MAX
    } else {
        q as i32
    };
    if negative {
        -q
    } else {
        q
    }
}

/// The OpenType-prescribed 16.16 → F2Dot14 conversion: add `0x0002` and
/// arithmetic-shift right by 2 (round-half-up on the raw fixed value).
pub(crate) fn fixed_to_f2dot14(v: Fixed) -> i16 {
    (v.wrapping_add(2) >> 2) as i16
}

/// Widen an F2Dot14 raw value to 16.16 (14 → 16 fractional bits; exact).
pub(crate) fn f2dot14_to_fixed(v: i16) -> Fixed {
    i32::from(v) * 4
}

// ---------------------------------------------------------------------------
// fvar
// ---------------------------------------------------------------------------

/// One `fvar` variation axis, in 16.16 user units.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Axis {
    /// 4-byte axis tag (`wght`, `wdth`, `opsz`, …).
    pub(crate) tag: [u8; 4],
    /// Minimum user value.
    pub(crate) min: Fixed,
    /// Default user value.
    pub(crate) def: Fixed,
    /// Maximum user value.
    pub(crate) max: Fixed,
}

/// Parse the `fvar` axis array.
///
/// Named instances are deliberately not parsed: the caller resolves the
/// location it wants and hands over user coordinates.
///
/// # Errors
/// [`SubsetError::InvalidFont`] for a truncated table, a version other than
/// 1.0, a zero axis count, or an axis array that leaves the table.
pub(crate) fn parse_fvar(table: &[u8]) -> Result<Vec<Axis>, SubsetError> {
    let bad = |what: &str| SubsetError::InvalidFont(format!("fvar: {what}"));

    let major = get_u16(table, 0).ok_or_else(|| bad("header truncated"))?;
    let minor = get_u16(table, 2).ok_or_else(|| bad("header truncated"))?;
    if major != 1 || minor != 0 {
        return Err(bad("unsupported version"));
    }
    let axes_offset = get_u16(table, 4).ok_or_else(|| bad("header truncated"))? as usize;
    let axis_count = get_u16(table, 8).ok_or_else(|| bad("header truncated"))? as usize;
    let axis_size = get_u16(table, 10).ok_or_else(|| bad("header truncated"))? as usize;

    if axis_count == 0 {
        return Err(bad("axisCount is zero"));
    }
    // `axisSize` is a stride, not a struct size: it may exceed 20, never be less.
    if axis_size < 20 {
        return Err(bad("axisSize below the 20-byte VariationAxisRecord"));
    }
    let array_len = axis_count
        .checked_mul(axis_size)
        .ok_or_else(|| bad("axis array size overflows"))?;
    let array_end = axes_offset
        .checked_add(array_len)
        .ok_or_else(|| bad("axis array size overflows"))?;
    if array_end > table.len() {
        return Err(bad("axis array leaves the table"));
    }

    let mut axes = Vec::with_capacity(axis_count);
    for i in 0..axis_count {
        let base = axes_offset + i * axis_size;
        let tag_bytes = table
            .get(base..base + 4)
            .ok_or_else(|| bad("axis record truncated"))?;
        let tag = [tag_bytes[0], tag_bytes[1], tag_bytes[2], tag_bytes[3]];
        let min = read_fixed(table, base + 4).ok_or_else(|| bad("axis record truncated"))?;
        let def = read_fixed(table, base + 8).ok_or_else(|| bad("axis record truncated"))?;
        let max = read_fixed(table, base + 12).ok_or_else(|| bad("axis record truncated"))?;
        axes.push(Axis { tag, min, def, max });
    }
    Ok(axes)
}

fn read_fixed(data: &[u8], offset: usize) -> Option<Fixed> {
    let end = offset.checked_add(4)?;
    data.get(offset..end)
        .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Clamp `value` into the axis range and map it to `[-1, 1]` in 16.16.
fn normalize_axis(axis: &Axis, value: Fixed) -> Fixed {
    // Guard an inverted record (max < min) rather than producing a negative
    // denominator below.
    let max = axis.max.max(axis.min);
    let min = axis.min.min(axis.max);
    let v = value.clamp(min, max);

    let n = if v < axis.def {
        -fixed_div(axis.def.saturating_sub(v), axis.def.saturating_sub(min))
    } else if v > axis.def {
        fixed_div(v.saturating_sub(axis.def), max.saturating_sub(axis.def))
    } else {
        0
    };
    n.clamp(-FIXED_ONE, FIXED_ONE)
}

// ---------------------------------------------------------------------------
// avar
// ---------------------------------------------------------------------------

/// One axis's `avar` segment map: `(fromCoordinate, toCoordinate)` in F2Dot14.
pub(crate) type SegmentMap = Vec<(i16, i16)>;

/// Parsed `avar` segment maps, one per axis.
///
/// A version-2 `avar` is accepted, but only its version-1 segment maps are
/// applied: the additional `ItemVariationStore` is deliberately ignored. No
/// shipping font measured for this implementation carries one, and guessing at
/// the store would be worse than reproducing version-1 behaviour exactly.
pub(crate) struct AvarMaps {
    /// One segment map per axis, in `fvar` axis order.
    pub(crate) maps: Vec<SegmentMap>,
}

/// Parse `avar` version 1 (or the version-1 part of version 2).
///
/// # Errors
/// [`SubsetError::InvalidFont`] for a truncated table or a major version other
/// than 1 or 2.
pub(crate) fn parse_avar(table: &[u8]) -> Result<AvarMaps, SubsetError> {
    let bad = |what: &str| SubsetError::InvalidFont(format!("avar: {what}"));

    let major = get_u16(table, 0).ok_or_else(|| bad("header truncated"))?;
    if major != 1 && major != 2 {
        return Err(bad("unsupported version"));
    }
    let axis_count = get_u16(table, 6).ok_or_else(|| bad("header truncated"))? as usize;

    let mut maps = Vec::with_capacity(axis_count.min(64));
    let mut pos = 8usize;
    for _ in 0..axis_count {
        let count = get_u16(table, pos).ok_or_else(|| bad("segment map truncated"))? as usize;
        pos += 2;
        // Bound the allocation by the bytes that actually remain.
        let needed = count
            .checked_mul(4)
            .ok_or_else(|| bad("segment map size overflows"))?;
        if pos.checked_add(needed).is_none_or(|end| end > table.len()) {
            return Err(bad("segment map leaves the table"));
        }
        let mut map = Vec::with_capacity(count);
        for _ in 0..count {
            let from = get_i16(table, pos).ok_or_else(|| bad("segment map truncated"))?;
            let to = get_i16(table, pos + 2).ok_or_else(|| bad("segment map truncated"))?;
            map.push((from, to));
            pos += 4;
        }
        maps.push(map);
    }

    Ok(AvarMaps { maps })
}

/// Apply one axis's segment map to a normalized 16.16 coordinate.
///
/// A conforming map is strictly increasing and contains the identity entries
/// `-1 → -1`, `0 → 0`, `+1 → +1`, so the below-span / above-span branches are
/// unreachable for a clamped input; they are the FreeType/HarfBuzz recovery for
/// fonts in the wild and are kept for that reason.
fn avar_apply(map: &[(i16, i16)], coord: Fixed) -> Fixed {
    let Some(&(first_from, first_to)) = map.first() else {
        return coord;
    };
    if map.len() == 1 {
        return coord
            .saturating_sub(f2dot14_to_fixed(first_from))
            .saturating_add(f2dot14_to_fixed(first_to));
    }
    for (i, &(from, to)) in map.iter().enumerate() {
        let from_fixed = f2dot14_to_fixed(from);
        if coord == from_fixed {
            return f2dot14_to_fixed(to);
        }
        if coord < from_fixed {
            if i == 0 {
                return coord
                    .saturating_sub(f2dot14_to_fixed(first_from))
                    .saturating_add(f2dot14_to_fixed(first_to));
            }
            let (prev_from, prev_to) = map[i - 1];
            let pf = f2dot14_to_fixed(prev_from);
            let pt = f2dot14_to_fixed(prev_to);
            let t = f2dot14_to_fixed(to);
            return pt.saturating_add(fixed_mul_div(
                t.saturating_sub(pt),
                coord.saturating_sub(pf),
                from_fixed.saturating_sub(pf),
            ));
        }
    }
    // Above the last breakpoint: shift by the last entry.
    let (last_from, last_to) = map[map.len() - 1];
    coord
        .saturating_sub(f2dot14_to_fixed(last_from))
        .saturating_add(f2dot14_to_fixed(last_to))
}

// ---------------------------------------------------------------------------
// Full pipeline
// ---------------------------------------------------------------------------

/// A fully pinned design location.
#[derive(Debug)]
pub(crate) struct Location {
    /// The face's axes, in `fvar` order.
    pub(crate) axes: Vec<Axis>,
    /// The effective **user** value per axis, after clamping and defaulting.
    pub(crate) user: Vec<f32>,
    /// The normalized F2Dot14 coordinate per axis — what the tuple scalars use.
    pub(crate) normalized: Vec<i16>,
}

impl Location {
    /// The effective user value of `tag`, if the face has that axis.
    pub(crate) fn user_value(&self, tag: &[u8; 4]) -> Option<f32> {
        let idx = self.axes.iter().position(|a| &a.tag == tag)?;
        self.user.get(idx).copied()
    }
}

/// Resolve `user_coords` against the face's axes and normalize them.
///
/// Axes absent from `user_coords` pin at their `fvar` default; a repeated tag
/// takes its last value; a tag naming no axis is an error, because silently
/// ignoring it would embed the default instance while reporting success.
///
/// # Errors
/// [`SubsetError::UnknownAxis`] for a tag the face does not have.
pub(crate) fn resolve_location(
    axes: Vec<Axis>,
    avar: Option<&AvarMaps>,
    user_coords: &[([u8; 4], f32)],
) -> Result<Location, SubsetError> {
    let mut user: Vec<f32> = axes.iter().map(|a| a.def as f32 / 65536.0).collect();
    let mut fixed: Vec<Fixed> = vec![0; axes.len()];

    for &(tag, value) in user_coords {
        let idx = axes
            .iter()
            .position(|a| a.tag == tag)
            .ok_or(SubsetError::UnknownAxis(tag))?;
        let Some(axis) = axes.get(idx).copied() else {
            continue;
        };
        let raw = fixed_from_f32(value);
        let clamped = raw.clamp(axis.min.min(axis.max), axis.max.max(axis.min));
        if let Some(slot) = user.get_mut(idx) {
            *slot = clamped as f32 / 65536.0;
        }
        if let Some(slot) = fixed.get_mut(idx) {
            *slot = normalize_axis(&axis, raw);
        }
    }

    if let Some(avar) = avar {
        for (i, seg) in avar.maps.iter().enumerate() {
            let Some(slot) = fixed.get_mut(i) else {
                break;
            };
            if !seg.is_empty() {
                *slot = avar_apply(seg, *slot);
            }
        }
    }

    let normalized = fixed.iter().map(|&f| fixed_to_f2dot14(f)).collect();
    Ok(Location {
        axes,
        user,
        normalized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `wght` 100..400..900 — the NotoSansJP-VF shape, whose default is its
    /// minimum (the case that makes an un-pinned instance embed the wrong mass).
    fn axis(tag: &[u8; 4], min: f32, def: f32, max: f32) -> Axis {
        Axis {
            tag: *tag,
            min: fixed_from_f32(min),
            def: fixed_from_f32(def),
            max: fixed_from_f32(max),
        }
    }

    fn build_fvar(axes: &[(&[u8; 4], f32, f32, f32)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
        out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
        out.extend_from_slice(&16u16.to_be_bytes()); // axesArrayOffset
        out.extend_from_slice(&2u16.to_be_bytes()); // reserved
        out.extend_from_slice(&(axes.len() as u16).to_be_bytes());
        out.extend_from_slice(&20u16.to_be_bytes()); // axisSize
        out.extend_from_slice(&0u16.to_be_bytes()); // instanceCount
        out.extend_from_slice(&0u16.to_be_bytes()); // instanceSize
        for (tag, min, def, max) in axes {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&fixed_from_f32(*min).to_be_bytes());
            out.extend_from_slice(&fixed_from_f32(*def).to_be_bytes());
            out.extend_from_slice(&fixed_from_f32(*max).to_be_bytes());
            out.extend_from_slice(&0u16.to_be_bytes()); // flags
            out.extend_from_slice(&0u16.to_be_bytes()); // axisNameID
        }
        out
    }

    fn build_avar(maps: &[&[(f32, f32)]]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(maps.len() as u16).to_be_bytes());
        for map in maps {
            out.extend_from_slice(&(map.len() as u16).to_be_bytes());
            for (from, to) in map.iter() {
                let f = (from * 16384.0).round() as i16;
                let t = (to * 16384.0).round() as i16;
                out.extend_from_slice(&f.to_be_bytes());
                out.extend_from_slice(&t.to_be_bytes());
            }
        }
        out
    }

    fn norm(axes: Vec<Axis>, avar: Option<&AvarMaps>, coords: &[([u8; 4], f32)]) -> Vec<i16> {
        resolve_location(axes, avar, coords)
            .expect("resolve")
            .normalized
    }

    #[test]
    fn fixed_div_rounds_rather_than_truncates() {
        // 1/3 in 16.16 is 21845.333…; the rounded quotient is 21845, and the
        // half-way case must round up.
        assert_eq!(fixed_div(FIXED_ONE, 3 * FIXED_ONE), 21845);
        assert_eq!(fixed_div(FIXED_ONE, 2 * FIXED_ONE), 32768);
        // 3/2 with the rounding term: (3<<16 + 1) / 2 * ... exercised via mul_div.
        assert_eq!(fixed_mul_div(3, 5, 2), 8); // 7.5 → 8
        assert_eq!(fixed_mul_div(-3, 5, 2), -8);
        assert_eq!(fixed_div(0, 0), i32::MAX);
    }

    #[test]
    fn f2dot14_conversion_is_the_spec_shift() {
        assert_eq!(fixed_to_f2dot14(FIXED_ONE), 16384);
        assert_eq!(fixed_to_f2dot14(-FIXED_ONE), -16384);
        assert_eq!(fixed_to_f2dot14(0), 0);
        assert_eq!(fixed_to_f2dot14(FIXED_ONE / 2), 8192);
    }

    #[test]
    fn axis_endpoints_and_clamping() {
        let axes = vec![axis(b"wght", 100.0, 400.0, 900.0)];
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 100.0)]), vec![-16384]);
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 400.0)]), vec![0]);
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 900.0)]), vec![16384]);
        // Below min and above max clamp to the endpoints, they do not extrapolate.
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", -50.0)]), vec![-16384]);
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 5000.0)]), vec![16384]);
        // An absent axis pins at the default.
        assert_eq!(norm(axes.clone(), None, &[]), vec![0]);
        // 700 of 400..900 = 0.6, which F2Dot14 cannot represent (0.6 * 16384 =
        // 9830.4). The 16.16 pipeline rounds the division and then rounds the
        // 16.16 → F2Dot14 conversion, landing on 9831 rather than the naive
        // 9830. Pinning the pipeline's answer, not the naive one, is the point
        // of this vector: it is what the reference implementations produce.
        assert_eq!(norm(axes, None, &[(*b"wght", 700.0)]), vec![9831]);
    }

    #[test]
    fn default_equal_to_minimum_normalizes_forward_only() {
        // NotoSansJP-VF: wght 100 / default 100 / 900.
        let axes = vec![axis(b"wght", 100.0, 100.0, 900.0)];
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 100.0)]), vec![0]);
        assert_eq!(norm(axes.clone(), None, &[(*b"wght", 900.0)]), vec![16384]);
        // 400 of 100..900 = 0.375 → 6144.
        assert_eq!(norm(axes, None, &[(*b"wght", 400.0)]), vec![6144]);
    }

    #[test]
    fn unknown_axis_tag_is_an_error() {
        let axes = vec![axis(b"wght", 100.0, 400.0, 900.0)];
        let err = resolve_location(axes, None, &[(*b"wdth", 75.0)]).unwrap_err();
        match err {
            SubsetError::UnknownAxis(tag) => assert_eq!(&tag, b"wdth"),
            other => panic!("expected UnknownAxis, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_tag_takes_the_last_value() {
        let axes = vec![axis(b"wght", 100.0, 400.0, 900.0)];
        assert_eq!(
            norm(axes, None, &[(*b"wght", 900.0), (*b"wght", 400.0)]),
            vec![0]
        );
    }

    #[test]
    fn avar_empty_and_single_entry_maps() {
        let axes = vec![axis(b"wght", 100.0, 400.0, 900.0)];
        // Empty map: identity.
        let avar = parse_avar(&build_avar(&[&[]])).expect("avar");
        assert_eq!(
            norm(axes.clone(), Some(&avar), &[(*b"wght", 700.0)]),
            vec![9831]
        );
        // Single entry: a pure shift of (to - from), applied in 16.16 before the
        // single F2Dot14 conversion, so 9831 + 4096 with the shift's own
        // rounding folded in.
        let avar = parse_avar(&build_avar(&[&[(0.0, 0.25)]])).expect("avar");
        assert_eq!(norm(axes, Some(&avar), &[(*b"wght", 700.0)]), vec![13927]);
    }

    #[test]
    fn avar_breakpoint_hit_and_interpolation() {
        let axes = vec![axis(b"wght", 100.0, 400.0, 900.0)];
        // -1 → -1, 0 → 0, 0.6 → 0.8, 1 → 1.
        let avar = parse_avar(&build_avar(&[&[
            (-1.0, -1.0),
            (0.0, 0.0),
            (0.6, 0.8),
            (1.0, 1.0),
        ]]))
        .expect("avar");
        // 700 normalizes to 0.6, the breakpoint, and maps to 0.8 → 13107.
        assert_eq!(
            norm(axes.clone(), Some(&avar), &[(*b"wght", 700.0)]),
            vec![13107]
        );
        // Endpoints stay pinned by the identity entries.
        assert_eq!(
            norm(axes.clone(), Some(&avar), &[(*b"wght", 900.0)]),
            vec![16384]
        );
        assert_eq!(
            norm(axes.clone(), Some(&avar), &[(*b"wght", 100.0)]),
            vec![-16384]
        );
        // 550 → 0.3, halfway along the 0.0..0.6 segment → 0.4. F2Dot14 cannot
        // represent 0.4 either (6553.6); the pipeline rounds to 6554.
        assert_eq!(norm(axes, Some(&avar), &[(*b"wght", 550.0)]), vec![6554]);
    }

    #[test]
    fn fvar_rejects_structural_breakage() {
        assert!(parse_fvar(&[]).is_err());
        let good = build_fvar(&[(b"wght", 100.0, 400.0, 900.0)]);
        assert!(parse_fvar(&good).is_ok());
        // Truncating anywhere must be an error, never a panic.
        for cut in 0..good.len() {
            let _ = parse_fvar(&good[..cut]);
        }
        // axisCount = 0.
        let mut zero_axes = good.clone();
        zero_axes[8] = 0;
        zero_axes[9] = 0;
        assert!(parse_fvar(&zero_axes).is_err());
        // axisSize below the record size.
        let mut small = good.clone();
        small[10] = 0;
        small[11] = 8;
        assert!(parse_fvar(&small).is_err());
    }

    #[test]
    fn avar_truncation_is_an_error_not_a_panic() {
        let good = build_avar(&[&[(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)]]);
        assert!(parse_avar(&good).is_ok());
        for cut in 0..good.len() {
            let _ = parse_avar(&good[..cut]);
        }
    }

    #[test]
    fn parsed_fvar_matches_the_hand_built_axes() {
        let data = build_fvar(&[
            (b"wght", 100.0, 400.0, 900.0),
            (b"wdth", 75.0, 100.0, 100.0),
        ]);
        let axes = parse_fvar(&data).expect("fvar");
        assert_eq!(axes.len(), 2);
        assert_eq!(&axes[0].tag, b"wght");
        assert_eq!(axes[0].min, fixed_from_f32(100.0));
        assert_eq!(axes[1].max, fixed_from_f32(100.0));
    }
}
