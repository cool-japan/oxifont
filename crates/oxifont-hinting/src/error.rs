//! Error type for the TrueType hinting interpreter.
//!
//! Every fallible operation in this crate returns a [`HintingError`]. The VM is
//! designed so that **no malformed or adversarial font program can panic** — all
//! stack, storage, CVT, function, and jump operations are bounds-checked and map
//! to a typed variant here instead of unwinding.

use oxifont_core::sfnt::SfntError;

/// Errors produced while loading a font's hinting tables or executing bytecode.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so future variants can be added in minor versions without a
/// semver break.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HintingError {
    /// The SFNT table directory could not be parsed.
    Sfnt(SfntError),
    /// A required table (`head`, `maxp`, `glyf`, `loca`, ...) is missing.
    MissingTable([u8; 4]),
    /// A table was present but too short / structurally invalid.
    MalformedTable {
        /// The 4-byte tag of the offending table.
        tag: [u8; 4],
        /// A human-readable reason.
        reason: &'static str,
    },
    /// A glyph id was outside the range described by `maxp`/`loca`.
    GlyphOutOfRange {
        /// The requested glyph id.
        gid: u16,
        /// The number of glyphs in the font.
        count: u16,
    },
    /// Composite-glyph nesting exceeded the safety bound.
    CompositeTooDeep,
    /// The interpreter popped from an empty operand stack.
    StackUnderflow,
    /// The operand stack grew beyond `maxp.maxStackElements` (guarded bound).
    StackOverflow,
    /// A storage-area index was out of bounds.
    StorageOutOfBounds {
        /// The requested storage index.
        index: usize,
        /// The storage-area length.
        len: usize,
    },
    /// A CVT index was out of bounds.
    CvtOutOfBounds {
        /// The requested CVT index.
        index: usize,
        /// The CVT length.
        len: usize,
    },
    /// A point index was out of bounds for the referenced zone.
    PointOutOfBounds {
        /// The zone number (0 = twilight, 1 = glyph).
        zone: u8,
        /// The requested point index.
        index: usize,
        /// The number of points in that zone.
        len: usize,
    },
    /// A `CALL`/`LOOPCALL` referenced an undefined function number.
    UndefinedFunction(u32),
    /// Function-call recursion exceeded the safety bound.
    CallDepthExceeded,
    /// The instruction stream jumped or advanced outside the program bounds.
    ProgramCounterOutOfBounds,
    /// A push instruction requested more inline bytes than the stream holds.
    TruncatedInstruction,
    /// An `IF`/`ELSE`/`EIF` or `FDEF`/`ENDF` block was not balanced.
    UnbalancedBlock,
    /// An unknown or reserved opcode was encountered.
    InvalidOpcode(u8),
    /// The total executed-instruction budget was exhausted (loop guard).
    ExecutionBudgetExceeded,
    /// A division by zero was requested (`DIV`).
    DivideByZero,
    /// The requested pixels-per-em value was zero or non-finite.
    InvalidPpem,
}

impl core::fmt::Display for HintingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HintingError::Sfnt(e) => write!(f, "SFNT parse error: {e}"),
            HintingError::MissingTable(tag) => {
                write!(f, "missing required table '{}'", tag_str(tag))
            }
            HintingError::MalformedTable { tag, reason } => {
                write!(f, "malformed table '{}': {reason}", tag_str(tag))
            }
            HintingError::GlyphOutOfRange { gid, count } => {
                write!(f, "glyph id {gid} out of range (count={count})")
            }
            HintingError::CompositeTooDeep => write!(f, "composite glyph nesting too deep"),
            HintingError::StackUnderflow => write!(f, "operand stack underflow"),
            HintingError::StackOverflow => write!(f, "operand stack overflow"),
            HintingError::StorageOutOfBounds { index, len } => {
                write!(f, "storage index {index} out of bounds (len={len})")
            }
            HintingError::CvtOutOfBounds { index, len } => {
                write!(f, "CVT index {index} out of bounds (len={len})")
            }
            HintingError::PointOutOfBounds { zone, index, len } => {
                write!(
                    f,
                    "point index {index} out of bounds in zone {zone} (len={len})"
                )
            }
            HintingError::UndefinedFunction(n) => write!(f, "undefined function {n}"),
            HintingError::CallDepthExceeded => write!(f, "function call depth exceeded"),
            HintingError::ProgramCounterOutOfBounds => {
                write!(f, "program counter out of bounds")
            }
            HintingError::TruncatedInstruction => write!(f, "truncated push instruction"),
            HintingError::UnbalancedBlock => {
                write!(f, "unbalanced IF/ELSE/EIF or FDEF/ENDF block")
            }
            HintingError::InvalidOpcode(op) => write!(f, "invalid or reserved opcode {op:#04x}"),
            HintingError::ExecutionBudgetExceeded => {
                write!(f, "instruction execution budget exceeded")
            }
            HintingError::DivideByZero => write!(f, "division by zero"),
            HintingError::InvalidPpem => write!(f, "invalid pixels-per-em value"),
        }
    }
}

impl std::error::Error for HintingError {}

impl From<SfntError> for HintingError {
    fn from(e: SfntError) -> Self {
        HintingError::Sfnt(e)
    }
}

fn tag_str(tag: &[u8; 4]) -> &str {
    core::str::from_utf8(tag).unwrap_or("????")
}
