//! Fixed-point arithmetic, unit vectors, and the rounding state machine.
//!
//! TrueType hinting works in two fixed-point formats:
//!
//! * **F26Dot6** — a signed 32-bit value with 6 fractional bits, so `64` == one
//!   pixel. Point coordinates, the CVT, distances, and the stack's "distance"
//!   operands all live here.
//! * **F2Dot14** — a signed value with 14 fractional bits, so `0x4000` == `1.0`.
//!   The projection, freedom, and dual-projection unit vectors use this format.
//!
//! All helpers mirror the reference (FreeType) integer semantics so that results
//! are bit-reproducible and never rely on floating point.

/// A coordinate or distance in 26.6 fixed point (`64` == one pixel).
pub type F26Dot6 = i32;

/// A unit-vector component in 2.14 fixed point (`0x4000` == `1.0`).
pub type F2Dot14 = i32;

/// `1.0` expressed in 2.14 fixed point.
pub const ONE_2DOT14: F2Dot14 = 0x4000;

/// One pixel expressed in 26.6 fixed point.
pub const ONE_PIXEL: F26Dot6 = 64;

/// Signed rounded multiply-divide: `(a * b) / c` with round-half-away-from-zero.
///
/// Uses 64-bit intermediate math so the multiply cannot overflow for any 32-bit
/// inputs. Returns `0` when `c == 0` (callers that must reject a zero divisor
/// check for it themselves — see `DIV`).
#[inline]
pub fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    if c == 0 {
        return 0;
    }
    let s = a.signum() * b.signum() * c.signum();
    let num = (a.unsigned_abs()) * (b.unsigned_abs());
    let den = c.unsigned_abs();
    let q = (num + den / 2) / den;
    s * (q as i64)
}

/// Multiply a value by a 2.14 fixed-point factor: `(a * factor) / 0x4000`.
#[inline]
pub fn mul_f2dot14(a: i64, factor: F2Dot14) -> i64 {
    mul_div(a, factor as i64, ONE_2DOT14 as i64)
}

/// The `MUL` opcode: F26Dot6 * F26Dot6 → F26Dot6 (`(a * b) / 64`).
#[inline]
pub fn f26dot6_mul(a: F26Dot6, b: F26Dot6) -> F26Dot6 {
    mul_div(a as i64, b as i64, 64) as F26Dot6
}

/// The `DIV` opcode: F26Dot6 / F26Dot6 → F26Dot6 (`(a * 64) / b`).
///
/// The caller must reject `b == 0` before calling.
#[inline]
pub fn f26dot6_div(a: F26Dot6, b: F26Dot6) -> F26Dot6 {
    mul_div(a as i64, 64, b as i64) as F26Dot6
}

/// A 2.14 unit vector used for the projection / freedom / dual vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vector {
    /// X component in 2.14 fixed point.
    pub x: F2Dot14,
    /// Y component in 2.14 fixed point.
    pub y: F2Dot14,
}

impl Vector {
    /// The x-axis unit vector `(1, 0)`.
    pub const X_AXIS: Vector = Vector {
        x: ONE_2DOT14,
        y: 0,
    };
    /// The y-axis unit vector `(0, 1)`.
    pub const Y_AXIS: Vector = Vector {
        x: 0,
        y: ONE_2DOT14,
    };

    /// Construct a vector from raw 2.14 components.
    #[inline]
    pub const fn new(x: F2Dot14, y: F2Dot14) -> Self {
        Vector { x, y }
    }

    /// Project a 26.6 displacement `(dx, dy)` onto this unit vector, returning a
    /// 26.6 scalar. This is the dot product `dx*x + dy*y` scaled back by 2.14.
    #[inline]
    pub fn project(&self, dx: F26Dot6, dy: F26Dot6) -> F26Dot6 {
        (mul_f2dot14(dx as i64, self.x) + mul_f2dot14(dy as i64, self.y)) as F26Dot6
    }

    /// The 2.14 dot product of two unit vectors, returned in 2.14.
    #[inline]
    pub fn dot(&self, other: &Vector) -> F2Dot14 {
        (mul_f2dot14(self.x as i64, other.x) + mul_f2dot14(self.y as i64, other.y)) as F2Dot14
    }

    /// Normalise `(x, y)` (given in an arbitrary scale) to a 2.14 unit vector.
    ///
    /// Returns [`Vector::X_AXIS`] when the input has (near) zero length so the
    /// graphics state never holds a degenerate vector.
    pub fn normalize(x: i64, y: i64) -> Vector {
        if x == 0 && y == 0 {
            return Vector::X_AXIS;
        }
        // Integer hypot via magnitude scaling to 2.14.
        let len = isqrt((x * x + y * y) as u128) as i64;
        if len == 0 {
            return Vector::X_AXIS;
        }
        let nx = mul_div(x, ONE_2DOT14 as i64, len) as F2Dot14;
        let ny = mul_div(y, ONE_2DOT14 as i64, len) as F2Dot14;
        Vector { x: nx, y: ny }
    }

    /// The perpendicular of this vector, rotated +90 degrees: `(-y, x)`.
    #[inline]
    pub fn perpendicular(&self) -> Vector {
        Vector {
            x: -self.y,
            y: self.x,
        }
    }
}

/// Integer square root (floor) for a 128-bit radicand.
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The rounding state selected by the `RTG`/`RTHG`/`SROUND`/... opcodes.
///
/// Every concrete rounding mode is expressed as a *super-round* triple
/// `(period, phase, threshold)` in 26.6 units, plus an `RoundKind::Off` escape
/// hatch for `ROFF`. The classic modes map to fixed triples; `SROUND` and
/// `S45ROUND` decode their triple from the opcode operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundState {
    kind: RoundKind,
    period: i32,
    phase: i32,
    threshold: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoundKind {
    Super,
    Off,
}

impl Default for RoundState {
    /// The TrueType default is round-to-grid (`RTG`).
    fn default() -> Self {
        RoundState::grid()
    }
}

impl RoundState {
    /// Round to grid (`RTG`): nearest whole pixel.
    pub fn grid() -> Self {
        RoundState {
            kind: RoundKind::Super,
            period: 64,
            phase: 0,
            threshold: 32,
        }
    }

    /// Round to half grid (`RTHG`): nearest pixel + 0.5.
    pub fn half_grid() -> Self {
        RoundState {
            kind: RoundKind::Super,
            period: 64,
            phase: 32,
            threshold: 32,
        }
    }

    /// Round to double grid (`RTDG`): nearest half pixel.
    pub fn double_grid() -> Self {
        RoundState {
            kind: RoundKind::Super,
            period: 32,
            phase: 0,
            threshold: 16,
        }
    }

    /// Round up to grid (`RUTG`): toward the next whole pixel.
    pub fn up_to_grid() -> Self {
        RoundState {
            kind: RoundKind::Super,
            period: 64,
            phase: 0,
            threshold: 63,
        }
    }

    /// Round down to grid (`RDTG`): toward the previous whole pixel.
    pub fn down_to_grid() -> Self {
        RoundState {
            kind: RoundKind::Super,
            period: 64,
            phase: 0,
            threshold: 0,
        }
    }

    /// No rounding (`ROFF`).
    pub fn off() -> Self {
        RoundState {
            kind: RoundKind::Off,
            period: 64,
            phase: 0,
            threshold: 0,
        }
    }

    /// Decode a `SROUND` / `S45ROUND` operand into a super-round triple.
    ///
    /// `base_period` is `64` for `SROUND` (whole pixel) or the 45-degree
    /// diagonal grid period for `S45ROUND`.
    pub fn super_round(base_period: i32, selector: u8) -> Self {
        let period = match selector & 0xC0 {
            0x00 => base_period / 2,
            0x40 => base_period,
            0x80 => base_period * 2,
            _ => base_period, // 0xC0 is reserved; fall back to one period.
        };
        let period = period.max(1);
        let phase = match selector & 0x30 {
            0x00 => 0,
            0x10 => period / 4,
            0x20 => period / 2,
            _ => period * 3 / 4,
        };
        let threshold = if selector & 0x0F == 0 {
            period - 1
        } else {
            ((selector & 0x0F) as i32 - 4) * period / 8
        };
        RoundState {
            kind: RoundKind::Super,
            period,
            phase,
            threshold,
        }
    }

    /// The 45-degree base period used by `S45ROUND`: `round(64 * sqrt(2))`.
    pub const S45_BASE_PERIOD: i32 = 90;

    /// Apply this rounding state to a 26.6 `distance`, with an optional engine
    /// `compensation` (also 26.6, usually zero).
    pub fn round(&self, distance: F26Dot6, compensation: F26Dot6) -> F26Dot6 {
        match self.kind {
            RoundKind::Off => {
                // ROFF still adds engine compensation but performs no snapping.
                distance + compensation
            }
            RoundKind::Super => {
                let period = self.period.max(1);
                if distance >= 0 {
                    let mut val = floor_to_period(
                        distance - self.phase + self.threshold + compensation,
                        period,
                    );
                    val += self.phase;
                    if val < 0 {
                        val = self.phase;
                    }
                    val
                } else {
                    let mut val = -floor_to_period(
                        self.threshold - self.phase - distance + compensation,
                        period,
                    );
                    val -= self.phase;
                    if val > 0 {
                        val = -self.phase;
                    }
                    val
                }
            }
        }
    }
}

/// Floor `v` to the nearest lower multiple of `period` (works for any positive
/// `period`, including non-powers-of-two used by `S45ROUND`).
#[inline]
fn floor_to_period(v: i32, period: i32) -> i32 {
    period * v.div_euclid(period)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_rounds_half_away_from_zero() {
        assert_eq!(mul_div(3, 1, 2), 2); // 1.5 -> 2
        assert_eq!(mul_div(-3, 1, 2), -2); // -1.5 -> -2
        assert_eq!(mul_div(5, 1, 2), 3); // 2.5 -> 3
    }

    #[test]
    fn mul_div_zero_divisor_is_zero() {
        assert_eq!(mul_div(10, 10, 0), 0);
    }

    #[test]
    fn f26dot6_mul_matches_pixels() {
        // 2px * 3px = 6px.
        assert_eq!(f26dot6_mul(128, 192), 384);
    }

    #[test]
    fn f26dot6_div_matches_pixels() {
        // 6px / 3px = 2px.
        assert_eq!(f26dot6_div(384, 192), 128);
    }

    #[test]
    fn projection_along_x_axis() {
        let v = Vector::X_AXIS;
        assert_eq!(v.project(100, 999), 100);
    }

    #[test]
    fn projection_along_y_axis() {
        let v = Vector::Y_AXIS;
        assert_eq!(v.project(999, 100), 100);
    }

    #[test]
    fn normalize_produces_unit_length() {
        let v = Vector::normalize(3, 4);
        // 3-4-5 triangle: x = 0.6, y = 0.8 in 2.14.
        assert!((v.x - (ONE_2DOT14 * 3 / 5)).abs() <= 2);
        assert!((v.y - (ONE_2DOT14 * 4 / 5)).abs() <= 2);
    }

    #[test]
    fn round_to_grid_snaps_to_pixel() {
        let r = RoundState::grid();
        assert_eq!(r.round(70, 0), 64); // 1.09px -> 1px
        assert_eq!(r.round(96, 0), 128); // 1.5px -> 2px
        assert_eq!(r.round(-70, 0), -64);
    }

    #[test]
    fn round_up_to_grid_ceils() {
        let r = RoundState::up_to_grid();
        assert_eq!(r.round(10, 0), 64);
        assert_eq!(r.round(64, 0), 64);
        assert_eq!(r.round(65, 0), 128);
    }

    #[test]
    fn round_down_to_grid_floors() {
        let r = RoundState::down_to_grid();
        assert_eq!(r.round(70, 0), 64);
        assert_eq!(r.round(63, 0), 0);
    }

    #[test]
    fn round_half_grid_snaps_to_half() {
        let r = RoundState::half_grid();
        // Half-grid results land on n.5 pixels (32, 96, ... in 26.6).
        assert_eq!(r.round(50, 0), 32); // 0.78px -> 0.5px
        assert_eq!(r.round(80, 0), 96); // 1.25px -> 1.5px
    }

    #[test]
    fn round_off_is_identity() {
        let r = RoundState::off();
        assert_eq!(r.round(77, 0), 77);
    }

    #[test]
    fn super_round_one_pixel_matches_grid() {
        // selector 0x48: period = one pixel, phase 0, threshold = period/2.
        let r = RoundState::super_round(64, 0x48);
        assert_eq!(r.round(70, 0), 64);
        assert_eq!(r.round(96, 0), 128);
    }
}
