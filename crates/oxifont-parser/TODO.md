# oxifont-parser TODO

## Status
Wraps `ttf_parser` with owned byte storage (`Arc<[u8]>`) so `ParsedFace` outlives the original byte slice. Implements `FontFace` trait. Extracts family name, style, weight, monospace flag, units_per_em, glyph mapping, advance widths, and variable axes. ~228 SLOC. Solid M1 implementation but missing advanced OpenType table access and outline extraction.

## Core Implementation
- [x] Expose `FontMetrics` from OS/2 and hhea tables (ascender, descender, line_gap, cap_height, x_height, underline_position/thickness, strikeout_position/thickness) (~60 SLOC)
- [x] Implement `outline(gid) -> Vec<GlyphOutline>` for extracting glyph outlines from glyf/CFF tables (~80 SLOC)
- [x] Add kerning support: `kern(gid_left, gid_right) -> i16` from kern table and GPOS PairPos (~70 SLOC)
- [x] Add `vertical_advance(gid) -> Option<u16>` from vmtx table for vertical text layout (~20 SLOC)
- [x] Add `vertical_origin(gid) -> Option<(i16, i16)>` from VORG table (~25 SLOC)
- [x] Expose `PostScriptName` extraction (name ID 6) (~10 SLOC)
- [x] Add `has_table(tag: [u8;4]) -> bool` for checking presence of OpenType tables (~10 SLOC)
- [x] Add `table_data(tag: [u8;4]) -> Option<&[u8]>` for raw table access (~15 SLOC)
- [x] Support CFF-outlined fonts: detect CFF/CFF2 and provide outline extraction (~100 SLOC)
- [x] Add `color_glyph_format() -> Option<ColorGlyphFormat>` detecting COLR/CPAL, CBDT/CBLC, sbix, SVG tables (~30 SLOC)
- [x] Add `glyph_count() -> u16` from maxp table (~5 SLOC)
- [x] Parse OpenType features list from GSUB/GPOS FeatureList for `supported_features() -> Vec<[u8;4]>` (~60 SLOC)
- [x] Parse supported scripts/languages from GSUB/GPOS ScriptList (~50 SLOC)
- [x] Add `variation_coordinates(settings: &[([u8;4], f32)]) -> ParsedFace` for creating variation instances (~40 SLOC)

## API Improvements
- [x] Add `ParsedFace::from_bytes(bytes: Vec<u8>, face_index: u32)` convenience constructor (takes ownership of Vec)
- [x] Implement `Clone` for `ParsedFace` (cheap: `Arc<[u8]>` is already cloneable)
- [x] Add `Send + Sync` guarantees documentation (already thread-safe due to Arc storage)
- [x] Add `ParsedFace::as_face_info() -> FaceInfo` for creating a FaceInfo from a parsed face
- [x] Add builder pattern for face options: `ParsedFace::builder(data).face_index(0).variation("wght", 700.0).build()`

## Testing
- [x] Add tests with real-world TTF fixture (NotoSans-Regular.ttf or similar)
- [x] Test TTC collection parsing with multi-face fixtures
- [x] Test variable font axis extraction with a variable font fixture
- [x] Test CFF-outlined OTF parsing
- [x] Test error paths: truncated data, invalid magic bytes, out-of-range face index
- [x] Benchmark `ParsedFace::parse()` overhead and caching strategies (2026-05-27)
- [x] Test PostScript name extraction
- [ ] Add fuzzing target for `ParsedFace::parse` with arbitrary byte input

## Performance
- [x] Cache the `ttf_parser::Face` inside `ParsedFace` instead of re-parsing on every `with_face()` call (measure memory vs. speed tradeoff) (~40 SLOC)
- [x] Lazy-parse fields: only extract family/style/weight on first access, not at construction time
- [x] Add `ParsedFace::preload()` to force-cache all metrics for hot-path usage

## Integration
- [x] Provide raw table access for oxifont-subset (currently subset re-parses from bytes)
- [x] Add `with_table_map` method to `ParsedFace` for shared SFNT directory access (planned 2026-05-26)
  - **Design:** `ParsedFace::with_table_map<R, F: FnOnce(&SfntTableMap) -> R>(&self, f: F) -> Result<R, SfntError>` — re-parses SFNT header on demand via `oxifont_core::sfnt::SfntTableMap::parse(&self.raw_data)`. Cheap to call; allows downstream consumers to get zero-copy table slices without a full re-parse.
  - **Files:** `src/lib.rs`.
  - **Tests:** Extend an existing test in `tests/parse.rs` to call `with_table_map` and assert `SfntTableMap` exposes `head`, `cmap`, `glyf` tables.
- [ ] Provide outline data for oxitext-raster to enable direct path rasterization without fontdue
- [ ] Provide GSUB/GPOS feature data for oxitext-shape complex script shaping
