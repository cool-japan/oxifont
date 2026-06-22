# oxifont-subset TODO

## Status
Pure Rust OpenType font subsetter. Takes SFNT bytes + codepoints/glyph IDs, produces minimal SFNT. Handles TrueType (glyf/loca) and CFF/CFF2 outline formats. Rewrites: cmap (format 4/12), hmtx/vmtx, maxp, head, hhea/vhea, post v3, name, OS/2, kern. Layout: GSUB/GPOS/GDEF subtable rewriting with coverage and classdef remapping. Color: COLR/CPAL, CBDT/CBLC, SVG, sbix, MATH. Variable: gvar per-glyph tuple subsetting, HVAR/VVAR, fvar/avar. High-level entry points: `subset_font`, `subset_font_with_options`, `subset_by_gids`, `subset_font_for_web`, `subset_font_for_pdf`, `PdfFontSubsetter` builder. Optional `parallel` feature (rayon). ~4200 SLOC, 42 public items, 0 stubs. M5–M6 subsetting complete.

## Core Implementation
- [x] Implement CFF (Type 1) outline subsetting: parse CFF CharStrings, rebuild CFF header/INDEX/Top DICT/Private DICT for subset glyph set (~300 SLOC)
- [x] Implement CFF2 outline subsetting with ItemVariationStore support (~200 SLOC)
- [x] Rewrite GSUB table: prune lookups/features referencing removed GIDs, compact coverage tables (~250 SLOC)
  - **Goal:** Rewrite GSUB tables: remap GIDs in all lookup subtables, drop unhandled lookups, rebuild SFL chain. (planned 2026-05-25)
  - **Design:** Common SFL rewriter in `src/layout.rs`. GSUB types 1-4,7; types 5,6,8 → safe-drop. Entry point: `rewrite_gsub(table, gid_remap) -> Vec<u8>`.
  - **Files:** `crates/oxifont-subset/src/layout.rs`, `src/lib.rs`.
  - **Tests:** `crates/oxifont-subset/tests/layout_gsub.rs`
- [x] Rewrite GPOS table: prune PairPos/MarkBase/MarkLig lookups for removed GIDs (~200 SLOC)
  - **Goal:** Rewrite GPOS tables using the common SFL rewriter, adding GPOS-specific subtable handlers. (planned 2026-05-25)
  - **Design:** GPOS types 1,2,4,6,9; types 3,5,7,8 → safe-drop. Entry point: `rewrite_gpos(table, gid_remap) -> Vec<u8>`.
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

## Testing
- [x] Test with NotoSans-Regular.ttf: subset ASCII Latin and verify glyph rendering matches original
- [x] Test with NotoSansCJK TTC: subset CJK codepoints from a TTC face
- [x] Test composite glyph closure: subset 'fi' ligature and verify components are included
- [x] Test format-12 cmap rewriting with supplementary plane codepoints (emoji)
- [x] Test variable font subsetting: fvar/gvar copied, HVAR rewritten
- [x] Test empty subset (only .notdef) produces valid SFNT
- [x] Test round-trip: subset then parse with ttf-parser, verify all retained glyphs accessible
- [x] Add CFF font test fixture and test CFF subsetting
- [x] Test name table filtering retains only IDs 0-6
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
