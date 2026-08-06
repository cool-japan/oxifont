# oxifont-hinting — Pure-Rust TrueType bytecode hinting interpreter for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-hinting.svg)](https://crates.io/crates/oxifont-hinting)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-alpha-orange.svg)](../../CHANGELOG.md)

`oxifont-hinting` implements the TrueType *instruction set* — the stack-based bytecode virtual machine that fonts use to grid-fit their outlines at a given pixels-per-em (ppem) size. It executes a font's font program (`fpgm`), control-value program (`prep`), and per-glyph instruction streams over a glyph's points and phantom points, producing grid-fitted 26.6 fixed-point coordinates that a rasterizer can turn into crisp, hinted pixels.

`HintingEngine` is the entry point. It copies the hinting-relevant tables out of a `SfntTableMap` at construction time, so it borrows nothing from the font afterward and is fully self-contained. Construction runs `fpgm` once; `set_ppem` scales the CVT and runs `prep` for a given size; `hint_glyph` fits a single glyph and returns its fitted points plus advance width. The crate is `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`, and depends on nothing but [`oxifont-core`](../oxifont-core).

## Installation

```toml
[dependencies]
oxifont-hinting = "0.2.2"
```

## Quick Start

```rust,no_run
use oxifont_core::sfnt::SfntTableMap;
use oxifont_hinting::HintingEngine;

# fn run() -> Result<(), oxifont_hinting::HintingError> {
let font_bytes = std::fs::read("NotoSans-Bold.ttf").expect("read font file");
let map = SfntTableMap::parse(&font_bytes).map_err(oxifont_hinting::HintingError::from)?;

let mut engine = HintingEngine::new(&map)?;
engine.set_ppem(16)?;
let glyph = engine.hint_glyph(36)?; // grid-fit glyph id 36 at 16 ppem

println!("advance = {:.2}px", glyph.advance_px());
for cmd in glyph.to_outline() {
    // feed `cmd` to a rasterizer …
    let _ = cmd;
}
# Ok(())
# }
```

### Reusing one engine across sizes and glyphs

`HintingEngine` is built once per font and reused: `set_ppem` re-scales the CVT from font units and re-runs `prep` from a clean state every time it's called, and `hint_glyph` can be called repeatedly at that size.

```rust,no_run
use oxifont_core::sfnt::SfntTableMap;
use oxifont_hinting::HintingEngine;

# fn run() -> Result<(), oxifont_hinting::HintingError> {
let font_bytes = std::fs::read("NotoSans-Bold.ttf").expect("read font file");
let map = SfntTableMap::parse(&font_bytes).map_err(oxifont_hinting::HintingError::from)?;
let mut engine = HintingEngine::new(&map)?;

for ppem in [11u16, 16, 24, 48] {
    engine.set_ppem(ppem)?;
    for gid in [36u16, 68, 100] {
        let glyph = engine.hint_glyph(gid)?;
        for point in &glyph.points {
            let (_x, _y) = (point.x_px(), point.y_px());
        }
    }
}
# Ok(())
# }
```

## API Overview

### `HintingEngine` — the entry point

| Method | Description |
|--------|-------------|
| `HintingEngine::new(map: &SfntTableMap) -> Result<Self, HintingError>` | Load a font's hinting tables and run `fpgm` once |
| `.set_ppem(ppem: u16) -> Result<(), HintingError>` | Scale the CVT and run `prep` at a pixels-per-em size |
| `.hint_glyph(gid: u16) -> Result<HintedGlyph, HintingError>` | Grid-fit one glyph at the current ppem |
| `.ppem() -> u16` | The current pixels-per-em |
| `.font() -> &FontProgram` | Access the loaded font tables |

### `HintedGlyph` / `HintedPoint` — fitted output

| Item | Description |
|------|-------------|
| `HintedGlyph.points: Vec<HintedPoint>` | Fitted contour points (phantom points excluded) |
| `HintedGlyph.contour_ends: Vec<u16>` | Inclusive contour end-point indices into `points` |
| `HintedGlyph.advance: F26Dot6` | Fitted horizontal advance, in 26.6 fixed point |
| `HintedGlyph::advance_px(&self) -> f32` | Fitted advance in floating-point pixels |
| `HintedGlyph::to_outline(&self) -> Vec<oxifont_core::GlyphOutline>` | Decompose fitted points into quadratic path commands |
| `HintedPoint.x` / `.y: F26Dot6` | Fitted coordinates in 26.6 fixed point |
| `HintedPoint.on_curve: bool` | Whether the point lies on the curve (vs. a Bézier control point) |
| `HintedPoint::x_px(&self)` / `::y_px(&self) -> f32` | Coordinates in floating-point pixels |

### Lower-level building blocks

Exposed for callers who need direct access to loaded font-program data or the fixed-point / graphics-state types — for example, to inspect why a particular glyph fits the way it does:

| Item | Description |
|------|-------------|
| `FontProgram` | Loaded `fpgm`/`prep`/CVT bytecode plus enough of `head`/`hhea`/`hmtx`/`loca`/`glyf` to decode glyph points |
| `GlyphPoints` | A single glyph's decoded, unscaled (font-unit) outline points and instruction stream |
| `MaxProfile` | The `maxp`-derived resource limits used to size the VM (storage, twilight points, stack depth, …) |
| `F26Dot6` / `F2Dot14` | The fixed-point type aliases used throughout (`i32`-backed, 26.6 and 2.14 fractional bits respectively) |
| `Vector` | A 2.14 unit vector (projection / freedom / dual-projection) |
| `RoundState` | The active rounding mode (`RTG`, `RTHG`, `SROUND`, …), expressed as a super-round `(period, phase, threshold)` triple |
| `GraphicsState` | The full TrueType graphics state (zone pointers, reference points, rounding, cut-ins, …) |
| `Point` / `Zone` / `ZonePointer` | A single tracked point, a zone of points (twilight or glyph), and the zone selector |

## Safety Against Adversarial Fonts

The VM **never panics** on malformed or hostile bytecode. Every stack, storage, CVT, point, and jump access is bounds-checked and mapped to a typed `HintingError` instead of unwinding. Execution is bounded on every axis an adversarial program could abuse:

- **Instruction budget** — each `fpgm`/`prep`/glyph run is capped at a fixed number of executed instructions (today's implementation allows 8,000,000 per run); runaway or intentionally infinite loops terminate with `HintingError::ExecutionBudgetExceeded` instead of hanging.
- **Call depth** — `CALL`/`LOOPCALL`/`FDEF` recursion is capped (128 frames today); deep or self-recursive function chains terminate with `HintingError::CallDepthExceeded` instead of overflowing the native stack.
- **Stack size** — the operand stack is derived from `maxp.maxStackElements`, floored at a safe minimum and hard-capped independently of it, rejecting adversarial growth with `HintingError::StackOverflow`.
- **Loop counters** (`SLOOP`) are clamped to a bounded, non-negative range before use, so a crafted loop count cannot force an unbounded pop loop.
- **Composite nesting** is depth-limited when decoding composite glyphs, independent of bytecode execution.

`tests/real_fonts.rs` exercises this against real bundled fonts — not synthetic edge cases — across an entire font's glyph set, asserting exactly two things on every glyph: no panic, and finite, bounded output.

## What It Does *Not* Do

- **No rasterization.** `hint_glyph` produces fitted point coordinates — a grid-fitted outline — not pixel coverage or a bitmap. Feed `HintedGlyph::to_outline()` to a rasterizer to get pixels.
- **No CFF/PostScript hinting.** Only TrueType (`glyf`/`loca`) outlines are decoded and fitted; CFF/CFF2-outlined fonts (which have neither table) are out of scope for this crate.
- **No OpenType Layout.** Shaping (`GSUB`/`GPOS`) is a separate concern, handled upstream of hinting.

## Errors

`HintingError` is `#[non_exhaustive]`; `match` expressions must include a catch-all arm so future variants don't break downstream code.

| Variant | Cause |
|---------|-------|
| `Sfnt(SfntError)` | The SFNT table directory could not be parsed |
| `MissingTable([u8; 4])` | A required table (`head`, `maxp`, `glyf`, `loca`, …) is missing |
| `MalformedTable { tag, reason }` | A table was present but too short / structurally invalid |
| `GlyphOutOfRange { gid, count }` | A glyph id was outside the range described by `maxp`/`loca` |
| `CompositeTooDeep` | Composite-glyph nesting exceeded the safety bound |
| `StackUnderflow` / `StackOverflow` | The operand stack popped empty / grew past the guarded bound |
| `StorageOutOfBounds { index, len }` | A storage-area index was out of bounds |
| `CvtOutOfBounds { index, len }` | A CVT index was out of bounds |
| `PointOutOfBounds { zone, index, len }` | A point index was out of bounds for the referenced zone |
| `UndefinedFunction(u32)` | `CALL`/`LOOPCALL` referenced an undefined function number |
| `CallDepthExceeded` | Function-call recursion exceeded the safety bound |
| `ProgramCounterOutOfBounds` | The instruction stream jumped or advanced outside the program bounds |
| `TruncatedInstruction` | A push instruction requested more inline bytes than the stream holds |
| `UnbalancedBlock` | An `IF`/`ELSE`/`EIF` or `FDEF`/`ENDF` block was not balanced |
| `InvalidOpcode(u8)` | An unknown or reserved opcode was encountered |
| `ExecutionBudgetExceeded` | The total executed-instruction budget was exhausted (loop guard) |
| `DivideByZero` | A `DIV` requested division by zero |
| `InvalidPpem` | The requested pixels-per-em value was zero |

## Status

New crate for the 0.2.1 release, so it's marked **Alpha** while its API sees real-world use — but it is thoroughly tested: 60 passing tests including a real-font integration suite (`tests/real_fonts.rs`) that runs the VM across bundled Noto fonts at multiple sizes and stress-tests an entire glyph set, plus 1 passing doctest. Since 0.2.2 the [`oxifont`](../oxifont) facade re-exports this crate as `oxifont::hinting` behind its `hinting` feature (along with the `oxifont::hinted_outline` one-shot convenience wrapper); depend on `oxifont-hinting` directly for the full engine API.

## Testing

```bash
cargo nextest run -p oxifont-hinting --all-features
```

60 tests passing, 0 failed, 0 skipped: fixed-point math and rounding-mode unit tests, instruction-stream navigation tests, opcode-level VM tests, and an integration suite that grid-fits real bundled Noto Sans fonts (both hinted and unhinted) across multiple ppem sizes, checks fitting is deterministic, confirms hinting actually snaps points to the pixel grid, verifies `to_outline()` shape consistency, and stress-runs every glyph in a font with "no panic, bounded output" as the only assertion. Plus 1 passing doctest (the crate's own `lib.rs` example).

## Cross-references

- [`oxifont-core`](../oxifont-core) — `SfntTableMap` (input) and `GlyphOutline` (the path-command type `to_outline()` produces)
- [`oxifont-parser`](../oxifont-parser) — typical source of the raw font bytes fed to `SfntTableMap::parse`; also a dev-dependency of this crate's test suite
- [`oxifont-bundled`](../oxifont-bundled) — supplies the real Noto fonts used by `tests/real_fonts.rs` (`bundled-noto` feature, dev-dependency only)
- [`oxifont`](../oxifont) — the top-level façade crate; re-exports this crate as `oxifont::hinting` behind its `hinting` feature (since 0.2.2), plus the `oxifont::hinted_outline` one-shot wrapper

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
