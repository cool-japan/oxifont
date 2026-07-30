//! `IUP` — interpolate untouched points.
//!
//! After the explicit hinting instructions have moved a subset of a contour's
//! points (marking them "touched"), `IUP` propagates that motion to the
//! remaining points so curves stay smooth. It runs independently per axis and
//! per contour using the classic reference-pair interpolation.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::{mul_div, F26Dot6};
use crate::state::ZonePointer;

/// One axis's view of a point for interpolation.
#[derive(Clone, Copy)]
struct AxisPoint {
    org: F26Dot6,
    cur: F26Dot6,
    touched: bool,
}

impl HintingEngine {
    /// `IUP[a]`: interpolate untouched points in the x-axis (`x_axis == true`)
    /// or y-axis. Always operates on the glyph zone (zone 1).
    pub(crate) fn op_iup(&mut self, x_axis: bool) -> Result<(), HintingError> {
        // Snapshot contour bounds first (immutable borrow), then mutate.
        let contour_ends = self.glyph_zone_contour_ends();
        let num_contour_points = self.glyph_zone_contour_point_count();

        let mut start = 0usize;
        for &end in &contour_ends {
            let end = end as usize;
            if end >= num_contour_points {
                break;
            }
            self.iup_contour(start, end, x_axis)?;
            start = end + 1;
        }
        Ok(())
    }

    /// The glyph zone's contour end indices (copied to break the borrow).
    fn glyph_zone_contour_ends(&self) -> Vec<u16> {
        self.zone(ZonePointer::Glyph).contour_ends.clone()
    }

    /// The number of contour points (excludes phantom points).
    fn glyph_zone_contour_point_count(&self) -> usize {
        // The last contour end + 1 bounds the real points; phantom points sit
        // after it and are never interpolated.
        self.zone(ZonePointer::Glyph)
            .contour_ends
            .last()
            .map(|&e| e as usize + 1)
            .unwrap_or(0)
    }

    fn iup_contour(&mut self, start: usize, end: usize, x_axis: bool) -> Result<(), HintingError> {
        if end < start {
            return Ok(());
        }
        let len = end - start + 1;
        // Build the per-axis view.
        let mut axis: Vec<AxisPoint> = Vec::with_capacity(len);
        {
            let zone = self.zone(ZonePointer::Glyph);
            for i in start..=end {
                let p = zone.points.get(i).ok_or(HintingError::PointOutOfBounds {
                    zone: 1,
                    index: i,
                    len: zone.points.len(),
                })?;
                axis.push(if x_axis {
                    AxisPoint {
                        org: p.org_x,
                        cur: p.cur_x,
                        touched: p.touched_x,
                    }
                } else {
                    AxisPoint {
                        org: p.org_y,
                        cur: p.cur_y,
                        touched: p.touched_y,
                    }
                });
            }
        }

        let touched: Vec<usize> = (0..len).filter(|&i| axis[i].touched).collect();
        if touched.is_empty() {
            return Ok(());
        }

        let mut new_cur = axis.iter().map(|a| a.cur).collect::<Vec<_>>();

        if touched.len() == 1 {
            // Single reference: shift the whole contour by its delta.
            let t = touched[0];
            let delta = axis[t].cur - axis[t].org;
            for (i, slot) in new_cur.iter_mut().enumerate() {
                if !axis[i].touched {
                    *slot = axis[i].org + delta;
                }
            }
        } else {
            // Interpolate each untouched run between consecutive touched points,
            // wrapping around the contour.
            for w in 0..touched.len() {
                let t1 = touched[w];
                let t2 = touched[(w + 1) % touched.len()];
                interpolate_run(&axis, &mut new_cur, t1, t2, len);
            }
        }

        // Write the interpolated coordinates back.
        let zone = self.zone_mut(ZonePointer::Glyph);
        for (offset, &value) in new_cur.iter().enumerate() {
            if let Some(p) = zone.points.get_mut(start + offset) {
                if x_axis {
                    p.cur_x = value;
                } else {
                    p.cur_y = value;
                }
            }
        }
        Ok(())
    }
}

/// Interpolate the untouched points strictly between touched points `t1` and
/// `t2` (contour-relative indices), wrapping around a contour of length `len`.
fn interpolate_run(axis: &[AxisPoint], new_cur: &mut [F26Dot6], t1: usize, t2: usize, len: usize) {
    // Order the two reference points by original coordinate.
    let (lo, hi) = if axis[t1].org <= axis[t2].org {
        (t1, t2)
    } else {
        (t2, t1)
    };
    let org_lo = axis[lo].org;
    let org_hi = axis[hi].org;
    let cur_lo = new_cur[lo];
    let cur_hi = new_cur[hi];
    let span = org_hi - org_lo;

    // Walk the untouched points between t1 and t2 in contour order.
    let mut i = (t1 + 1) % len;
    while i != t2 {
        if !axis[i].touched {
            let ou = axis[i].org;
            let value = if ou <= org_lo {
                ou + (cur_lo - org_lo)
            } else if ou >= org_hi {
                ou + (cur_hi - org_hi)
            } else if span != 0 {
                cur_lo
                    + mul_div((ou - org_lo) as i64, (cur_hi - cur_lo) as i64, span as i64)
                        as F26Dot6
            } else {
                ou + (cur_lo - org_lo)
            };
            new_cur[i] = value;
        }
        i = (i + 1) % len;
    }
}
