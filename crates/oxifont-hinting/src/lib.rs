#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `oxifont-hinting` — a Pure Rust TrueType bytecode hinting interpreter.
//!
//! This crate implements the TrueType *instruction set* — the stack-based
//! bytecode virtual machine that fonts use to grid-fit their outlines at a
//! given pixels-per-em (ppem) size. It executes the font program (`fpgm`), the
//! control-value program (`prep`), and per-glyph instruction streams over a
//! glyph's points and phantom points, producing grid-fitted 26.6 fixed-point
//! coordinates.
//!
//! # Design
//!
//! * [`HintingEngine`] is the entry point. It borrows nothing from the font
//!   after construction — it copies the hinting tables out of a
//!   [`SfntTableMap`](oxifont_core::sfnt::SfntTableMap), so it is fully
//!   self-contained.
//! * Construction runs `fpgm` once. [`HintingEngine::set_ppem`] scales the CVT
//!   and runs `prep`. [`HintingEngine::hint_glyph`] fits a single glyph.
//! * The output [`HintedGlyph`] exposes fitted points plus a
//!   [`HintedGlyph::to_outline`] convenience that decomposes them into the
//!   ecosystem's [`GlyphOutline`](oxifont_core::GlyphOutline) path commands.
//!
//! # Safety against adversarial fonts
//!
//! The VM **never panics** on malformed or hostile bytecode. Every stack,
//! storage, CVT, point, and jump access is bounds-checked and mapped to a typed
//! [`HintingError`]. Instruction count, call depth, and loop counts are all
//! bounded, so infinite loops and deep recursion terminate with an error rather
//! than hanging or overflowing the native stack.
//!
//! # Example
//!
//! ```no_run
//! use oxifont_core::sfnt::SfntTableMap;
//! use oxifont_hinting::HintingEngine;
//!
//! # fn run(font_bytes: &[u8]) -> Result<(), oxifont_hinting::HintingError> {
//! let map = SfntTableMap::parse(font_bytes)
//!     .map_err(oxifont_hinting::HintingError::from)?;
//! let mut engine = HintingEngine::new(&map)?;
//! engine.set_ppem(16)?;
//! let glyph = engine.hint_glyph(36)?; // grid-fit glyph id 36 at 16 ppem
//! for cmd in glyph.to_outline() {
//!     // feed `cmd` to a rasterizer …
//!     let _ = cmd;
//! }
//! # Ok(())
//! # }
//! ```

extern crate alloc;

mod dispatch;
mod font;
mod interp;
mod math;
mod opcodes;
mod ops_arith;
mod ops_iup;
mod ops_move;
mod ops_state;
mod state;

#[cfg(test)]
mod vm_tests;

pub mod error;

pub use error::HintingError;
pub use font::{FontProgram, GlyphPoints, MaxProfile};
pub use interp::{HintedGlyph, HintedPoint, HintingEngine};
pub use math::{F26Dot6, F2Dot14, RoundState, Vector};
pub use state::{GraphicsState, Point, Zone, ZonePointer};
