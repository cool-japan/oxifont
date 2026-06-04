# oxifont-core TODO

## Status
Shared trait surface and data types for the OxiFont ecosystem. Provides `FontFace`, `FontCatalog`, `FaceInfo`, `FontQuery`, `FontStyle`, `VariationAxis`, and `FontError`. Zero external dependencies, ~178 SLOC. Functionally complete for M1-M2 needs but missing several traits and types required by downstream crates at production grade.

## Core Implementation
- [x] Add `FontStretch` enum (ultra-condensed..ultra-expanded, CSS values 1-9) and add `stretch` field to `FaceInfo` (~30 SLOC)
- [x] Add `FontCapabilities` trait for querying supported OpenType features/scripts/languages (~40 SLOC)
- [x] Add `GlyphOutline` enum (MoveTo/LineTo/QuadTo/CubicTo) and `outline()` method to `FontFace` trait for path extraction (~60 SLOC)
- [x] Add `KerningPair` struct and `kern(gid_left, gid_right)` method to `FontFace` trait (~25 SLOC)
- [x] Add `FontMetrics` struct (ascender, descender, line_gap, cap_height, x_height, units_per_em, underline_position, underline_thickness, strikeout_position, strikeout_thickness) (~50 SLOC)
- [x] Add `metrics()` method returning `FontMetrics` to `FontFace` trait (~5 SLOC)
- [x] Add `ColorGlyph` support: `has_color_glyphs()`, `color_glyph_type() -> ColorGlyphFormat` (COLRv0/v1, CBDT, sbix, SVG) to `FontFace` (~30 SLOC)
- [x] Add `NameTable` trait for querying localized name records (copyright, designer, license, description, sample text) (~45 SLOC)
- [x] Implement `no_std` + `alloc` compatibility: remove `std::path::PathBuf` dependency behind a `std` feature gate, use `alloc::string::String` (~40 SLOC)
- [x] Add `FontCollection` trait for TTC file iteration (iterate faces, get face by index) (~20 SLOC)
- [x] Add `EmbeddingLicense` enum (Installable/Editable/PreviewPrint/RestrictedLicense) derived from OS/2 fsType (~20 SLOC)
- [x] Add `SfntTableMap<'a>` zero-copy SFNT directory parser (planned 2026-05-26)
  - **Goal:** New `pub mod sfnt` in oxifont-core exposing `SfntTableMap<'a>` that walks the 12-byte SFNT header + numTables × 16-byte directory entries, returning zero-copy `&'a [u8]` slices for each table. Used by both oxifont-parser and oxifont-subset to eliminate independent directory re-parses.
  - **Design:** `pub struct SfntTableMap<'a> { pub sfnt_version: u32, tables: BTreeMap<[u8;4], &'a [u8]>, raw: &'a [u8] }`. `parse(data: &'a [u8]) -> Result<Self, SfntError>`. Error type: `pub enum SfntError { Truncated, BadMagic(u32), DuplicateTag([u8;4]), OutOfBounds([u8;4]) }`. `no_std`-compatible (uses `alloc::collections::BTreeMap`).
  - **Files:** `crates/oxifont-core/src/sfnt.rs` (new, ~150 lines), `crates/oxifont-core/src/lib.rs` (`pub mod sfnt;`).
  - **Tests:** `crates/oxifont-core/tests/sfnt_table_map.rs` — parse test.ttf fixture (get `glyf` table, assert sorted tag iteration); corrupt magic → `BadMagic`; truncated → `Truncated`.
  - **Risk:** BTreeMap requires `alloc` — already present in oxifont-core.

## API Improvements
- [x] Implement `PartialOrd`/`Ord` for `FontStyle` to enable CSS-style style preference ordering
- [x] Add `Hash` derive to `FontStyle`, `FaceInfo`, `FontQuery` for use as HashMap keys
- [x] Add `Serialize`/`Deserialize` behind a `serde` feature gate for `FaceInfo`, `FontQuery`, `FontStyle`, `VariationAxis`
- [x] Add `FontQuery::stretch()` builder method once `FontStretch` is added
- [x] Add `FontQuery::postscript_name()` for matching by PostScript name
- [x] Add `FontCatalog::find_all()` returning an iterator over all matching faces
- [x] Make `FontError` implement `Clone` (requires wrapping `io::Error` in an `Arc`)

## Testing
- [x] Add property-based tests for `FontQuery` builder (all combinations of family/style/weight)
- [x] Add tests for `FontError` Display formatting
- [x] Add round-trip serialization tests (behind `serde` feature)
- [x] Test `FontStyle` ordering for CSS matching semantics
- [x] Add doc-tests for all public methods

## Performance
- [x] Evaluate switching `FaceInfo::family` from `String` to `Arc<str>` for cheaper cloning in large catalogs
- [x] Benchmark `FontQuery` matching in catalogs of 1000+ faces

## Integration
- [x] Ensure `FontFace::outline()` output is compatible with oxitext-raster path-based rasterization
- [x] Align `VariationAxis` with oxifont-db's `VariableAxis` (currently duplicated types)
- [x] Add `From<oxifont_db::FaceInfo>` impl for `oxifont_core::FaceInfo` to bridge the two face types
