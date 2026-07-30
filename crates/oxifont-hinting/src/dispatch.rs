//! The simple-opcode dispatch table.
//!
//! Control-flow opcodes (`IF`/`ELSE`/`EIF`, `FDEF`/`ENDF`/`IDEF`, `CALL`,
//! `LOOPCALL`, `JMPR`/`JROT`/`JROF`) are handled in [`crate::interp`]; every
//! other opcode is routed here. Push instructions read their (already
//! bounds-validated) inline operands; all others delegate to a category handler.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::RoundState;
use crate::opcodes::{next_pc, OP_NPUSHB, OP_NPUSHW};

impl HintingEngine {
    /// Execute a non-control-flow opcode at `pc`, returning the next pc.
    pub(crate) fn exec_simple(
        &mut self,
        code: &[u8],
        pc: usize,
        op: u8,
    ) -> Result<usize, HintingError> {
        let end = next_pc(code, pc)?;
        match op {
            OP_NPUSHB => self.op_npushb(code, pc)?,
            OP_NPUSHW => self.op_npushw(code, pc)?,
            0xB0..=0xB7 => self.op_pushb(code, pc, (op - 0xB0) as usize + 1)?,
            0xB8..=0xBF => self.op_pushw(code, pc, (op - 0xB8) as usize + 1)?,
            _ => self.exec_op(op)?,
        }
        Ok(end)
    }

    /// Execute an opcode that consumes no inline stream operands.
    fn exec_op(&mut self, op: u8) -> Result<(), HintingError> {
        match op {
            // ── Vectors ──────────────────────────────────────────────────────
            0x00..=0x01 => self.op_svtca(op & 1 != 0),
            0x02..=0x03 => self.op_spvtca(op & 1 != 0),
            0x04..=0x05 => self.op_sfvtca(op & 1 != 0),
            0x06..=0x07 => self.op_spvtl(op & 1 != 0)?,
            0x08..=0x09 => self.op_sfvtl(op & 1 != 0)?,
            0x0A => self.op_spvfs()?,
            0x0B => self.op_sfvfs()?,
            0x0C => self.op_gpv()?,
            0x0D => self.op_gfv()?,
            0x0E => self.op_sfvtpv(),
            0x0F => self.op_isect()?,

            // ── Reference points / zones ────────────────────────────────────
            0x10 => self.op_srp(0)?,
            0x11 => self.op_srp(1)?,
            0x12 => self.op_srp(2)?,
            0x13 => self.op_szp(0)?,
            0x14 => self.op_szp(1)?,
            0x15 => self.op_szp(2)?,
            0x16 => self.op_szps()?,
            0x17 => self.op_sloop()?,

            // ── Rounding state ──────────────────────────────────────────────
            0x18 => self.set_round(RoundState::grid()),
            0x19 => self.set_round(RoundState::half_grid()),
            0x1A => self.op_smd()?,
            0x1D => self.op_scvtci()?,
            0x1E => self.op_sswci()?,
            0x1F => self.op_ssw()?,

            // ── Stack ────────────────────────────────────────────────────────
            0x20 => self.op_dup()?,
            0x21 => self.op_pop()?,
            0x22 => self.op_clear(),
            0x23 => self.op_swap()?,
            0x24 => self.op_depth()?,
            0x25 => self.op_cindex()?,
            0x26 => self.op_mindex()?,

            // ── Point alignment / movement ──────────────────────────────────
            0x27 => self.op_alignpts()?,
            0x29 => self.op_utp()?,
            0x2E..=0x2F => self.op_mdap(op & 1 != 0)?,
            0x30..=0x31 => self.op_iup(op & 1 != 0)?,
            0x32..=0x33 => self.op_shp(op & 1 != 0)?,
            0x34..=0x35 => self.op_shc(op & 1 != 0)?,
            0x36..=0x37 => self.op_shz(op & 1 != 0)?,
            0x38 => self.op_shpix()?,
            0x39 => self.op_ip()?,
            0x3A..=0x3B => self.op_msirp(op & 1 != 0)?,
            0x3C => self.op_alignrp()?,
            0x3D => self.set_round(RoundState::double_grid()),
            0x3E..=0x3F => self.op_miap(op & 1 != 0)?,

            // ── Storage / CVT ───────────────────────────────────────────────
            0x42 => self.op_ws()?,
            0x43 => self.op_rs()?,
            0x44 => self.op_wcvtp()?,
            0x45 => self.op_rcvt()?,

            // ── Measurement ─────────────────────────────────────────────────
            0x46..=0x47 => self.op_gc(op & 1 != 0)?,
            0x48 => self.op_scfs()?,
            0x49..=0x4A => self.op_md(op & 1 != 0)?,
            0x4B => self.op_mppem()?,
            0x4C => self.op_mps()?,
            0x4D => self.op_flip_auto(true),
            0x4E => self.op_flip_auto(false),
            0x4F => {
                // DEBUG: pop the debug code and ignore it.
                self.pop()?;
            }

            // ── Comparison / logic ──────────────────────────────────────────
            0x50 => self.op_lt()?,
            0x51 => self.op_lteq()?,
            0x52 => self.op_gt()?,
            0x53 => self.op_gteq()?,
            0x54 => self.op_eq()?,
            0x55 => self.op_neq()?,
            0x56 => self.op_odd()?,
            0x57 => self.op_even()?,
            0x5A => self.op_and()?,
            0x5B => self.op_or()?,
            0x5C => self.op_not()?,

            // ── Delta (points) ──────────────────────────────────────────────
            0x5D => self.op_delta(0, false)?,
            0x5E => self.op_sdb()?,
            0x5F => self.op_sds()?,

            // ── Arithmetic ──────────────────────────────────────────────────
            0x60 => self.op_add()?,
            0x61 => self.op_sub()?,
            0x62 => self.op_div()?,
            0x63 => self.op_mul()?,
            0x64 => self.op_abs()?,
            0x65 => self.op_neg()?,
            0x66 => self.op_floor()?,
            0x67 => self.op_ceiling()?,
            0x68..=0x6B => self.op_round()?,
            0x6C..=0x6F => self.op_nround()?,

            // ── CVT write / more deltas ─────────────────────────────────────
            0x70 => self.op_wcvtf()?,
            0x71 => self.op_delta(16, false)?,
            0x72 => self.op_delta(32, false)?,
            0x73 => self.op_delta(0, true)?,
            0x74 => self.op_delta(16, true)?,
            0x75 => self.op_delta(32, true)?,

            // ── Rounding modes ──────────────────────────────────────────────
            0x76 => self.op_sround()?,
            0x77 => self.op_s45round()?,
            0x7A => self.set_round(RoundState::off()),
            0x7C => self.set_round(RoundState::up_to_grid()),
            0x7D => self.set_round(RoundState::down_to_grid()),

            // ── Deprecated no-ops (consume their operand) ───────────────────
            0x7E => {
                self.pop()?; // SANGW
            }
            0x7F => {
                self.pop()?; // AA
            }

            // ── On-curve flag flips ─────────────────────────────────────────
            0x80 => self.op_flippt()?,
            0x81 => self.op_fliprgon(true)?,
            0x82 => self.op_fliprgon(false)?,

            // ── Scan conversion / info ──────────────────────────────────────
            0x85 => self.op_scanctrl()?,
            0x86..=0x87 => self.op_sdpvtl(op & 1 != 0)?,
            0x88 => self.op_getinfo()?,
            0x8A => self.op_roll()?,
            0x8B => self.op_max()?,
            0x8C => self.op_min()?,
            0x8D => self.op_scantype()?,
            0x8E => self.op_instctrl()?,

            // ── Managed relative moves ──────────────────────────────────────
            0xC0..=0xDF => self.op_mdrp(op & 0x1F)?,
            0xE0..=0xFF => self.op_mirp(op & 0x1F)?,

            // ── User-defined or reserved ────────────────────────────────────
            _ => {
                if let Some(body) = self.instruction_defs.get(&op).cloned() {
                    self.execute(&body)?;
                } else {
                    return Err(HintingError::InvalidOpcode(op));
                }
            }
        }
        Ok(())
    }
}
