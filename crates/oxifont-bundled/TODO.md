# oxifont-bundled TODO

## Status
Round 14 Slice 2 complete (2026-05-27). `BundledFont` struct extended with `parsed: OnceLock<Arc<ParsedFace>>` field and new `parsed_face()` method (OnceLock cache). `SANS_ITALIC` and `MONO_REGULAR` static constants added (variable-font TTFs from Google Fonts repo), both behind `bundled-noto` feature. `ALL_FONT_REFS` extended to 5 entries. `tests/bundled_italic_mono.rs` added (OnceLock Arc::ptr_eq, italic style, mono family, catalog count=5). `tests/wasm32_compile.rs` added. `cargo check --target wasm32-unknown-unknown` passes. 61 tests pass with `--all-features`, 18 pass with default features. Zero clippy warnings in all configurations.

## Core Implementation
- [x] Create `Cargo.toml` with `name = "oxifont-bundled"`, workspace version/edition/authors/license/repository, dependency on `oxifont-core`
- [x] Add `oxifont-bundled` to root `Cargo.toml` workspace members list
- [x] Add `oxifont-bundled` to root `[workspace.dependencies]` table
- [x] Obtain and embed Noto Sans Regular (OFL-1.1, ~424 KB TTF) into `fonts/NotoSans-Regular.ttf` as the default sans-serif fallback (behind `bundled-noto` feature)
- [x] Obtain and commit Noto Sans Bold (OFL-1.1) into `fonts/NotoSans-Bold.ttf`
- [x] Obtain and commit Noto Sans Italic (OFL-1.1) into `fonts/NotoSans-Italic.ttf` (variable-font TTF from Google Fonts repo, 2.2MB)
- [x] Obtain and commit Noto Sans Mono Regular (OFL-1.1) into `fonts/NotoSansMono-Regular.ttf` (variable-font TTF from Google Fonts repo, 1.6MB)
- [x] Add `pub static SANS_ITALIC: BundledFont` constant
- [x] Add `pub static MONO_REGULAR: BundledFont` constant
- [x] Test that `MONO_REGULAR` descriptor has `is_monospace = true`; note: ttf_parser `is_monospaced()` returns false for this variable font — family/PS name used for parsed-face identity test
- [x] Evaluate `once_cell`/`OnceLock` for caching parsed `ParsedFace` — implemented as `OnceLock<Arc<ParsedFace>>` field on `BundledFont`, `std::sync::OnceLock` (MSRV ≥ 1.70, no new dep)
- [x] Ensure `oxifont-bundled` works in `wasm32-unknown-unknown` — `cargo check --target wasm32-unknown-unknown -p oxifont-bundled --features bundled-noto` passes
  - **Goal:** Complete bundled Latin font matrix (Sans Regular/Bold/Italic, Serif Regular, Mono Regular) with parsed-face caching and wasm32 build smoke test.
  - **Design:**
    - **Font acquisition:** Run `curl -sI` on each URL before download. Primary: `https://github.com/notofonts/notosans/raw/main/fonts/NotoSans/full/ttf/NotoSans-Italic.ttf` and `https://github.com/notofonts/latin-greek-cyrillic/raw/main/fonts/NotoSansMono/full/ttf/NotoSansMono-Regular.ttf`. Fallback: Google Fonts repo, then Google Fonts API CSS parse. On all failures, mark font items back to `[ ]` and proceed with cache + wasm32 only.
    - **Constants:** `lib.rs` adds `pub const NOTO_SANS_ITALIC: &[u8] = include_bytes!("../fonts/NotoSans-Italic.ttf");` and `NOTO_SANS_MONO_REGULAR` gated `#[cfg(feature = "bundled-noto")]`. `catalog.rs` adds `pub static SANS_ITALIC: BundledFont` (style=Italic, weight=400, family="Noto Sans") and `pub static MONO_REGULAR: BundledFont` (family="Noto Sans Mono"). `ALL_FONT_REFS` extended to 5 entries.
    - **OnceLock cache:** `BundledFont` gains `parsed: OnceLock<Arc<ParsedFace>>`. New method `parsed_face(&self) -> Result<Arc<ParsedFace>, FontError>` using `get_or_init`. `std::sync::OnceLock` (MSRV ≥ 1.70, no new dep).
    - **wasm32 smoke:** `tests/wasm32_compile.rs` gated `#![cfg(target_arch = "wasm32")]`. Verify via `cargo check --target wasm32-unknown-unknown -p oxifont-bundled --features bundled-noto`.
  - **Files:** `fonts/NotoSans-Italic.ttf` (new), `fonts/NotoSansMono-Regular.ttf` (new), `src/lib.rs` (constants), `src/catalog.rs` (statics + cache), `tests/bundled_italic_mono.rs` (new), `tests/wasm32_compile.rs` (new).
  - **Tests:** `sans_italic_parses_with_italic_style`, `mono_regular_parses_as_monospace`, `bundled_catalog_includes_italic_and_mono` (len==5), `parsed_face_returns_same_arc_on_repeat_call` (Arc::ptr_eq).
  - **Risk:** Font download may fail in sandboxed environment. Cache + wasm32 land regardless.
- [x] Add `fonts/LICENSE-OFL.txt` with the SIL Open Font License for bundled Noto fonts
- [x] Implement `src/lib.rs` with `include_bytes!` statics gated on Cargo features
- [x] Implement `BundledFontProvider` struct providing `font_data()`, `by_name()`, and `ofl_license_text()` APIs
- [x] Implement `BundledFont` struct holding `&'static [u8]` data, family name, style, weight metadata (`src/catalog.rs`)
- [x] Implement `BundledCatalog` implementing `FontCatalog` trait over all bundled fonts
- [x] Add `pub static SANS_REGULAR: BundledFont` constant for direct access without catalog lookup
- [x] Add `pub static SANS_BOLD: BundledFont` constant (requires font file `fonts/NotoSans-Bold.ttf`)
- [x] Add `pub fn all() -> &'static [&'static BundledFont]` returning slice of all bundled fonts

## API Improvements
- [x] Add `BundledFont::family_name() -> &'static str` accessor for pre-parse family name queries
- [x] Add `BundledFont::weight() -> u16` and `BundledFont::style() -> FontStyle` accessors mirroring `FaceInfo` fields
- [x] Add `BundledFont::data() -> &'static [u8]` for raw font bytes access (e.g. for subsetting or passing to external renderers)
- [x] Add `BundledCatalog::find_by_family(name: &str) -> Option<&BundledFont>` convenience method with case-insensitive match
- [x] Add feature gate `noto-cjk` for optional CJK font bundle (Noto Sans CJK, ~16 MB, too large for default)
- [x] Add feature gate `noto-emoji` for optional emoji font bundle (Noto Color Emoji CBDT, ~10 MB)
- [x] Add feature gate `noto-serif` for optional serif font bundle (Noto Serif Regular/Bold/Italic)
- [x] Re-export `BundledCatalog` from the `oxifont` facade crate behind a `bundled` feature flag
- [x] Implement `Default` for `BundledCatalog` (same as `new(ALL_FONT_REFS)`)
- [x] Implement `Debug` for `BundledFont` showing family/weight/style without dumping raw bytes

## Testing
- [x] Test that each bundled font constant parses successfully via `oxifont-parser` (in `tests/bundled.rs`)
- [x] Test that `BundledCatalog::faces()` returns the expected number of entries
- [x] Test `BundledCatalog::find()` matches by family name, weight, and style combinations
- [x] Test that `SANS_REGULAR.parse()` returns a face with `family_name() == "Noto Sans"` and `weight() == 400`
- [x] Test that `all()` returns entries with distinct (family, weight, style) tuples
- [x] Test that raw `data()` bytes start with valid TTF/OTF magic number (0x00010000 or 0x4F54544F)
- [x] Compile-test with `--no-default-features` (default features are empty; passes with 18 tests)

## Performance
- [x] Benchmark `BundledFont::parse()` cold-start latency
  - **Design:** Added as part of Slice 5 criterion bench infrastructure; bench in `crates/oxifont-adapter-pure/benches/` or bundled benches.
- [ ] Measure binary size impact of each bundled font (report in README)
- [x] Add `compressed` feature: store fonts as OxiARC-deflate compressed, decompress on first `parse()` to reduce binary size (~80 SLOC) — API implemented (`BundledFont::decompressed_data()`, `compressed::decompress_font()`); build.rs compression generation deferred

## Integration
- [x] Wire `BundledCatalog` into `oxifont` facade crate as fallback when system font discovery returns zero fonts (`system_fonts_with_bundled_fallback()` in oxifont/src/lib.rs)
- [ ] Use `oxifont-bundled` as the default font source in `oxitext` rasterization tests (deterministic rendering without system fonts)
- [ ] Provide `oxifont_bundled::SANS_REGULAR` as default fallback in `oxipdf` text rendering when requested font is missing
- [x] Add integration test: round-trip from `BundledCatalog::find()` through `oxifont-parser::ParsedFace` to `FontFace::glyph_for_char('A')`
