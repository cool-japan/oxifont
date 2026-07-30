//! The TrueType graphics state and point zones.

use crate::math::{F26Dot6, RoundState, Vector, ONE_PIXEL};

/// The full TrueType graphics state.
///
/// A fresh [`GraphicsState::default`] holds the interpreter defaults defined by
/// the specification. The `prep` (control-value) program mutates a default state
/// at each ppem; the resulting state is cloned as the starting point for every
/// glyph program.
#[derive(Debug, Clone)]
pub struct GraphicsState {
    /// Enables automatic sign flipping of the CVT cut-in in `MIRP`/`MDRP`.
    pub auto_flip: bool,
    /// The control-value cut-in distance (26.6). Below it, CVT snapping is used.
    pub control_value_cut_in: F26Dot6,
    /// The delta base (`SDB`) — the ppem for the first `DELTA` exception.
    pub delta_base: u32,
    /// The delta shift (`SDS`) — the granularity of `DELTA` exceptions.
    pub delta_shift: u32,
    /// The dual projection vector (2.14 unit vector).
    pub dual_projection: Vector,
    /// The freedom vector (2.14 unit vector) — the direction points move in.
    pub freedom: Vector,
    /// The projection vector (2.14 unit vector) — the direction distances measure.
    pub projection: Vector,
    /// The instruction-control flags (`INSTCTRL`).
    pub instruct_control: u8,
    /// The loop counter (`SLOOP`) for `SHP`/`IP`/... family opcodes.
    pub loop_counter: i32,
    /// The minimum distance (26.6) enforced by `MDRP`/`MIRP`.
    pub minimum_distance: F26Dot6,
    /// The reference point 0 index.
    pub rp0: usize,
    /// The reference point 1 index.
    pub rp1: usize,
    /// The reference point 2 index.
    pub rp2: usize,
    /// The active rounding state.
    pub round_state: RoundState,
    /// The scan-conversion control flags (`SCANCTRL`).
    pub scan_control: u16,
    /// The scan-conversion rule type (`SCANTYPE`).
    pub scan_type: i32,
    /// The single-width cut-in (26.6).
    pub single_width_cut_in: F26Dot6,
    /// The single-width value (26.6).
    pub single_width_value: F26Dot6,
    /// Zone pointer 0 (selects the zone `rp0`-relative ops act on).
    pub zp0: ZonePointer,
    /// Zone pointer 1.
    pub zp1: ZonePointer,
    /// Zone pointer 2.
    pub zp2: ZonePointer,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            auto_flip: true,
            control_value_cut_in: 68, // 17/16 px in 26.6 (68 == 1.0625 * 64).
            delta_base: 9,
            delta_shift: 3,
            dual_projection: Vector::X_AXIS,
            freedom: Vector::X_AXIS,
            projection: Vector::X_AXIS,
            instruct_control: 0,
            loop_counter: 1,
            minimum_distance: ONE_PIXEL,
            rp0: 0,
            rp1: 0,
            rp2: 0,
            round_state: RoundState::grid(),
            scan_control: 0,
            scan_type: 0,
            single_width_cut_in: 0,
            single_width_value: 0,
            zp0: ZonePointer::Glyph,
            zp1: ZonePointer::Glyph,
            zp2: ZonePointer::Glyph,
        }
    }
}

/// Which zone a zone-pointer selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZonePointer {
    /// Zone 0: the twilight zone (scratch points created by the program).
    Twilight,
    /// Zone 1: the glyph zone (the outline being fitted).
    Glyph,
}

impl ZonePointer {
    /// Decode a zone number from the stack (0 = twilight, otherwise glyph).
    #[inline]
    pub fn from_number(n: i32) -> Self {
        if n == 0 {
            ZonePointer::Twilight
        } else {
            ZonePointer::Glyph
        }
    }

    /// The zone number (0 or 1) for error reporting.
    #[inline]
    pub fn number(self) -> u8 {
        match self {
            ZonePointer::Twilight => 0,
            ZonePointer::Glyph => 1,
        }
    }
}

/// A single point in a zone, tracking original, scaled, and fitted coordinates.
#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    /// The current (being-fitted) x coordinate in 26.6.
    pub cur_x: F26Dot6,
    /// The current (being-fitted) y coordinate in 26.6.
    pub cur_y: F26Dot6,
    /// The original scaled x coordinate in 26.6 (before fitting).
    pub org_x: F26Dot6,
    /// The original scaled y coordinate in 26.6 (before fitting).
    pub org_y: F26Dot6,
    /// True when the point lies on the curve (vs. a Bezier control point).
    pub on_curve: bool,
    /// True once the x coordinate has been touched by an instruction (for IUP).
    pub touched_x: bool,
    /// True once the y coordinate has been touched by an instruction (for IUP).
    pub touched_y: bool,
}

/// A zone of points plus the contour end-point indices (glyph zone only).
#[derive(Debug, Clone, Default)]
pub struct Zone {
    /// The points, including any trailing phantom points.
    pub points: Vec<Point>,
    /// Indices (into `points`) of the last point of each contour.
    pub contour_ends: Vec<u16>,
}

impl Zone {
    /// Create an all-zero twilight zone with `n` points.
    pub fn twilight(n: usize) -> Self {
        Zone {
            points: vec![Point::default(); n],
            contour_ends: Vec::new(),
        }
    }

    /// The number of points in this zone.
    #[inline]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether this zone has no points.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Return `(start, end)` point indices for the contour containing `point`.
    ///
    /// Contours are stored as inclusive end indices; the start of contour `c`
    /// is one past the end of contour `c - 1` (or `0` for the first).
    pub fn contour_range(&self, point: usize) -> Option<(usize, usize)> {
        let mut start = 0usize;
        for &end in &self.contour_ends {
            let end = end as usize;
            if point >= start && point <= end {
                return Some((start, end));
            }
            start = end + 1;
        }
        None
    }
}
