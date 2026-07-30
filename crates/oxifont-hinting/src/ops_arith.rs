//! Push, stack, arithmetic, logic, and rounding opcode handlers.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::{f26dot6_div, f26dot6_mul, RoundState};

impl HintingEngine {
    // ── Push instructions (inline operands already bounds-validated) ─────────

    /// `NPUSHB`: push `n` unsigned bytes read from the stream.
    pub(crate) fn op_npushb(&mut self, code: &[u8], pc: usize) -> Result<(), HintingError> {
        let n = code[pc + 1] as usize;
        for i in 0..n {
            self.push(code[pc + 2 + i] as i32)?;
        }
        Ok(())
    }

    /// `NPUSHW`: push `n` signed words read from the stream.
    pub(crate) fn op_npushw(&mut self, code: &[u8], pc: usize) -> Result<(), HintingError> {
        let n = code[pc + 1] as usize;
        for i in 0..n {
            let hi = code[pc + 2 + i * 2];
            let lo = code[pc + 2 + i * 2 + 1];
            self.push(i16::from_be_bytes([hi, lo]) as i32)?;
        }
        Ok(())
    }

    /// `PUSHB[n]`: push `count` unsigned bytes read from the stream.
    pub(crate) fn op_pushb(
        &mut self,
        code: &[u8],
        pc: usize,
        count: usize,
    ) -> Result<(), HintingError> {
        for i in 0..count {
            self.push(code[pc + 1 + i] as i32)?;
        }
        Ok(())
    }

    /// `PUSHW[n]`: push `count` signed words read from the stream.
    pub(crate) fn op_pushw(
        &mut self,
        code: &[u8],
        pc: usize,
        count: usize,
    ) -> Result<(), HintingError> {
        for i in 0..count {
            let hi = code[pc + 1 + i * 2];
            let lo = code[pc + 1 + i * 2 + 1];
            self.push(i16::from_be_bytes([hi, lo]) as i32)?;
        }
        Ok(())
    }

    // ── Stack manipulation ──────────────────────────────────────────────────

    /// `DUP`: duplicate the top stack element.
    pub(crate) fn op_dup(&mut self) -> Result<(), HintingError> {
        let v = *self.stack.last().ok_or(HintingError::StackUnderflow)?;
        self.push(v)
    }

    /// `POP`: discard the top stack element.
    pub(crate) fn op_pop(&mut self) -> Result<(), HintingError> {
        self.pop().map(|_| ())
    }

    /// `CLEAR`: empty the stack.
    pub(crate) fn op_clear(&mut self) {
        self.stack.clear();
    }

    /// `SWAP`: exchange the top two stack elements.
    pub(crate) fn op_swap(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        let b = self.pop()?;
        self.push(a)?;
        self.push(b)
    }

    /// `DEPTH`: push the current stack depth.
    pub(crate) fn op_depth(&mut self) -> Result<(), HintingError> {
        let d = self.stack.len() as i32;
        self.push(d)
    }

    /// `CINDEX`: copy the `k`-th element (1-based, from top) to the top.
    pub(crate) fn op_cindex(&mut self) -> Result<(), HintingError> {
        let k = self.pop()?;
        if k <= 0 || (k as usize) > self.stack.len() {
            return Err(HintingError::StackUnderflow);
        }
        let v = self.stack[self.stack.len() - k as usize];
        self.push(v)
    }

    /// `MINDEX`: move the `k`-th element (1-based, from top) to the top.
    pub(crate) fn op_mindex(&mut self) -> Result<(), HintingError> {
        let k = self.pop()?;
        if k <= 0 || (k as usize) > self.stack.len() {
            return Err(HintingError::StackUnderflow);
        }
        let idx = self.stack.len() - k as usize;
        let v = self.stack.remove(idx);
        self.push(v)
    }

    /// `ROLL`: rotate the top three elements `(a b c)` → `(b c a)`.
    pub(crate) fn op_roll(&mut self) -> Result<(), HintingError> {
        let c = self.pop()?;
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(b)?;
        self.push(c)?;
        self.push(a)
    }

    // ── Arithmetic (26.6 fixed point) ───────────────────────────────────────

    /// `ADD`.
    pub(crate) fn op_add(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(a.wrapping_add(b))
    }

    /// `SUB`.
    pub(crate) fn op_sub(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(a.wrapping_sub(b))
    }

    /// `MUL`.
    pub(crate) fn op_mul(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(f26dot6_mul(a, b))
    }

    /// `DIV` (errors on a zero divisor).
    pub(crate) fn op_div(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        if b == 0 {
            return Err(HintingError::DivideByZero);
        }
        self.push(f26dot6_div(a, b))
    }

    /// `ABS`.
    pub(crate) fn op_abs(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        let v = (a as i64).unsigned_abs().min(i32::MAX as u64) as i32;
        self.push(v)
    }

    /// `NEG`.
    pub(crate) fn op_neg(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        self.push(a.wrapping_neg())
    }

    /// `FLOOR`: round toward negative infinity to a whole pixel.
    pub(crate) fn op_floor(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        self.push(a & !63)
    }

    /// `CEILING`: round toward positive infinity to a whole pixel.
    pub(crate) fn op_ceiling(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        self.push(a.wrapping_add(63) & !63)
    }

    /// `MAX`.
    pub(crate) fn op_max(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(a.max(b))
    }

    /// `MIN`.
    pub(crate) fn op_min(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push(a.min(b))
    }

    // ── Logic and comparison ────────────────────────────────────────────────

    /// Push a boolean as `1`/`0`.
    fn push_bool(&mut self, b: bool) -> Result<(), HintingError> {
        self.push(if b { 1 } else { 0 })
    }

    /// `LT`.
    pub(crate) fn op_lt(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a < b)
    }

    /// `LTEQ`.
    pub(crate) fn op_lteq(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a <= b)
    }

    /// `GT`.
    pub(crate) fn op_gt(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a > b)
    }

    /// `GTEQ`.
    pub(crate) fn op_gteq(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a >= b)
    }

    /// `EQ`.
    pub(crate) fn op_eq(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a == b)
    }

    /// `NEQ`.
    pub(crate) fn op_neq(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a != b)
    }

    /// `AND`.
    pub(crate) fn op_and(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a != 0 && b != 0)
    }

    /// `OR`.
    pub(crate) fn op_or(&mut self) -> Result<(), HintingError> {
        let b = self.pop()?;
        let a = self.pop()?;
        self.push_bool(a != 0 || b != 0)
    }

    /// `NOT`.
    pub(crate) fn op_not(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        self.push_bool(a == 0)
    }

    /// `ODD`: round then test whether the whole-pixel result is odd.
    pub(crate) fn op_odd(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        let r = self.gs.round_state.round(a, 0);
        self.push_bool(((r >> 6) & 1) != 0)
    }

    /// `EVEN`: round then test whether the whole-pixel result is even.
    pub(crate) fn op_even(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        let r = self.gs.round_state.round(a, 0);
        self.push_bool(((r >> 6) & 1) == 0)
    }

    // ── Rounding ────────────────────────────────────────────────────────────

    /// `ROUND[ab]`: round the top value using the current rounding state.
    pub(crate) fn op_round(&mut self) -> Result<(), HintingError> {
        let a = self.pop()?;
        let r = self.gs.round_state.round(a, 0);
        self.push(r)
    }

    /// `NROUND[ab]`: apply engine compensation without rounding (identity here).
    pub(crate) fn op_nround(&mut self) -> Result<(), HintingError> {
        // No engine/color compensation is modelled, so NROUND is a pass-through.
        Ok(())
    }

    /// Set the rounding state to a fixed mode.
    pub(crate) fn set_round(&mut self, state: RoundState) {
        self.gs.round_state = state;
    }

    /// `SROUND`: configure a super-round from the popped selector.
    pub(crate) fn op_sround(&mut self) -> Result<(), HintingError> {
        let n = self.pop()?;
        self.gs.round_state = RoundState::super_round(64, n as u8);
        Ok(())
    }

    /// `S45ROUND`: configure a 45-degree super-round from the popped selector.
    pub(crate) fn op_s45round(&mut self) -> Result<(), HintingError> {
        let n = self.pop()?;
        self.gs.round_state = RoundState::super_round(RoundState::S45_BASE_PERIOD, n as u8);
        Ok(())
    }
}
