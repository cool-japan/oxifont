# OxiFont Project TODO

## Status
Pure Rust font discovery, parsing, subsetting, and webfont processing. **v0.1.2 released 2026-06-10.**
10 crates in workspace, ~28 000 Rust SLOC, 908 tests passing (0 failures; excludes slow native CoreText/DirectWrite tests). M0–M7 milestones complete.
Full pipeline: TTF/OTF/TTC parsing, filesystem and native (CoreText/DirectWrite) font enumeration,
CSS Fonts Level 4 matching, TrueType+CFF glyph subsetting, WOFF1/WOFF2 encode+decode,
bundled Noto fonts, SfntTableMap shared table directory, COLR/CBDT/SVG/sbix/MATH subsetting.

## Milestone Summary

### M0 (Complete)
- [x] Workspace skeleton, Cargo.toml, deny.toml, ffi-audit, .gitignore

### M1 (Complete)
- [x] oxifont-core: trait surface (FontFace, FontCatalog), FaceInfo, FontQuery, FontStyle, VariationAxis, FontError
- [x] oxifont-parser: TTF/OTF/TTC parsing via ttf-parser, FontFace impl, owned Arc<[u8]> storage
- [x] oxifont-discovery: system font dir scanning (macOS/Linux/Windows), walkdir-based recursion
- [x] oxifont-adapter-pure: FontCatalog from filesystem scanning
- [x] oxifont facade: re-export layer with feature gates

### M2 (Complete)
- [x] oxifont-db: in-memory indexed database with CSS Level 4 query engine
- [x] oxifont-db: stretch/style/weight narrowing per CSS Fonts Level 4 section 4.5
- [x] oxifont-db: fontconfig generic-alias resolution (sans-serif, serif, monospace, cursive, fantasy)
- [x] oxifont-db: variable-font wght-axis preference
- [x] oxifont-db: locale-aware name table reads (60+ BCP-47 to LCID mappings)
- [x] oxifont-db: opt-in JSON disk cache behind `cache` feature

### M3 (Complete)
- [x] oxifont-webfont: WOFF1 decode (zlib per-table via oxiarc-deflate)
- [x] oxifont-webfont: WOFF2 decode (brotli via oxiarc-brotli)
- [x] oxifont-webfont: WOFF2 transformed glyf/loca reconstruction (triplet decoding, 255UInt16, composite, bbox bitmap, instruction streams)
- [x] oxifont-webfont: WOFF2 transformed hmtx reconstruction (proportional/mono lsb omission)
- [x] oxifont-subset: TrueType subsetting with composite glyph closure
- [x] oxifont-subset: cmap format 4/12 rewriting, hmtx/vmtx/hhea/vhea rewriting
- [x] oxifont-subset: HVAR/VVAR delta-set index map rewriting for variable fonts
- [x] oxifont-subset: verbatim fvar/gvar/avar copy, post v3, name table pruning

### M4 (Complete)
- [x] oxifont-adapter-native: CoreText (macOS) with weight mapping, symbolic traits, font path extraction
- [x] oxifont-adapter-native: DirectWrite (Windows) with COM enumeration, local font file loader, localized strings

### M5 (In Progress)
- [x] CFF/CFF2 outline subsetting in oxifont-subset (~500 SLOC)
- [x] GSUB/GPOS table subsetting: prune lookups for removed GIDs (~450 SLOC)
- [x] gvar per-glyph variation tuple subsetting for variable fonts (~150 SLOC)
- [x] WOFF1/WOFF2 encoding (SFNT -> WOFF conversion) (~450 SLOC) (planned 2026-05-25)
  - **Goal:** oxifont-webfont can encode SFNT → WOFF1 and SFNT → WOFF2; facade oxifont exposes subset_and_encode_woff2.
  - **Design:** WOFF1: per-table oxiarc_deflate zlib_compress + header/directory writer. WOFF2: glyf/loca/hmtx forward transforms (inverse of decoder), single brotli stream via oxiarc_brotli, UIntBase128/255UInt16 writers, transform-version asymmetry handled per-tag. detect_format/decode_auto/DecodeResult API lands in oxifont-webfont. subset_and_encode_woff2 in facade behind subset+woff2 features.
  - **Files:** `crates/oxifont-webfont/src/woff1/encode.rs`, `src/woff2/encode.rs`, `src/detect.rs`, `src/lib.rs`; `crates/oxifont/src/lib.rs`, `Cargo.toml`.
  - **Prerequisites:** oxiarc-deflate + oxiarc-brotli (already deps); subset_font in oxifont-subset (exists at lib.rs:513).
  - **Tests:** tests/woff1_encode.rs, tests/woff2_encode.rs, tests/detect.rs (round-trips with build_sfnt + real TTF); crates/oxifont/tests/subset_encode.rs.
  - **Risk:** WOFF2 triplet encoding off-by-one; transform-version asymmetry mis-set. Mitigation: decoder as oracle, transform layer tested independently.
- [x] Font outline extraction in oxifont-parser (glyf/CFF -> path commands) (~160 SLOC)
- [x] FontStretch, FontMetrics, GlyphOutline, KerningPair, ColorGlyphFormat types in oxifont-core
- [x] Full FontFace trait implementation in oxifont-parser: metrics, outline, kern, glyph_count, color detection, PostScript name, table queries, vertical advance
- [x] Facade convenience APIs: load_font, load_font_bytes, detect_format, decode_and_parse, prelude module, version()
- [x] CoreText FontStretch extraction, DirectWrite FontStretch extraction

### M6 (Planned)
- [x] COLR/CPAL subsetting for color fonts
- [x] SVG/sbix/CBDT bitmap font subsetting
- [x] fontconfig XML config parsing for Linux font discovery
- [x] Font fallback chains with codepoint coverage queries
- [x] Async font loading APIs
- [x] GDEF table subsetting

### M7 (In Progress)
- [x] oxifont-bundled: SIL-OFL-licensed Noto font subsets for environments without system fonts
- [x] Binary cache format (replace JSON with compact binary for faster cold start)
- [ ] TrueType hinting interpreter (deferred: modern CFF outlines and oxitext pseudo-hinting cover realistic use cases) **DEFERRED: modern CFF outlines and oxitext pseudo-hinting cover realistic rendering use cases; a full hinting interpreter adds ~2000 SLOC of complex bytecode execution for marginal quality gain at target resolutions.**

## Cross-Crate Tasks
- [x] Unify `VariationAxis` (oxifont-core) and `VariableAxis` (oxifont-db) into a single shared type
- [x] Bridge `FaceInfo` between oxifont-core and oxifont-db with From impls
- [x] Share SFNT table directory parsing via `SfntTableMap<'a>` in oxifont-core (avoid double-parse between parser + subset) (planned 2026-05-26)
  - **Goal:** Lightweight zero-copy `SfntTableMap<'a>` in `oxifont-core/src/sfnt.rs` consumed by both `oxifont-parser` and `oxifont-subset`. Eliminates the independent SFNT directory walks each currently performs.
  - **Design:** `pub struct SfntTableMap<'a> { sfnt_version: u32, tables: BTreeMap<[u8;4], &'a [u8]>, raw: &'a [u8] }`. Methods: `parse(data: &'a [u8]) -> Result<Self, SfntError>`, `table(&self, tag: &[u8;4]) -> Option<&'a [u8]>`, `tags()`, `raw()`. Error enum: `Truncated`, `BadMagic(u32)`, `DuplicateTag([u8;4])`, `OutOfBounds([u8;4])`. Parser adds `with_table_map` method. Subset's `read_table_directory` delegates to `SfntTableMap::parse`. New public API: `oxifont_subset::subset_with_table_map(map: &SfntTableMap, gid_set: &BTreeSet<u16>, opts: &SubsetOptions) -> Result<(Vec<u8>, SubsetStats), SubsetError>`.
  - **Files:** `crates/oxifont-core/src/sfnt.rs` (new), `crates/oxifont-core/src/lib.rs` (`pub mod sfnt;`), `crates/oxifont-parser/src/lib.rs` (`with_table_map`), `crates/oxifont-subset/src/tables.rs` (delegate to SfntTableMap), `crates/oxifont-subset/src/lib.rs` (`subset_with_table_map`).
  - **Tests:** `crates/oxifont-core/tests/sfnt_table_map.rs` (parse fixture, corrupt magic, truncated); `crates/oxifont-subset/tests/shared_table_map.rs` (byte-compare subset_with_table_map vs subset_font); `crates/oxifont-parser/tests/parse.rs` extension.
  - **Risk:** Purely additive — no existing API breaks.
- [x] Fix HVAR/VVAR offset field mapping (documented FIXME in varfont.rs) (planned 2026-05-25)
  - **Goal:** oxifont-subset rewrites advanceWidthMappingOffset from the correct field position (bytes 8-11, not 4-7).
  - **Design:** In `crates/oxifont-subset/src/varfont.rs` around line 167: HVAR/VVAR header layout is majorVersion(u16) minorVersion(u16) itemVariationStoreOffset(Offset32, bytes 4-7) advanceWidthMappingOffset(Offset32, bytes 8-11). Current code reads bytes 4-7 — wrong. Fix to read/write bytes 8-11; leave bytes 4-7 (IVS offset) untouched; remove stale FIXME comment.
  - **Files:** `crates/oxifont-subset/src/varfont.rs`.
  - **Prerequisites:** none.
  - **Tests:** Synthetic HVAR table with distinct IVS/advanceWidthMapping/lsb offsets; assert each field read correctly before and after rewrite.
  - **Risk:** Synthetic-test may mis-encode the layout. Mitigation: cross-check byte offsets against spec in test comment; assert each field independently.
- [x] End-to-end integration test: discover -> query -> subset -> encode WOFF2 -> decode -> verify

## Per-Subcrate TODOs
See individual TODO.md files in each subcrate directory:
- `crates/oxifont-core/TODO.md`
- `crates/oxifont-parser/TODO.md`
- `crates/oxifont-discovery/TODO.md`
- `crates/oxifont-adapter-pure/TODO.md`
- `crates/oxifont-adapter-native/TODO.md`
- `crates/oxifont-db/TODO.md`
- `crates/oxifont-subset/TODO.md`
- `crates/oxifont-webfont/TODO.md`
- `crates/oxifont/TODO.md`
