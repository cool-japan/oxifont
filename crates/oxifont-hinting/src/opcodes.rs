//! Opcode constants and instruction-stream navigation helpers.
//!
//! The helpers here let the interpreter skip over variable-length `PUSH`
//! instructions correctly when scanning `IF`/`ELSE`/`EIF` and `FDEF`/`ENDF`
//! blocks, without which forward jumps could land in the middle of inline push
//! data.

use crate::error::HintingError;

// Control-flow and definition opcodes handled directly by the main loop.
pub const OP_IF: u8 = 0x58;
pub const OP_ELSE: u8 = 0x1B;
pub const OP_EIF: u8 = 0x59;
pub const OP_JMPR: u8 = 0x1C;
pub const OP_JROT: u8 = 0x78;
pub const OP_JROF: u8 = 0x79;
pub const OP_FDEF: u8 = 0x2C;
pub const OP_ENDF: u8 = 0x2D;
pub const OP_IDEF: u8 = 0x89;
pub const OP_CALL: u8 = 0x2B;
pub const OP_LOOPCALL: u8 = 0x2A;

// Push opcodes (variable length).
pub const OP_NPUSHB: u8 = 0x40;
pub const OP_NPUSHW: u8 = 0x41;
pub const OP_PUSHB_LO: u8 = 0xB0;
pub const OP_PUSHB_HI: u8 = 0xB7;
pub const OP_PUSHW_LO: u8 = 0xB8;
pub const OP_PUSHW_HI: u8 = 0xBF;

/// Number of stream bytes the instruction at `pc` occupies, including any inline
/// push operands. Returns the index of the following instruction.
///
/// This is the single source of truth for stream navigation used by both the
/// executor and the block-skipping helpers.
pub fn next_pc(code: &[u8], pc: usize) -> Result<usize, HintingError> {
    let op = *code
        .get(pc)
        .ok_or(HintingError::ProgramCounterOutOfBounds)?;
    let len = match op {
        OP_NPUSHB => {
            let n = *code.get(pc + 1).ok_or(HintingError::TruncatedInstruction)? as usize;
            2 + n
        }
        OP_NPUSHW => {
            let n = *code.get(pc + 1).ok_or(HintingError::TruncatedInstruction)? as usize;
            2 + 2 * n
        }
        OP_PUSHB_LO..=OP_PUSHB_HI => 1 + (op - OP_PUSHB_LO) as usize + 1,
        OP_PUSHW_LO..=OP_PUSHW_HI => 1 + 2 * ((op - OP_PUSHW_LO) as usize + 1),
        _ => 1,
    };
    let end = pc
        .checked_add(len)
        .ok_or(HintingError::TruncatedInstruction)?;
    if end > code.len() {
        return Err(HintingError::TruncatedInstruction);
    }
    Ok(end)
}

/// Starting just after an `IF` at `pc`, find the matching `ELSE` or `EIF`.
///
/// Returns the pc **after** a matching top-level `ELSE` (so execution resumes in
/// the else-branch) or **after** the matching `EIF` when there is no else. Nested
/// `IF` blocks are tracked so only the balanced partner is matched.
pub fn skip_to_else_or_eif(code: &[u8], pc: usize) -> Result<usize, HintingError> {
    let mut depth = 0usize;
    let mut cursor = next_pc(code, pc)?; // step past the IF itself
    while cursor < code.len() {
        let op = code[cursor];
        let after = next_pc(code, cursor)?;
        match op {
            OP_IF => depth += 1,
            OP_ELSE if depth == 0 => return Ok(after),
            OP_EIF if depth == 0 => return Ok(after),
            OP_EIF => depth -= 1,
            _ => {}
        }
        cursor = after;
    }
    Err(HintingError::UnbalancedBlock)
}

/// Starting at an `ELSE` at `pc`, find the pc after the matching `EIF`.
pub fn skip_past_eif(code: &[u8], pc: usize) -> Result<usize, HintingError> {
    let mut depth = 0usize;
    let mut cursor = next_pc(code, pc)?; // step past the ELSE
    while cursor < code.len() {
        let op = code[cursor];
        let after = next_pc(code, cursor)?;
        match op {
            OP_IF => depth += 1,
            OP_EIF if depth == 0 => return Ok(after),
            OP_EIF => depth -= 1,
            _ => {}
        }
        cursor = after;
    }
    Err(HintingError::UnbalancedBlock)
}

/// Starting at an `FDEF`/`IDEF` at `pc`, return `(body_start, endf_end)` where
/// `body_start..endf_start` is the function body (excluding the trailing `ENDF`)
/// and `endf_end` is the pc after the `ENDF`.
pub fn scan_function(code: &[u8], pc: usize) -> Result<(usize, usize, usize), HintingError> {
    let body_start = next_pc(code, pc)?;
    let mut cursor = body_start;
    while cursor < code.len() {
        let op = code[cursor];
        let after = next_pc(code, cursor)?;
        if op == OP_ENDF {
            return Ok((body_start, cursor, after));
        }
        // Nested FDEF is illegal in TrueType; treat as malformed.
        if op == OP_FDEF {
            return Err(HintingError::UnbalancedBlock);
        }
        cursor = after;
    }
    Err(HintingError::UnbalancedBlock)
}

/// Apply a relative `JMPR`/`JROT`/`JROF` jump of `offset` bytes from `pc`.
pub fn apply_jump(pc: usize, offset: i32, code_len: usize) -> Result<usize, HintingError> {
    let target = pc as i64 + offset as i64;
    if target < 0 || target > code_len as i64 {
        return Err(HintingError::ProgramCounterOutOfBounds);
    }
    Ok(target as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_pc_handles_pushb() {
        // PUSHB[2] pushes 3 bytes -> total length 4.
        let code = [0xB2u8, 1, 2, 3, 0x00];
        assert_eq!(next_pc(&code, 0).unwrap(), 4);
        assert_eq!(next_pc(&code, 4).unwrap(), 5);
    }

    #[test]
    fn next_pc_handles_npushw() {
        // NPUSHW, count=2 -> 2 + 2*2 = 6 bytes.
        let code = [0x41u8, 2, 0, 1, 0, 2];
        assert_eq!(next_pc(&code, 0).unwrap(), 6);
    }

    #[test]
    fn next_pc_rejects_truncated_push() {
        let code = [0xB5u8, 1, 2]; // PUSHB[5] wants 6 bytes, only 2 present
        assert!(next_pc(&code, 0).is_err());
    }

    #[test]
    fn skip_if_finds_else() {
        // IF <nop> ELSE <nop> EIF
        let code = [OP_IF, 0x00, OP_ELSE, 0x00, OP_EIF];
        // after IF: else-branch starts after ELSE at index 3.
        assert_eq!(skip_to_else_or_eif(&code, 0).unwrap(), 3);
    }

    #[test]
    fn skip_if_nested() {
        // IF IF EIF ELSE EIF  -> outer else must skip the inner IF/EIF.
        let code = [OP_IF, OP_IF, OP_EIF, OP_ELSE, OP_EIF];
        assert_eq!(skip_to_else_or_eif(&code, 0).unwrap(), 4);
    }

    #[test]
    fn scan_function_locates_endf() {
        // FDEF <nop> <nop> ENDF
        let code = [OP_FDEF, 0x00, 0x00, OP_ENDF];
        let (start, endf, end) = scan_function(&code, 0).unwrap();
        assert_eq!((start, endf, end), (1, 3, 4));
    }

    #[test]
    fn scan_function_unbalanced_errors() {
        let code = [OP_FDEF, 0x00, 0x00];
        assert!(scan_function(&code, 0).is_err());
    }
}
