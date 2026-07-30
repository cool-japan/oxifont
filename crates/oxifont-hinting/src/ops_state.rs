//! Graphics-state, vector, storage, CVT, and measurement opcode handlers.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::Vector;
use crate::state::ZonePointer;

impl HintingEngine {
    // ── Vector setup ────────────────────────────────────────────────────────

    /// Axis unit vector for an opcode's low bit (`1` → x-axis, `0` → y-axis).
    fn axis_vector(a: bool) -> Vector {
        if a {
            Vector::X_AXIS
        } else {
            Vector::Y_AXIS
        }
    }

    /// `SVTCA[a]`: set freedom and projection to a coordinate axis.
    pub(crate) fn op_svtca(&mut self, a: bool) {
        self.set_both_vectors(Self::axis_vector(a));
    }

    /// `SPVTCA[a]`: set the projection (and dual) vector to a coordinate axis.
    pub(crate) fn op_spvtca(&mut self, a: bool) {
        let v = Self::axis_vector(a);
        self.gs.projection = v;
        self.gs.dual_projection = v;
    }

    /// `SFVTCA[a]`: set the freedom vector to a coordinate axis.
    pub(crate) fn op_sfvtca(&mut self, a: bool) {
        self.gs.freedom = Self::axis_vector(a);
    }

    /// Current-coordinate unit vector between two points (`p1 - p2`).
    fn line_unit_vector(
        &self,
        zp1: ZonePointer,
        p1: usize,
        zp2: ZonePointer,
        p2: usize,
        original: bool,
    ) -> Result<Vector, HintingError> {
        let a = self.point(zp1, p1)?;
        let b = self.point(zp2, p2)?;
        let (dx, dy) = if original {
            (a.org_x - b.org_x, a.org_y - b.org_y)
        } else {
            (a.cur_x - b.cur_x, a.cur_y - b.cur_y)
        };
        Ok(Vector::normalize(dx as i64, dy as i64))
    }

    /// `SPVTL[a]`: set projection parallel (`a=0`) / perpendicular (`a=1`) to a line.
    pub(crate) fn op_spvtl(&mut self, perpendicular: bool) -> Result<(), HintingError> {
        let p1 = self.pop_uint()?;
        let p2 = self.pop_uint()?;
        let mut v = self.line_unit_vector(self.gs.zp1, p1, self.gs.zp2, p2, false)?;
        if perpendicular {
            v = v.perpendicular();
        }
        self.gs.projection = v;
        self.gs.dual_projection = v;
        Ok(())
    }

    /// `SFVTL[a]`: set freedom parallel / perpendicular to a line.
    pub(crate) fn op_sfvtl(&mut self, perpendicular: bool) -> Result<(), HintingError> {
        let p1 = self.pop_uint()?;
        let p2 = self.pop_uint()?;
        let mut v = self.line_unit_vector(self.gs.zp1, p1, self.gs.zp2, p2, false)?;
        if perpendicular {
            v = v.perpendicular();
        }
        self.gs.freedom = v;
        Ok(())
    }

    /// `SDPVTL[a]`: set dual projection (original) and projection (current) to a line.
    pub(crate) fn op_sdpvtl(&mut self, perpendicular: bool) -> Result<(), HintingError> {
        let p1 = self.pop_uint()?;
        let p2 = self.pop_uint()?;
        let mut dual = self.line_unit_vector(self.gs.zp1, p1, self.gs.zp2, p2, true)?;
        let mut proj = self.line_unit_vector(self.gs.zp1, p1, self.gs.zp2, p2, false)?;
        if perpendicular {
            dual = dual.perpendicular();
            proj = proj.perpendicular();
        }
        self.gs.dual_projection = dual;
        self.gs.projection = proj;
        Ok(())
    }

    /// `SPVFS`: set projection (and dual) vector from two 2.14 stack values.
    pub(crate) fn op_spvfs(&mut self) -> Result<(), HintingError> {
        let y = self.pop()?;
        let x = self.pop()?;
        let v = Vector::normalize(x as i64, y as i64);
        self.gs.projection = v;
        self.gs.dual_projection = v;
        Ok(())
    }

    /// `SFVFS`: set freedom vector from two 2.14 stack values.
    pub(crate) fn op_sfvfs(&mut self) -> Result<(), HintingError> {
        let y = self.pop()?;
        let x = self.pop()?;
        self.gs.freedom = Vector::normalize(x as i64, y as i64);
        Ok(())
    }

    /// `GPV`: push the projection vector's x and y components (2.14).
    pub(crate) fn op_gpv(&mut self) -> Result<(), HintingError> {
        let v = self.gs.projection;
        self.push(v.x)?;
        self.push(v.y)
    }

    /// `GFV`: push the freedom vector's x and y components (2.14).
    pub(crate) fn op_gfv(&mut self) -> Result<(), HintingError> {
        let v = self.gs.freedom;
        self.push(v.x)?;
        self.push(v.y)
    }

    /// `SFVTPV`: set the freedom vector to the projection vector.
    pub(crate) fn op_sfvtpv(&mut self) {
        self.gs.freedom = self.gs.projection;
    }

    // ── Reference points and zone pointers ──────────────────────────────────

    /// `SRP0`/`SRP1`/`SRP2`: set a reference point.
    pub(crate) fn op_srp(&mut self, which: u8) -> Result<(), HintingError> {
        let p = self.pop_uint()?;
        match which {
            0 => self.gs.rp0 = p,
            1 => self.gs.rp1 = p,
            _ => self.gs.rp2 = p,
        }
        Ok(())
    }

    /// `SZP0`/`SZP1`/`SZP2`: set a zone pointer.
    pub(crate) fn op_szp(&mut self, which: u8) -> Result<(), HintingError> {
        let z = ZonePointer::from_number(self.pop()?);
        match which {
            0 => self.gs.zp0 = z,
            1 => self.gs.zp1 = z,
            _ => self.gs.zp2 = z,
        }
        Ok(())
    }

    /// `SZPS`: set all three zone pointers.
    pub(crate) fn op_szps(&mut self) -> Result<(), HintingError> {
        let z = ZonePointer::from_number(self.pop()?);
        self.gs.zp0 = z;
        self.gs.zp1 = z;
        self.gs.zp2 = z;
        Ok(())
    }

    // ── Scalar graphics-state setters ───────────────────────────────────────

    /// `SLOOP`: set the loop counter.
    pub(crate) fn op_sloop(&mut self) -> Result<(), HintingError> {
        self.gs.loop_counter = self.pop()?;
        Ok(())
    }

    /// `SMD`: set the minimum distance.
    pub(crate) fn op_smd(&mut self) -> Result<(), HintingError> {
        self.gs.minimum_distance = self.pop()?;
        Ok(())
    }

    /// `SCVTCI`: set the control-value cut-in.
    pub(crate) fn op_scvtci(&mut self) -> Result<(), HintingError> {
        self.gs.control_value_cut_in = self.pop()?;
        Ok(())
    }

    /// `SSWCI`: set the single-width cut-in.
    pub(crate) fn op_sswci(&mut self) -> Result<(), HintingError> {
        self.gs.single_width_cut_in = self.pop()?;
        Ok(())
    }

    /// `SSW`: set the single-width value (font units → pixels).
    pub(crate) fn op_ssw(&mut self) -> Result<(), HintingError> {
        let v = self.pop()?;
        self.gs.single_width_value = self.scale_funit(v);
        Ok(())
    }

    /// `SDB`: set the delta base.
    pub(crate) fn op_sdb(&mut self) -> Result<(), HintingError> {
        self.gs.delta_base = self.pop()? as u32;
        Ok(())
    }

    /// `SDS`: set the delta shift (clamped to a sane range).
    pub(crate) fn op_sds(&mut self) -> Result<(), HintingError> {
        let v = self.pop()?;
        self.gs.delta_shift = (v.clamp(0, 6)) as u32;
        Ok(())
    }

    /// `FLIPON` / `FLIPOFF`: toggle the auto-flip flag.
    pub(crate) fn op_flip_auto(&mut self, on: bool) {
        self.gs.auto_flip = on;
    }

    /// `SCANCTRL`: set scan-conversion control.
    pub(crate) fn op_scanctrl(&mut self) -> Result<(), HintingError> {
        self.gs.scan_control = (self.pop()? & 0xFFFF) as u16;
        Ok(())
    }

    /// `SCANTYPE`: set scan-conversion type.
    pub(crate) fn op_scantype(&mut self) -> Result<(), HintingError> {
        self.gs.scan_type = self.pop()?;
        Ok(())
    }

    /// `INSTCTRL`: set instruction-execution control flags.
    pub(crate) fn op_instctrl(&mut self) -> Result<(), HintingError> {
        let selector = self.pop()?;
        let value = self.pop()?;
        // selector 1 → grid-fit disable bit; selector 2 → default-flags bit.
        if selector & 1 != 0 {
            if value & 1 != 0 {
                self.gs.instruct_control |= 0x01;
            } else {
                self.gs.instruct_control &= !0x01;
            }
        }
        if selector & 2 != 0 {
            if value & 2 != 0 {
                self.gs.instruct_control |= 0x02;
            } else {
                self.gs.instruct_control &= !0x02;
            }
        }
        Ok(())
    }

    /// `GETINFO`: report rasterizer capabilities for the requested selector.
    pub(crate) fn op_getinfo(&mut self) -> Result<(), HintingError> {
        let selector = self.pop()?;
        let mut result = 0i32;
        // Bit 0: rasterizer version. Report a modern ClearType-era version (40).
        if selector & 0x01 != 0 {
            result |= 40;
        }
        // Bit 1 (glyph rotated), bit 2 (stretched), grayscale, ClearType bits:
        // this engine performs monochrome, unrotated, unstretched fitting → 0.
        self.push(result)
    }

    // ── Storage and CVT ─────────────────────────────────────────────────────

    /// `RS`: read from the storage area.
    pub(crate) fn op_rs(&mut self) -> Result<(), HintingError> {
        let idx = self.pop_uint()?;
        let v = *self
            .storage
            .get(idx)
            .ok_or(HintingError::StorageOutOfBounds {
                index: idx,
                len: self.storage.len(),
            })?;
        self.push(v)
    }

    /// `WS`: write to the storage area.
    pub(crate) fn op_ws(&mut self) -> Result<(), HintingError> {
        let value = self.pop()?;
        let idx = self.pop_uint()?;
        let len = self.storage.len();
        *self
            .storage
            .get_mut(idx)
            .ok_or(HintingError::StorageOutOfBounds { index: idx, len })? = value;
        Ok(())
    }

    /// `WCVTP`: write a pixel value to the CVT.
    pub(crate) fn op_wcvtp(&mut self) -> Result<(), HintingError> {
        let value = self.pop()?;
        let idx = self.pop_uint()?;
        self.write_cvt(idx, value)
    }

    /// `WCVTF`: write a font-unit value to the CVT (scaled to pixels).
    pub(crate) fn op_wcvtf(&mut self) -> Result<(), HintingError> {
        let value = self.pop()?;
        let idx = self.pop_uint()?;
        let scaled = self.scale_funit(value);
        self.write_cvt(idx, scaled)
    }

    fn write_cvt(&mut self, idx: usize, value: i32) -> Result<(), HintingError> {
        let len = self.cvt.len();
        *self
            .cvt
            .get_mut(idx)
            .ok_or(HintingError::CvtOutOfBounds { index: idx, len })? = value;
        Ok(())
    }

    /// `RCVT`: read a CVT entry.
    pub(crate) fn op_rcvt(&mut self) -> Result<(), HintingError> {
        let idx = self.pop_uint()?;
        let v = *self.cvt.get(idx).ok_or(HintingError::CvtOutOfBounds {
            index: idx,
            len: self.cvt.len(),
        })?;
        self.push(v)
    }

    /// Read a CVT entry, bounds-checked (shared by MIAP/MIRP).
    pub(crate) fn cvt_get(&self, idx: usize) -> Result<i32, HintingError> {
        self.cvt
            .get(idx)
            .copied()
            .ok_or(HintingError::CvtOutOfBounds {
                index: idx,
                len: self.cvt.len(),
            })
    }

    // ── Measurement ─────────────────────────────────────────────────────────

    /// `GC[a]`: push a point's projected coordinate (`a=1` uses original).
    pub(crate) fn op_gc(&mut self, original: bool) -> Result<(), HintingError> {
        let p = self.pop_uint()?;
        let zp2 = self.gs.zp2;
        let value = if original {
            self.dual_project_org(zp2, p)?
        } else {
            self.project_cur(zp2, p)?
        };
        self.push(value)
    }

    /// `SCFS`: set a point's projected coordinate from the stack.
    pub(crate) fn op_scfs(&mut self) -> Result<(), HintingError> {
        let value = self.pop()?;
        let p = self.pop_uint()?;
        let zp2 = self.gs.zp2;
        let cur = self.project_cur(zp2, p)?;
        self.move_point(zp2, p, value - cur)?;
        Ok(())
    }

    /// `MD[a]`: push the distance between two points (`a=1` uses original).
    pub(crate) fn op_md(&mut self, original: bool) -> Result<(), HintingError> {
        let p1 = self.pop_uint()?;
        let p2 = self.pop_uint()?;
        let zp0 = self.gs.zp0;
        let zp1 = self.gs.zp1;
        let distance = if original {
            self.dual_project_org(zp0, p2)? - self.dual_project_org(zp1, p1)?
        } else {
            self.project_cur(zp0, p2)? - self.project_cur(zp1, p1)?
        };
        self.push(distance)
    }

    /// `MPPEM`: push the pixels-per-em.
    pub(crate) fn op_mppem(&mut self) -> Result<(), HintingError> {
        let ppem = self.ppem() as i32;
        self.push(ppem)
    }

    /// `MPS`: push the point size (modelled as equal to ppem).
    pub(crate) fn op_mps(&mut self) -> Result<(), HintingError> {
        let ppem = self.ppem() as i32;
        self.push(ppem)
    }

    // ── On-curve flag flips ─────────────────────────────────────────────────

    /// `FLIPPT`: toggle the on-curve flag of `loop_counter` points.
    pub(crate) fn op_flippt(&mut self) -> Result<(), HintingError> {
        let count = self.take_loop_counter();
        for _ in 0..count {
            let p = self.pop_uint()?;
            let point = self.point_mut(ZonePointer::Glyph, p)?;
            point.on_curve = !point.on_curve;
        }
        Ok(())
    }

    /// `FLIPRGON`: set the on-curve flag for a point range.
    pub(crate) fn op_fliprgon(&mut self, on: bool) -> Result<(), HintingError> {
        let high = self.pop_uint()?;
        let low = self.pop_uint()?;
        if high < low {
            return Ok(());
        }
        let zone = self.zone_mut(ZonePointer::Glyph);
        let len = zone.points.len();
        for p in low..=high {
            match zone.points.get_mut(p) {
                Some(point) => point.on_curve = on,
                None => {
                    return Err(HintingError::PointOutOfBounds {
                        zone: 1,
                        index: p,
                        len,
                    })
                }
            }
        }
        Ok(())
    }
}
