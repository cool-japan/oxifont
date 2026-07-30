# OxiFont Project TODO

## Status
Pure Rust font discovery, parsing, subsetting, webfont processing, and TrueType hinting execution. **v0.2.1.**
11 crates in workspace, ~34 500 Rust SLOC, 1020 tests passing with all features enabled (0 failures, 2 skipped; 962 passing under default features). M0–M7 milestones complete.
Full pipeline: TTF/OTF/TTC parsing, filesystem and native (CoreText/DirectWrite) font enumeration,
CSS Fonts Level 4 matching, TrueType+CFF glyph subsetting, WOFF1/WOFF2 encode+decode,
bundled Noto fonts, SfntTableMap shared table directory, COLR/CBDT/SVG/sbix/MATH subsetting,
TrueType bytecode hinting execution (grid-fitting VM).

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

### M7 (Complete)
- [x] oxifont-bundled: SIL-OFL-licensed Noto font subsets for environments without system fonts
- [x] Binary cache format (replace JSON with compact binary for faster cold start)
- [x] TrueType hinting interpreter — DONE: implemented as the new `oxifont-hinting` crate (bytecode VM executing `fpgm`/`prep`/per-glyph instruction streams, grid-fitting outlines to 26.6 fixed-point coordinates; never panics on hostile bytecode). Supersedes the deferral previously noted here; see the Production-Readiness Backlog F1 entry below for full detail.

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


---

<!-- production-readiness-backlog 2026-07-16 -->
## Production-Readiness Backlog — 2026-07-16

_Consolidated from static audit + Opus adversarial bug-hunt (48 verified defects across noffi) + baseline nextest/clippy + design investigation. See `../NOFFI_PRODUCTION_BACKLOG.md` for the full cross-project list and severity/model legend. Not implemented; no commits._

**Confirmed bugs — Opus-verified — all 4 now FIXED (verified in working tree 2026-07-17):**
- [x] **S · high** `oxifont-webfont/src/woff2/glyf.rs:1373` — 255UInt16 decoder: `ONE_MORE_BYTE_CODE1` (255) now correctly maps to `byte + 253` and `ONE_MORE_BYTE_CODE2` (254) to `byte + 506`; no more 2-byte-word misread. Verified against WOFF2 §5.1 `Read255UShort`.
- [x] **S · high** `oxifont-webfont/src/woff2/header.rs:281` — `read_255_u16_slice` carries the same corrected 253/254/255 code mapping as the decoder above (collection-header parsing).
- [x] **S · med** `oxifont-webfont/src/woff2/glyf.rs:1171` — `total_points` is now validated against `flag_cur.remaining()` (with a `checked_add` overflow guard on the running sum) *before* any point-sized `Vec::with_capacity` allocation, closing the huge-alloc DoS.
- [x] **S · med** `oxifont-subset/src/cmap.rs:73` — format-4 builder now computes `length` via `checked_mul(8).and_then(checked_add(16))` bounded to `u16::MAX` and returns a typed `SubsetError::InvalidFont` instead of overflowing when a subset exceeds the ~8189-segment addressable size.
**Designed / audit:**
- [x] **A/hard/Opus · F1** TrueType hinting interpreter — DONE: new `oxifont-hinting` crate (workspace member, v0.2.1, `#![forbid(unsafe_code)]`, ~4,260 SLOC across `interp.rs`/`dispatch.rs`/`ops_*.rs`/`state.rs`/`math.rs`/`font.rs`, all files under the 2000-line policy limit). Runs `fpgm`/`prep`/per-glyph instruction streams over a `SfntTableMap`, bounds-checks every stack/storage/CVT/point/jump access, and never panics on hostile bytecode (typed `HintingError` instead). M7's "TrueType hinting interpreter (deferred)" line above is superseded by this crate.
- [x] **A/med/Opus · F2** bundled 0-byte fonts fix — DONE: CJK bundling is now honest opt-in. `bundled-noto-cjk-{jp,kr,sc,tc}` features expose `noto_sans_<lang>_regular()` accessors that read the real font from `OXIFONT_NOTO_CJK_<LANG>` env var or an in-tree `fonts/cjk-<lang>/` file at build time; when absent they return a typed `oxifont_core::FontError::NotFound` — never a fake/empty byte slice. See `crates/oxifont-bundled/src/lib.rs` and its TODO.md.
- [x] **B/easy · F3** unwrap reduction (29 non-test) — DONE 2026-07-17: audited `oxifont-webfont` (incl. `woff2/header.rs`, `woff2/glyf.rs`), `oxifont-parser`, `oxifont-adapter-pure`, `oxifont` lib, plus `oxifont-subset`/`oxifont-adapter-native`; zero non-test `.unwrap()`/`.expect()` remain reachable from untrusted font-input parsing (all remaining hits are `#[cfg(test)]`-gated or doc-comment examples; the handful in `oxifont-subset` are guarded immediately above by an `is_empty()` check and left with a `"non-empty"` justification comment). Added root `SECURITY.md` + `CONTRIBUTING.md`. Confirmed all 4 fuzz crates (`oxifont-webfont`, `oxifont-parser`, `oxifont-db`, `oxifont-subset`; 8 targets total) build with `cargo +nightly fuzz build`. `cargo nextest run --workspace`: 962 passed, 2 skipped. `cargo clippy --all-targets --workspace -- -D warnings`: clean.
