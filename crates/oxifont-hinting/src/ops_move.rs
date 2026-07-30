//! Point-movement, interpolation, shift, intersection, IUP, and delta handlers.
//!
//! These opcodes are the heart of grid fitting: they move outline points along
//! the freedom vector so their projected positions land on rounded/CVT-derived
//! distances, then interpolate the untouched points to follow.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::{mul_div, mul_f2dot14, F26Dot6};
use crate::state::ZonePointer;

impl HintingEngine {
    /// Enforce the minimum distance on `distance`, preserving the sign implied by
    /// `reference` (used by the `MDRP`/`MIRP` min-distance bit).
    fn enforce_min_distance(&self, distance: F26Dot6, reference: F26Dot6) -> F26Dot6 {
        let min = self.gs.minimum_distance;
        if distance.abs() < min {
            if reference >= 0 {
                min
            } else {
                -min
            }
        } else {
            distance
        }
    }

    /// Apply the single-width setting: snap `distance` to `single_width_value`
    /// when it is within the single-width cut-in.
    fn apply_single_width(&self, distance: F26Dot6) -> F26Dot6 {
        let sw = self.gs.single_width_value;
        if sw != 0 && (distance - sw).abs() < self.gs.single_width_cut_in {
            if distance >= 0 {
                sw
            } else {
                -sw
            }
        } else {
            distance
        }
    }

    /// The original distance between `rp0` (in `zp0`) and point `p` (in `zp1`),
    /// measured along the dual projection vector.
    fn original_distance(
        &self,
        zp1: ZonePointer,
        p: usize,
        zp0: ZonePointer,
        rp0: usize,
    ) -> Result<F26Dot6, HintingError> {
        Ok(self.dual_project_org(zp1, p)? - self.dual_project_org(zp0, rp0)?)
    }

    /// Move point `p` (in `zp`) by `delta` pixels directly along the freedom
    /// vector, touching the affected axes (shared by `SHPIX` and `DELTAP`).
    fn shift_along_freedom(
        &mut self,
        zp: ZonePointer,
        p: usize,
        delta: F26Dot6,
    ) -> Result<(), HintingError> {
        let freedom = self.gs.freedom;
        let point = self.point_mut(zp, p)?;
        if freedom.x != 0 {
            point.cur_x += mul_f2dot14(delta as i64, freedom.x) as F26Dot6;
            point.touched_x = true;
        }
        if freedom.y != 0 {
            point.cur_y += mul_f2dot14(delta as i64, freedom.y) as F26Dot6;
            point.touched_y = true;
        }
        Ok(())
    }

    // ── MDAP / MIAP ─────────────────────────────────────────────────────────

    /// `MDAP[a]`: move direct absolute point (optionally rounding to grid).
    pub(crate) fn op_mdap(&mut self, round: bool) -> Result<(), HintingError> {
        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let cur = self.project_cur(zp0, p)?;
        let distance = if round {
            self.gs.round_state.round(cur, 0) - cur
        } else {
            0
        };
        self.move_point(zp0, p, distance)?;
        self.gs.rp0 = p;
        self.gs.rp1 = p;
        Ok(())
    }

    /// `MIAP[a]`: move indirect absolute point using a CVT entry.
    pub(crate) fn op_miap(&mut self, round: bool) -> Result<(), HintingError> {
        let n = self.pop_uint()?;
        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let mut distance = self.cvt_get(n)?;

        // On the twilight zone the point is first pinned to the CVT position.
        if zp0 == ZonePointer::Twilight {
            let proj = self.gs.projection;
            let point = self.point_mut(zp0, p)?;
            point.cur_x = mul_f2dot14(distance as i64, proj.x) as F26Dot6;
            point.cur_y = mul_f2dot14(distance as i64, proj.y) as F26Dot6;
            point.org_x = point.cur_x;
            point.org_y = point.cur_y;
        }

        let cur = self.project_cur(zp0, p)?;
        if round {
            if (distance - cur).abs() > self.gs.control_value_cut_in {
                distance = cur;
            }
            distance = self.gs.round_state.round(distance, 0);
        }
        self.move_point(zp0, p, distance - cur)?;
        self.gs.rp0 = p;
        self.gs.rp1 = p;
        Ok(())
    }

    // ── MDRP / MIRP / MSIRP ─────────────────────────────────────────────────

    /// `MDRP[abcde]`: move direct relative point.
    pub(crate) fn op_mdrp(&mut self, flags: u8) -> Result<(), HintingError> {
        let set_rp0 = flags & 0x10 != 0;
        let use_min = flags & 0x08 != 0;
        let round = flags & 0x04 != 0;

        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let rp0 = self.gs.rp0;

        let org_dist = self.original_distance(zp1, p, zp0, rp0)?;
        let mut distance = self.apply_single_width(org_dist);
        distance = if round {
            self.gs.round_state.round(distance, 0)
        } else {
            distance
        };
        if use_min {
            distance = self.enforce_min_distance(distance, org_dist);
        }

        let cur_rp0 = self.project_cur(zp0, rp0)?;
        let cur_p = self.project_cur(zp1, p)?;
        self.move_point(zp1, p, cur_rp0 + distance - cur_p)?;

        self.gs.rp1 = rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    /// `MIRP[abcde]`: move indirect relative point using a CVT entry.
    pub(crate) fn op_mirp(&mut self, flags: u8) -> Result<(), HintingError> {
        let set_rp0 = flags & 0x10 != 0;
        let use_min = flags & 0x08 != 0;
        let round = flags & 0x04 != 0;

        let n = self.pop_uint()?;
        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let rp0 = self.gs.rp0;

        let mut cvt_dist = self.apply_single_width(self.cvt_get(n)?);
        let org_dist = self.original_distance(zp1, p, zp0, rp0)?;

        if self.gs.auto_flip && org_dist != 0 && cvt_dist.signum() != org_dist.signum() {
            cvt_dist = -cvt_dist;
        }

        let mut distance = cvt_dist;
        if round {
            if (cvt_dist - org_dist).abs() > self.gs.control_value_cut_in {
                distance = org_dist;
            }
            distance = self.gs.round_state.round(distance, 0);
        }
        if use_min {
            distance = self.enforce_min_distance(distance, cvt_dist);
        }

        let cur_rp0 = self.project_cur(zp0, rp0)?;
        let cur_p = self.project_cur(zp1, p)?;
        self.move_point(zp1, p, cur_rp0 + distance - cur_p)?;

        self.gs.rp1 = rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    /// `MSIRP[a]`: move stack-indirect relative point.
    pub(crate) fn op_msirp(&mut self, set_rp0: bool) -> Result<(), HintingError> {
        let distance = self.pop()?;
        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let rp0 = self.gs.rp0;

        let cur_rp0 = self.project_cur(zp0, rp0)?;
        let cur_p = self.project_cur(zp1, p)?;
        self.move_point(zp1, p, cur_rp0 + distance - cur_p)?;

        self.gs.rp1 = rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    // ── IP / ALIGNRP / ALIGNPTS ─────────────────────────────────────────────

    /// `IP`: interpolate `loop` points between reference points `rp1` and `rp2`.
    pub(crate) fn op_ip(&mut self) -> Result<(), HintingError> {
        let count = self.take_loop_counter();
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let zp2 = self.gs.zp2;
        let rp1 = self.gs.rp1;
        let rp2 = self.gs.rp2;

        let org_rp1 = self.dual_project_org(zp0, rp1)?;
        let org_rp2 = self.dual_project_org(zp1, rp2)?;
        let cur_rp1 = self.project_cur(zp0, rp1)?;
        let cur_rp2 = self.project_cur(zp1, rp2)?;
        let denom = org_rp2 - org_rp1;

        for _ in 0..count {
            let p = self.pop_uint()?;
            let org_p = self.dual_project_org(zp2, p)?;
            let cur_p = self.project_cur(zp2, p)?;
            let new_p = if denom != 0 {
                cur_rp1
                    + mul_div(
                        (org_p - org_rp1) as i64,
                        (cur_rp2 - cur_rp1) as i64,
                        denom as i64,
                    ) as F26Dot6
            } else {
                cur_rp1 + (org_p - org_rp1)
            };
            self.move_point(zp2, p, new_p - cur_p)?;
        }
        Ok(())
    }

    /// `ALIGNRP`: align `loop` points onto reference point `rp0`.
    pub(crate) fn op_alignrp(&mut self) -> Result<(), HintingError> {
        let count = self.take_loop_counter();
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let rp0 = self.gs.rp0;
        let cur_rp0 = self.project_cur(zp0, rp0)?;
        for _ in 0..count {
            let p = self.pop_uint()?;
            let cur_p = self.project_cur(zp1, p)?;
            self.move_point(zp1, p, cur_rp0 - cur_p)?;
        }
        Ok(())
    }

    /// `ALIGNPTS`: move two points to their shared midpoint projection.
    pub(crate) fn op_alignpts(&mut self) -> Result<(), HintingError> {
        let p1 = self.pop_uint()?;
        let p2 = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let c1 = self.project_cur(zp1, p1)?;
        let c2 = self.project_cur(zp0, p2)?;
        let target = (c1 + c2) / 2;
        self.move_point(zp1, p1, target - c1)?;
        self.move_point(zp0, p2, target - c2)?;
        Ok(())
    }

    /// `UTP`: mark a point untouched along the freedom vector's axes.
    pub(crate) fn op_utp(&mut self) -> Result<(), HintingError> {
        let p = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let freedom = self.gs.freedom;
        let point = self.point_mut(zp0, p)?;
        if freedom.x != 0 {
            point.touched_x = false;
        }
        if freedom.y != 0 {
            point.touched_y = false;
        }
        Ok(())
    }

    // ── SHP / SHC / SHZ / SHPIX ─────────────────────────────────────────────

    /// The reference `(zone, point)` and the amount it moved along the
    /// projection vector, for the `SHP`/`SHC`/`SHZ` family.
    fn shift_reference(&self, use_rp1: bool) -> Result<F26Dot6, HintingError> {
        let (zp, rp) = if use_rp1 {
            (self.gs.zp0, self.gs.rp1)
        } else {
            (self.gs.zp1, self.gs.rp2)
        };
        let point = self.point(zp, rp)?;
        Ok(self
            .gs
            .projection
            .project(point.cur_x - point.org_x, point.cur_y - point.org_y))
    }

    /// `SHP[a]`: shift `loop` points by the reference point's movement.
    pub(crate) fn op_shp(&mut self, use_rp1: bool) -> Result<(), HintingError> {
        let dist = self.shift_reference(use_rp1)?;
        let count = self.take_loop_counter();
        let zp2 = self.gs.zp2;
        for _ in 0..count {
            let p = self.pop_uint()?;
            self.move_point(zp2, p, dist)?;
        }
        Ok(())
    }

    /// `SHC[a]`: shift every point of a contour by the reference movement.
    pub(crate) fn op_shc(&mut self, use_rp1: bool) -> Result<(), HintingError> {
        let dist = self.shift_reference(use_rp1)?;
        let contour = self.pop_uint()?;
        let zp2 = self.gs.zp2;
        let range = self
            .zone(zp2)
            .contour_range(*self.zone(zp2).contour_ends.get(contour).ok_or(
                HintingError::PointOutOfBounds {
                    zone: zp2.number(),
                    index: contour,
                    len: self.zone(zp2).contour_ends.len(),
                },
            )? as usize);
        if let Some((start, end)) = range {
            for p in start..=end {
                self.move_point(zp2, p, dist)?;
            }
        }
        Ok(())
    }

    /// `SHZ[a]`: shift every point in a zone by the reference movement.
    pub(crate) fn op_shz(&mut self, use_rp1: bool) -> Result<(), HintingError> {
        let dist = self.shift_reference(use_rp1)?;
        let zone_num = self.pop()?;
        let target = ZonePointer::from_number(zone_num);
        let len = self.zone(target).len();
        for p in 0..len {
            self.move_point(target, p, dist)?;
        }
        Ok(())
    }

    /// `SHPIX`: shift `loop` points by a pixel amount along the freedom vector.
    pub(crate) fn op_shpix(&mut self) -> Result<(), HintingError> {
        let amount = self.pop()?;
        let count = self.take_loop_counter();
        let zp2 = self.gs.zp2;
        for _ in 0..count {
            let p = self.pop_uint()?;
            self.shift_along_freedom(zp2, p, amount)?;
        }
        Ok(())
    }

    // ── ISECT ───────────────────────────────────────────────────────────────

    /// `ISECT`: move a point to the intersection of two lines.
    pub(crate) fn op_isect(&mut self) -> Result<(), HintingError> {
        let b1 = self.pop_uint()?;
        let b0 = self.pop_uint()?;
        let a1 = self.pop_uint()?;
        let a0 = self.pop_uint()?;
        let p = self.pop_uint()?;

        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let zp2 = self.gs.zp2;

        let pa0 = self.point(zp1, a0)?;
        let pa1 = self.point(zp1, a1)?;
        let pb0 = self.point(zp0, b0)?;
        let pb1 = self.point(zp0, b1)?;

        let (ax, ay) = intersect(
            (pa0.cur_x, pa0.cur_y),
            (pa1.cur_x, pa1.cur_y),
            (pb0.cur_x, pb0.cur_y),
            (pb1.cur_x, pb1.cur_y),
        );

        let point = self.point_mut(zp2, p)?;
        point.cur_x = ax;
        point.cur_y = ay;
        point.touched_x = true;
        point.touched_y = true;
        Ok(())
    }

    // ── DELTA ───────────────────────────────────────────────────────────────

    /// Shared body for `DELTAP1/2/3` (`is_cvt == false`) and `DELTAC1/2/3`.
    pub(crate) fn op_delta(&mut self, band: u32, is_cvt: bool) -> Result<(), HintingError> {
        let count = self.pop_uint()?;
        let ppem = self.ppem() as u32;
        let base = self.gs.delta_base + band;
        let shift = self.gs.delta_shift.min(6);
        let step = 64i32 >> shift;
        let zp0 = self.gs.zp0;

        for _ in 0..count {
            self.budget = self
                .budget
                .checked_sub(1)
                .ok_or(HintingError::ExecutionBudgetExceeded)?;
            let arg = self.pop()?;
            let target = self.pop_uint()?;
            let selector = ((arg >> 4) & 0xF) as u32;
            if base + selector != ppem {
                continue;
            }
            let mut mag = arg & 0xF;
            mag = if mag < 8 { mag - 8 } else { mag - 7 };
            let delta = mag * step;
            if is_cvt {
                let len = self.cvt.len();
                let slot = self
                    .cvt
                    .get_mut(target)
                    .ok_or(HintingError::CvtOutOfBounds { index: target, len })?;
                *slot += delta;
            } else {
                self.shift_along_freedom(zp0, target, delta)?;
            }
        }
        Ok(())
    }
}

/// Compute the intersection of line A (`a0`→`a1`) and line B (`b0`→`b1`) in
/// 26.6 coordinates. Parallel lines fall back to the midpoint of the segment
/// endpoints so the result is always finite.
fn intersect(
    a0: (F26Dot6, F26Dot6),
    a1: (F26Dot6, F26Dot6),
    b0: (F26Dot6, F26Dot6),
    b1: (F26Dot6, F26Dot6),
) -> (F26Dot6, F26Dot6) {
    let dax = (a1.0 - a0.0) as i64;
    let day = (a1.1 - a0.1) as i64;
    let dbx = (b1.0 - b0.0) as i64;
    let dby = (b1.1 - b0.1) as i64;
    let denom = dax * dby - day * dbx;
    if denom == 0 {
        // Parallel / degenerate: use the average of the four endpoints.
        let x = (a0.0 as i64 + a1.0 as i64 + b0.0 as i64 + b1.0 as i64) / 4;
        let y = (a0.1 as i64 + a1.1 as i64 + b0.1 as i64 + b1.1 as i64) / 4;
        return (x as F26Dot6, y as F26Dot6);
    }
    // t along line A where it meets line B.
    let num = (b0.0 as i64 - a0.0 as i64) * dby - (b0.1 as i64 - a0.1 as i64) * dbx;
    let x = a0.0 as i64 + num * dax / denom;
    let y = a0.1 as i64 + num * day / denom;
    (x as F26Dot6, y as F26Dot6)
}
