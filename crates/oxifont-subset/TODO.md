# oxifont-subset TODO

## Status
Pure Rust OpenType font subsetter. Takes SFNT bytes + codepoints/glyph IDs, produces minimal SFNT. Handles TrueType (glyf/loca) and CFF/CFF2 outline formats. Rewrites: cmap (format 4/12; the format-4 builder is segment-count-checked and returns a typed `SubsetError::InvalidFont` instead of overflowing on subsets beyond the ~8189-segment addressable size), hmtx/vmtx, maxp, head, hhea/vhea, post v3, name, OS/2, kern. Layout: GSUB/GPOS/GDEF subtable rewriting with coverage and classdef remapping — every GSUB lookup type (1–8) and GPOS lookup type (1–9) is remapped, including contextual/chaining lookups in all three formats and their Extension-wrapped forms; a subtable is dropped only when malformed or unable to match under the subset (counted in `SubsetStats::dropped_context_subtables`). Color: COLR/CPAL, CBDT/CBLC, SVG, sbix, MATH. Variable: gvar per-glyph tuple subsetting, HVAR/VVAR, fvar/avar. Output flavour follows the outline table present: `OTTO` for CFF/CFF2, `0x00010000` for glyf, with spec-conformant offset-table search fields. High-level entry points: `subset_font`, `subset_font_with_options`, `subset_by_gids`, `subset_font_for_web`, `subset_font_for_pdf`, `PdfFontSubsetter` builder; `_at_face` siblings select a face out of a `ttcf` collection (`face_count`, `SubsetError::FaceIndexOutOfRange`) and `_mapped` siblings return the `SubsetGidMap` old ↔ new glyph-ID mapping (the CID assignment a PDF CIDFont needs). Static instancing: `instance()` pins a `glyf` variable face at one design location and emits a static SFNT with unchanged glyph IDs (full `gvar` tuple walk, IUP, phantom-point metrics, composites kept as composites) — see the Instancing section below. Optional `parallel` feature (rayon). 26 source files, ~9900 SLOC across `src/`, 0 stubs. M5–M6 subsetting complete. `cargo nextest run -p oxifont-subset --all-features`: 251 passed, 0 failed, plus 21 `#[ignore]`d live tests over `%SystemRoot%\Fonts` (`--run-ignored ignored-only`: 21 passed).

## Core Implementation
- [x] Implement CFF (Type 1) outline subsetting: parse CFF CharStrings, rebuild CFF header/INDEX/Top DICT/Private DICT for subset glyph set (~300 SLOC)
- [x] Implement CFF2 outline subsetting with ItemVariationStore support (~200 SLOC)
- [x] Rewrite GSUB table: prune lookups/features referencing removed GIDs, compact coverage tables (~250 SLOC)
  - **Goal:** Rewrite GSUB tables: remap GIDs in all lookup subtables, drop unhandled lookups, rebuild SFL chain. (planned 2026-05-25)
  - **Design:** Common SFL rewriter in `src/layout.rs`. GSUB types 1–8 are all remapped: 1–4 directly, 5/6 (contextual, all three formats) via `src/otl_context.rs`, 7 (Extension) recursing into its inner type, 8 (ReverseChainSingleSubst) directly. Entry point: `rewrite_gsub(table, gid_remap) -> Vec<u8>`.
  - **Files:** `crates/oxifont-subset/src/layout.rs`, `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/layout_gsub.rs`
- [x] Rewrite GPOS table: prune PairPos/MarkBase/MarkLig lookups for removed GIDs (~200 SLOC)
  - **Goal:** Rewrite GPOS tables using the common SFL rewriter, adding GPOS-specific subtable handlers. (planned 2026-05-25)
  - **Design:** GPOS types 1–9 are all remapped: 1/2/4/6 directly, 3 (CursivePos) and 5 (MarkLigPos) with anchor copying that preserves NULL anchors, 7/8 (contextual, all three formats) via `src/otl_context.rs`, 9 (Extension) recursing into its inner type. Entry point: `rewrite_gpos(table, gid_remap) -> Vec<u8>`.
  - **Files:** `crates/oxifont-subset/src/layout.rs` or `src/otl.rs`, `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/layout_gpos.rs`
- [x] Rewrite GDEF table: prune GlyphClassDef, AttachList, LigCaretList, MarkAttachClassDef for removed GIDs (~100 SLOC)
  - **Goal:** Shared Coverage/ClassDef primitives in `layout.rs` + `rewrite_gdef(table, gid_remap) -> Vec<u8>`. (planned 2026-05-25)
  - **Design:** read/write/remap_coverage, read/write/remap_classdef helpers. GDEF: remap GlyphClassDef, MarkAttachClassDef, AttachList, LigCaretList, MarkGlyphSetsDef.
  - **Files:** `crates/oxifont-subset/src/layout.rs` (new), `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/layout_gdef.rs`
- [x] Subset OS/2 table: update ulUnicodeRange, usFirstCharIndex, usLastCharIndex (~40 SLOC)
  - **Goal:** `rewrite_os2(table, codepoints) -> Vec<u8>` — recompute ulUnicodeRange1-4 (bytes 42–57) and usFirstCharIndex/usLastCharIndex (bytes 64–67). (planned 2026-05-25)
  - **Design:** New `src/os2.rs`. ~128-entry lookup table mapping Unicode blocks to OS/2 bit positions. Guard: table length < 68 → verbatim. Wire into pipeline.
  - **Files:** `crates/oxifont-subset/src/os2.rs` (new), `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/os2.rs`
- [x] Subset gvar table for variable fonts: rewrite per-glyph variation tuples for the new GID space (~150 SLOC)
  - **Goal:** `rewrite_gvar(table, rev_remap, new_glyph_count) -> Vec<u8>` — reorder per-glyph data blocks to new GID space. (planned 2026-05-25)
  - **Design:** New `src/gvar.rs`. Keep header+shared tuples verbatim, reorder opaque per-glyph data blocks by new GID, rebuild offset array (short or long per flags bit 0).
  - **Files:** `crates/oxifont-subset/src/gvar.rs` (new), `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/gvar.rs`
- [x] Handle TrueType instructions: optionally strip fpgm/prep/cvt tables for smaller output (~20 SLOC)
- [x] Add COLR/CPAL subsetting: prune color layers referencing removed base GIDs (~80 SLOC)
- [x] Add SVG table subsetting: remove SVG documents for removed GIDs (~40 SLOC)
- [x] Add sbix table subsetting: remove bitmap strikes for removed GIDs (~40 SLOC)
- [x] Add CBDT/CBLC table subsetting: prune bitmap data for removed GIDs (~80 SLOC)
- [x] Fix HVAR/VVAR rewriting: correct the offset field mapping (currently uses bytes 4-7 instead of 8-11 for advanceWidthMappingOffset as noted in FIXME) (~15 SLOC) (planned 2026-05-25)
  - **Goal:** advanceWidthMappingOffset is correctly read from bytes 8-11 per OpenType spec; IVS offset (bytes 4-7) left untouched; stale FIXME removed.
  - **Design:** In `src/varfont.rs` around line 167. HVAR header: majorVersion(u16, 0-1) minorVersion(u16, 2-3) itemVariationStoreOffset(Offset32, 4-7) advanceWidthMappingOffset(Offset32, 8-11) lsbMappingOffset(Offset32, 12-15) rsbMappingOffset(Offset32, 16-19). Change read of advanceWidthMappingOffset from `data[4..8]` to `data[8..12]`; update write-back to the same range.
  - **Files:** `crates/oxifont-subset/src/varfont.rs`.
  - **Prerequisites:** none.
  - **Tests:** `crates/oxifont-subset/tests/` — synthesize a minimal HVAR table via byte builder with distinct non-zero IVS/advanceWidthMapping/lsb offsets; run rewrite; assert IVS preserved and advanceWidthMapping read from 8-11.
  - **Risk:** Synthetic test could mis-encode the layout it checks. Mitigation: assert each field byte-offset independently; cross-check spec offset table in comment.
- [x] Add kern table subsetting: prune kerning pairs referencing removed GIDs (~40 SLOC)
  - **Goal:** `rewrite_kern(table, gid_remap) -> Vec<u8>` — prune pairs with removed GIDs, remap survivors, recompute binary-search header. (planned 2026-05-25)
  - **Design:** New `src/kern.rs`. Format-0 subtables only; non-format-0 → drop. Sort pairs, recompute searchRange/entrySelector/rangeShift.
  - **Files:** `crates/oxifont-subset/src/kern.rs` (new), `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/kern.rs`
- [x] Add MATH table subsetting for mathematical typesetting fonts (~60 SLOC)

## API Improvements
- [x] Add `SubsetOptions` builder: `strip_hints(bool)`, `retain_names(bool)`, `retain_layout_tables(bool)`, `desubroutinize_cff(bool)` (~40 SLOC)
  - **Goal:** Configurable subsetting pipeline: `SubsetOptions` struct with builder, `SubsetStats` return, `subset_by_gids`, presets for web/PDF, `strip_hints` flag, `retain_codepoint_range`. (planned 2026-05-25)
  - **Design:** New `SubsetOptions` with `strip_hints: bool`, `retain_layout_tables: bool`, `retain_names: bool`, `retain_codepoint_range: Option<(char, char)>`. Refactor `subset_font` into `subset_with_gid_set(data, old_gid_set, opts) -> Result<(Vec<u8>, SubsetStats), SubsetError>`. `subset_font` becomes thin wrapper. New `subset_by_gids`, `subset_font_for_web`, `subset_font_for_pdf` presets. `SubsetStats { original_size, subset_size, glyphs_retained, tables_retained }`.
  - **Files:** `crates/oxifont-subset/src/lib.rs`, possibly `src/options.rs`.
  - **Tests:** `crates/oxifont-subset/tests/options.rs`
- [x] Add `subset_by_gids(font_data, gids: &BTreeSet<u16>)` for GID-based subsetting without cmap lookup (~30 SLOC)
- [x] Add `subset_font_for_pdf(font_data, codepoints)` that produces PDF-optimized output (strip hints, minimal name table, post v3) (~20 SLOC)
- [x] Add `subset_font_for_web(font_data, codepoints)` that produces web-optimized output (strip hints, compact tables) (~20 SLOC)
- [x] Return subset statistics: original size, subset size, tables retained, glyphs retained
- [x] Add `retain_codepoint_range(start_char..end_char)` for range-based subsetting

## Instancing
- [x] `instance(font_data, face_index, coords) -> Result<Vec<u8>, SubsetError>` — static instancing of a `glyf` variable face
  - **Goal:** A fully pinned user-space location in, a complete static SFNT out, with glyph IDs unchanged so the result feeds straight into the ordinary subsetting entry points. Instance-first rather than instance-during-subset: the gvar walk, the composite closure, the gid remap and the phantom points never have to interact, and neither variable-table rewriter (`gvar::rewrite_gvar`, `varfont::rewrite_hvar_vvar`) ever runs.
  - **Design:** 16.16 fixed-point coordinate pipeline (`fvar` clamp → default normalisation → `avar` segment maps) with FreeType's *rounded* `FT_DivFix`/`FT_MulDiv`, converting to F2Dot14 exactly once at the end; `gvar` tuple variation store (both offset formats, shared and embedded peak tuples, intermediate regions, shared and private packed point numbers, packed deltas); region scalars in `f64` with the `instanceCoord == 0` test before the malformed-region guards; scale → infer → sum ordering with IUP against the **default** outline; four phantom points per glyph feeding `hmtx`/`vmtx`; composites kept as composites with only their component offsets moved and `ARG_1_AND_2_ARE_WORDS` recomputed; one rounding rule (`otRound`, round half toward +∞) applied exactly once per emitted value.
  - **Files:** `src/instance/{mod,coords,tuples,outline,metrics,ivs}.rs`; `src/lib.rs` gains `pub mod instance;` + `pub use instance::instance;`. Reused after a visibility promotion: `glyf`'s composite flag constants and `loca_entry`, the new `glyf::build_loca`, `gvar::{GvarHeader, parse_header, parse_offsets}`, `varfont::{EntryFormat, read_entry, read_delta_set_map}`, and the `get/set_u16/i16` helpers moved from `lib.rs` to `tables.rs`.
  - **Tests:** `src/instance/coords.rs` (normalisation vectors: axis endpoints, clamping, an absent axis, a duplicate tag, an unknown tag, empty / one-entry / multi-entry `avar` maps, truncation), `src/instance/{tuples,outline,metrics,ivs}.rs` unit tests (packed encodings, region scalars, IUP, `otRound`, composite argument re-encoding, `usWidthClass`), `tests/instance.rs` (21 synthetic-fixture tests), `tests/instance_live.rs` (21 `#[ignore]`d differential tests over `%SystemRoot%\Fonts`).
  - **Risk:** the coordinate pipeline and the rounding rule are the flaky surface — both are pinned by exact-value tests rather than tolerances.
- [x] Coordinate pipeline: `fvar` parse + clamp + normalise, `avar` v1 segment maps, all in 16.16, one F2Dot14 conversion at the end
- [x] `gvar` tuple variation store: both offset formats, `EMBEDDED_PEAK_TUPLE`, `INTERMEDIATE_REGION`, `PRIVATE_POINT_NUMBERS`, shared point numbers, `POINTS_ARE_WORDS`, packed deltas with the `0x80` zero-run winning over `0x40`
- [x] IUP (Interpolate Untouched Points) against the default outline, cyclic pairing within each contour, four single-point phantom contours
- [x] Phantom points → `hmtx`/`vmtx`/`hhea`/`vhea`; empty glyphs covered by iterating `0..numGlyphs` rather than the `glyf` records
- [x] Composites: component offsets varied, `ARG_1_AND_2_ARE_WORDS` recomputed from the rounded values, point-matched components' deltas discarded, `SCALED_COMPONENT_OFFSET` honoured, `ROUND_XY_TO_GRID` preserved but not acted on, bounding boxes by decomposition with a depth cap and cycle detection
- [x] Output policy: variation and hint tables dropped, instructions stripped, `maxp` counters recomputed, `OS/2`/`head`/`post` style fields from the pinned location, everything else verbatim
- [x] `ItemVariationStore` evaluation for the `fvar`-without-`gvar` carve-out (`src/instance/ivs.rs`) — the only place `HVAR`/`VVAR` are read rather than deleted unread
- [x] `SubsetOptions::drop_variations` + `#[non_exhaustive]`; `DSIG` off the verbatim list; `.notdef` inserted in `subset_from_tables`; `SubsetStats::cff_charstrings_verbatim`; `SubsetError::{UnknownAxis, Unsupported}`
- [ ] Scoped instancing: an `instance()` overload taking a composite-closed gid set, emitting empty `glyf` entries for the rest
  - **Goal:** avoid instancing all 17 936 glyphs of a CJK variable face when a page uses ~600. Measured cost of the whole-face pass (release, full pipeline stand-in): 8.2 ms / 108 KB for `bahnschrift`, 24.7 ms for `SegUIVar`, **325 ms and ~5.5 MB transient for `NotoSansJP-VF`**. Acceptable for a desktop print export; the likely complainant is `wasm32`.
  - **Risk:** the closure must be computed *before* instancing, or a composite's base glyph is left at the default master — which re-introduces exactly the interaction instance-first exists to remove. Do not build it speculatively; the trigger is a *measured* export-latency problem.
- [ ] `avar` version 2: the additional `ItemVariationStore` is ignored, only the version-1 segment maps are applied. No shipping font measured for this work carries one; a face that did would normalise slightly differently from skrifa. Detect-and-report is currently impossible — see the DEVIATION note below.
- [ ] `GDEF` `ItemVariationStore` (v1.3) referenced by `GPOS` `VariationIndex` value records: after full pinning those deltas can no longer be resolved by a renderer with no `fvar`. Correct handling folds them into the `GPOS` value records and drops `GDEF.itemVarStore`. Out of scope for the first pass; `GPOS` in an embedded PDF font is never executed.
- [ ] `cvar`-aware hint retention (`keep_hints`): apply `cvar` deltas to `cvt ` and keep `cvt `/`fpgm`/`prep` and the per-glyph instruction streams, for callers that want a hintable static font. Today all four leave together, which is the only self-consistent behaviour without `cvar` support.
- [ ] **Upstream issue, not fixed:** `varfont::rewrite_hvar_vvar` passes a table through verbatim whenever its `AdvWidthMap` is absent or unparseable — measured on `NotoSansJP-VF`, where a 20-glyph subset carries the source's 35 922-byte `HVAR` unchanged, indexed by the *original* glyph IDs. Also `varfont.rs`'s `LsbMap`/`RsbMap` offset fixups are explicitly abandoned (`let _ = (old_map_end, shift);`), and `gvar.rs`'s short-offset writer stores `cursor / 2` without padding blocks to an even length, so an odd-length block truncates its last byte. None of the three is on the instancing path — `instance()` deletes all of these tables — and `SubsetOptions::drop_variations` avoids them too; they matter only to a caller who deliberately wants a *variable* subset.

### Deviations from the design document
- DEVIATION 2026-08-04 F1 (composite IUP): the design said a composite's component offsets form **one** contour for IUP purposes. Measured against a live variable-font renderer, that interpolates an unreferenced component's offset from a neighbouring component and moves 218 of `bahnschrift`'s 959 outlined glyphs by up to 696 font units at `wdth 75` (e.g. gid 3's base component displaced by −210 units, which no renderer agrees with). Each component is therefore its own single-point contour, so an unreferenced component takes a zero delta. With that change the divergence is 0 glyphs at every axis endpoint tested and ≤ 1.23 units (double rounding) at interior locations.
- DEVIATION 2026-08-04 F1 (diagnostics): the design called for `tracing::warn`/`debug` counters on per-glyph variation-data drops, `int16` clamps, a non-zero `pp1.x`, an ignored `avar` v2 store, and `HVAR`-vs-phantom disagreement. `oxifont-subset` has no logging dependency and the task forbade adding one, so no counters are collected — an unused counter struct would only trip `dead_code`. Each condition still has the prescribed *behaviour* (per-glyph fallback to the default outline, clamping rather than wrapping, v1 segment maps applied) and each is covered by a test; only the observability is missing. Adding `log`/`tracing` behind an off-by-default feature is the natural follow-up.
- DEVIATION 2026-08-04 F5 (differential tolerance): the design's oracle prescribed outline and bbox agreement within ±1 design unit. That holds at every axis endpoint (measured maximum divergence 0.5). At *interior* axis locations composites double-round — the base glyph is rounded to `int16` before the component transform, then the component offset is rounded again — and the measured worst case across all six installed variable faces is 1.23 units, so `live_interior_locations_stay_within_the_rounding_residue` allows 2.0 for composites (0.6 for simple glyphs, whose measured worst case is 0.518). The endpoint tests keep the ±1 bound.

## Testing
- [x] Test with NotoSans-Regular.ttf: subset ASCII Latin and verify glyph rendering matches original
- [x] Test with NotoSansCJK TTC: subset CJK codepoints from a TTC face
- [x] `tests/ttc.rs` — hand-built two-face `ttcf` (face 1 distinguished by `head.unitsPerEm`): face selection, face-0-equals-standalone, out-of-range refusal, hostile headers (zero / huge `numFonts`, offset past EOF, offset at non-SFNT bytes, truncated header, unknown version), legacy entry points still refusing a container, plus a real `%SystemRoot%\Fonts\*.ttc` round-trip that skips when absent
- [x] `tests/gid_map.rs` — `SubsetGidMap` is the dense rank order of the composite closure, round-trips old→new→old, lists a composite's components when only the composite was requested, and the `_mapped` entry points are byte-identical to their siblings
- [x] `tests/sfnt_header.rs` — `OTTO` vs `0x00010000` flavour selection (unit and end-to-end) and the offset-table search fields at `numTables` 0, 1–9 (including the exact power of two)
- [x] Test composite glyph closure: subset 'fi' ligature and verify components are included
- [x] Test format-12 cmap rewriting with supplementary plane codepoints (emoji)
- [x] Test variable font subsetting: fvar/gvar copied, HVAR rewritten
- [x] Test empty subset (only .notdef) produces valid SFNT
- [x] Test round-trip: subset then parse with ttf-parser, verify all retained glyphs accessible
- [x] Add CFF font test fixture and test CFF subsetting
- [x] Test name table filtering retains only IDs 0-6
- [x] `tests/instance.rs` — hand-built fixture faces covering the encodings shipping fonts never use (`EMBEDDED_PEAK_TUPLE`, `POINTS_ARE_WORDS`, `SCALED_COMPONENT_OFFSET`, point-matched components, short-offset `gvar`, short `loca` output, `int16` coordinate clamping, `fvar` without `gvar`, `cvar` present), plus default-location identity, empty-glyph advance deltas, `OS/2`/`head`/`post` style updates, a broken variation block degrading to the default outline, a composite cycle, byte-determinism, and truncation of `fvar`/`avar`/`gvar` at every byte offset
- [x] `tests/instance_live.rs` — `#[ignore]`d differential tests over `%SystemRoot%\Fonts`: pinned advance oracles for NotoSansJP-VF, SegUIVar, SitkaVF and bahnschrift (exact, tolerance 0), outline agreement with `ttf_parser`'s live variable-font evaluation, default-location identity over every glyph of four faces, no variation tables in the output, no surviving instruction streams, and byte-determinism
- [x] `tests/upstream_fixes.rs` — `.notdef` retained by every entry point with matching numbering, no `DSIG` in any output, `drop_variations` on a synthetic and a real variable face, and `drop_variations` proven a no-op on an already-instanced face
- [x] Fuzz `subset_font` with arbitrary bytes and codepoint sets — `fuzz/` infrastructure added (2026-06-03): fuzz_subset.rs (codepoint-bitmask derived from input), fuzz_subset_by_gids.rs (GID bitmask). Both verify no-panic + SFNT magic on success.

## Performance
- [x] Avoid copying verbatim tables: use `Cow<[u8]>` (~30 SLOC)
  - `output_tables` now holds `Vec<([u8;4], Cow<'_,[u8]>)>`. Verbatim tags use
    `Cow::Borrowed(slice)` (zero heap allocation); rewritten tables use `Cow::Owned(vec)`.
    `build_sfnt` updated to accept `&[([u8;4], Cow<[u8]>)]`. Public API unchanged.
- [x] Pre-allocate output buffer based on estimated subset size
  - `build_sfnt` now computes exact body_size (padded) and passes the precise capacity so the
    output `Vec<u8>` is allocated once. `output_tables` pre-sized to 25 slots.
- [x] Benchmark `subset_font()` for 100- and 1000-codepoint subsets (planned 2026-05-26)
  - **Design:** `benches/subset_font.rs` — criterion bench `subset_font(test.ttf, codepoints_100)` and `subset_font(test.ttf, codepoints_1000)`. Requires `criterion.workspace = true` in `[dev-dependencies]` + `[[bench]] name = "subset_font" harness = false`. Workspace criterion dep added by Slice 5.
- [x] Parallelize independent table rewrites (glyf/cmap/hmtx can be done concurrently)

## Integration
- [x] Provide subset API for oxifont-webfont WOFF2 encoding pipeline (subset then compress)
- [x] Integrate with oxitext for on-the-fly font subsetting in PDF text rendering
  - **Implementation:** `oxitext` workspace adds `oxifont-subset` as an optional dep behind the
    `font-subset` feature. New `crates/oxitext/src/pdf_subset.rs` exposes `TextFontSubsetter` —
    a thin ergonomic wrapper around `PdfFontSubsetter` with text-oriented API (`feed_text`,
    `feed_char`, `feed_gid`, `merge`, `finalize`). Re-exports `SubsetOptions`, `SubsetStats`,
    `SubsetError`, and `PdfSubsetResult` so callers need not add a direct dep on oxifont-subset.
    Feature matrix table and "What each feature pulls in" section in oxitext `lib.rs` updated.
- [x] Coordinate with oxifont-parser for shared table access via `SfntTableMap` (planned 2026-05-26)
  - **Design:** `oxifont-subset/src/tables.rs::read_table_directory` delegates to `SfntTableMap::parse(data)?` from `oxifont-core::sfnt`. Returns same `HashMap<[u8;4], &[u8]>` as before. New public API: `subset_with_table_map(map: &SfntTableMap, gid_set: &BTreeSet<u16>, opts: &SubsetOptions) -> Result<(Vec<u8>, SubsetStats), SubsetError>` — saves one directory walk for callers (facade) that pre-parse.
  - **Files:** `src/tables.rs` (delegate to SfntTableMap), `src/lib.rs` (add `subset_with_table_map`).
  - **Tests:** `tests/shared_table_map.rs` — pre-parse via `SfntTableMap::parse`, call `subset_with_table_map`, byte-compare output to `subset_font(data, codepoints)`.
