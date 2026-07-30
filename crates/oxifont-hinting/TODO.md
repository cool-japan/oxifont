# oxifont-hinting TODO

## Status
v0.2.2 — 2026-07-30. Pure Rust TrueType bytecode hinting interpreter (grid-fitting VM). `HintingEngine` loads a font's hinting tables via `FontProgram::load`, runs `fpgm` once at construction, scales the CVT and re-runs `prep` on every `set_ppem`, and grid-fits a single glyph's contour + phantom points via `hint_glyph`. Full TrueType instruction-set dispatch in `dispatch.rs`, control flow and function calls in `interp.rs`, `IUP` interpolation in `ops_iup.rs`, point-movement/measurement opcodes in `ops_move.rs`, stack/arithmetic/rounding opcodes in `ops_arith.rs`, and graphics-state/vector/storage/CVT opcodes in `ops_state.rs`. Bounded execution (instruction budget, call depth, stack size, loop-counter clamping, composite-nesting depth) guards against adversarial bytecode so the VM never panics. 12 source files, ~4300 SLOC. 58 tests passing, 0 failed, 0 skipped (`cargo nextest run -p oxifont-hinting --all-features`); 1 doctest passing. Not yet re-exported from the `oxifont` facade crate or depended on by any other workspace member.

## Core Implementation
- [x] SFNT hinting-table loading: `head`/`maxp`/`glyf`/`loca` required, `fpgm`/`prep`/`cvt `/`hhea`/`hmtx` optional — a glyf font with none of the optional tables is simply unhinted (`font.rs`, `FontProgram::load`)
- [x] Simple and composite glyph outline decoding: point flags/coordinates with repeat-run decoding, nested composite transforms (scale / x-y scale / full 2×2), and a composite-nesting depth guard (`font.rs`, `decode_simple_glyph`, `decode_composite_glyph`, `MAX_COMPOSITE_DEPTH`)
- [x] Font program (`fpgm`) execution once at engine construction, collecting `FDEF`/`IDEF` function and instruction definitions (`interp.rs`, `HintingEngine::new`)
- [x] Control-value program (`prep`) execution on every `set_ppem` call, re-scaling the CVT from font units each time so `prep` always starts from a clean size (`interp.rs`, `HintingEngine::set_ppem`)
- [x] Per-glyph instruction execution over contour points plus the 4 standard phantom points, honoring an `INSTCTRL`-disabled glyph program (`interp.rs`, `HintingEngine::hint_glyph`)
- [x] Full opcode dispatch (`dispatch.rs`) covering the TrueType instruction set: vector setup (`SVTCA`/`SPVTCA`/`SFVTCA`/`SPVTL`/`SFVTL`/`SPVFS`/`SFVFS`/`GPV`/`GFV`/`SFVTPV`/`SDPVTL`), point/line intersection (`ISECT`), reference points and zones (`SRP0-2`/`SZP0-2`/`SZPS`/`SLOOP`), rounding-state selection (`RTG`/`RTHG`/`RTDG`/`RUTG`/`RDTG`/`ROFF`/`SROUND`/`S45ROUND`/`SMD`), CVT and single-width cut-ins (`SCVTCI`/`SSWCI`/`SSW`), stack manipulation (`DUP`/`POP`/`CLEAR`/`SWAP`/`DEPTH`/`CINDEX`/`MINDEX`/`ROLL`), point movement (`MDAP`/`MDRP`/`MIRP`/`MSIRP`/`ALIGNRP`/`ALIGNPTS`/`SHP`/`SHC`/`SHZ`/`SHPIX`/`IP`/`MIAP`/`UTP`), storage/CVT read-write (`WS`/`RS`/`WCVTP`/`WCVTF`/`RCVT`), measurement (`GC`/`SCFS`/`MD`/`MPPEM`/`MPS`/`FLIPON`/`FLIPOFF`), comparison/logic (`LT`/`LTEQ`/`GT`/`GTEQ`/`EQ`/`NEQ`/`ODD`/`EVEN`/`AND`/`OR`/`NOT`), arithmetic (`ADD`/`SUB`/`DIV`/`MUL`/`ABS`/`NEG`/`FLOOR`/`CEILING`/`ROUND`/`NROUND`), delta exceptions (`DELTAP1-3`/`DELTAC1-3`/`SDB`/`SDS`), on-curve flag flips (`FLIPPT`/`FLIPRGON`/`FLIPRGOFF`), scan-conversion/info (`SCANCTRL`/`GETINFO`/`MAX`/`MIN`/`SCANTYPE`/`INSTCTRL`), the full `MDRP`/`MIRP` managed-relative-move opcode ranges, deprecated no-ops that still consume their operand (`SANGW`/`AA`), and a fallback to user-defined (`IDEF`) instruction bodies for otherwise-unrecognized opcodes
- [x] Control flow: `IF`/`ELSE`/`EIF` block skipping, `JMPR`/`JROT`/`JROF` jumps, `FDEF`/`ENDF`/`IDEF` function scanning, `CALL`/`LOOPCALL` dispatch, all via shared instruction-stream navigation helpers that correctly skip variable-length inline `PUSH` operands (`interp.rs`, `opcodes.rs`)
- [x] `IUP` (interpolate untouched points): per-contour, per-axis reference-pair interpolation of points left untouched by explicit move instructions, against original vs. current point positions (`ops_iup.rs`)
- [x] Push instructions with bounds-validated inline operands (`NPUSHB`/`NPUSHW`/`PUSHB[n]`/`PUSHW[n]`) (`ops_arith.rs`)
- [x] F26Dot6 / F2Dot14 fixed-point math: round-half-away-from-zero multiply-divide, the `MUL`/`DIV` opcodes' exact semantics, unit-vector projection / dot product / normalize / perpendicular (`math.rs`)
- [x] Round-state machine covering all classic modes (`RTG`/`RTHG`/`RTDG`/`RUTG`/`RDTG`/`ROFF`) plus `SROUND`/`S45ROUND` super-round `(period, phase, threshold)` triple decoding from the opcode operand (`math.rs`)
- [x] `HintedGlyph::to_outline()` — decomposes fitted points into `oxifont_core::GlyphOutline` quadratic path commands, correctly handling all-off-curve contours and implied on-curve midpoints (`interp.rs`)

## Safety (Adversarial-Input Bounds)
- [x] Bounded per-run executed-instruction budget (loop guard) — `HintingError::ExecutionBudgetExceeded`
- [x] Bounded function-call / instruction-definition recursion depth — `HintingError::CallDepthExceeded`
- [x] Bounded operand stack, derived from `maxp.maxStackElements` but floored and hard-capped independently of it — `HintingError::StackOverflow` / `StackUnderflow`
- [x] Bounded composite-glyph nesting depth during outline decoding — `HintingError::CompositeTooDeep`
- [x] Clamped `SLOOP` loop counter (non-negative, bounded) so a crafted loop count cannot force an unbounded pop loop
- [x] Bounds-checked stack, storage, CVT, point, and jump/program-counter access everywhere, mapped to typed `HintingError` variants instead of panicking
- [x] `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]` crate-wide

## Testing
- [x] Fixed-point math and rounding-mode unit tests (`math.rs`)
- [x] Instruction-stream navigation unit tests (`opcodes.rs`)
- [x] Opcode-level VM unit tests (`vm_tests.rs`)
- [x] Real-font integration suite (`tests/real_fonts.rs`) against bundled Noto Sans Regular/Bold: `fpgm`/`prep` execution at multiple ppem sizes, determinism of repeated fits, grid-snapping verification, unhinted-font pass-through, `to_outline()` shape consistency, `set_ppem(0)` rejection, and a whole-glyph-set no-panic stress run
- [x] 58 tests passing, 0 failed, 0 skipped (`cargo nextest run -p oxifont-hinting --all-features`); 1 doctest passing

## Integration
- [x] Consumes `oxifont_core::sfnt::SfntTableMap` directly for zero-copy table access — the only non-dev dependency is `oxifont-core`
- [x] Dev-dependencies only: `oxifont-bundled` (`bundled-noto` feature) and `oxifont-parser`, for the real-font integration test suite
- [x] Standalone crate for 0.2.1 — not yet consumed by the `oxifont` facade crate or any other workspace member; depend on `oxifont-hinting = "0.2.2"` directly
