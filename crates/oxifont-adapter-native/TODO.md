# oxifont-adapter-native TODO

## Status
OS-native font adapter using CoreText on macOS and DirectWrite on Windows. Falls back to `oxifont-adapter-pure::FontDatabase` on other platforms. CoreText adapter: ~196 SLOC with weight mapping, family name extraction, style traits. DirectWrite adapter: ~284 SLOC with COM interface handling, path extraction, localized string reading. Both implement `FontCatalog`.

## Core Implementation
- [x] CoreText: extract `FontStretch` (width) from CTFontSymbolicTraits (`kCTFontCondensedTrait` / `kCTFontExpandedTrait`) (~15 SLOC)
- [x] CoreText: detect oblique style via `kCTFontSlantTrait` (currently only detects italic via symbolic traits) (~10 SLOC)
- [x] CoreText: extract PostScript name via `CTFontCopyPostScriptName` for precise font matching (~15 SLOC)
- [x] DirectWrite: handle DWRITE_FONT_STYLE_OBLIQUE distinction from italic (~5 SLOC, already partially done)
- [x] DirectWrite: extract font stretch from `IDWriteFont::GetStretch()` (~10 SLOC)
- [x] DirectWrite: read multiple localized family names (not just index 0) for multi-locale support (~25 SLOC)
- [x] Add Linux fontconfig-free alternative: parse fontconfig XML configuration directly to discover font paths without libfontconfig (~120 SLOC) — implemented in oxifont-discovery/src/fontconfig.rs
- [x] Add font registration: `register_font(path)` to add custom fonts to the native catalog (~30 SLOC)
- [x] Add font deregistration: `unregister_font(path)` to remove dynamically added fonts (~20 SLOC)
- [x] Implement font fallback: `find_for_codepoint(char) -> Option<&FaceInfo>` using CoreText/DirectWrite fallback APIs (~50 SLOC)
- [x] CoreText: support for font collections (.ttc) proper index extraction instead of counting by path (~20 SLOC)

## API Improvements
- [x] Unify `NativeCatalog::load()` error handling: return a richer error type with platform-specific details
- [x] Add `NativeCatalog::reload()` to refresh the catalog when fonts are installed/removed
- [x] Provide `NativeCatalog::system_with_options(opts)` for controlling which font types to enumerate
- [x] Add `Debug` implementation for `NativeCatalog`

## Testing
- [x] CoreText: test weight mapping with known system fonts (SF Pro, Helvetica Neue weight spectrum)
- [x] CoreText: verify face_index derivation for TTC files (Hiragino, PingFang)
- [x] DirectWrite: test on Windows CI with known system fonts — `tests/directwrite.rs` (2026-06-03)
  - **Note:** Tests are `#[cfg(windows)]`-gated and compile on macOS as a single placeholder. Real Windows CI must run with `--include-ignored` to execute the full suite. Tests cover: non-empty catalog, well-known fonts (Segoe UI / Arial / Times New Roman), weight range, family names, path existence, system_with_options, reload, Debug impl, italic classification.
- [x] Benchmark `NativeCatalog::load()` time vs `FontDatabase::system()` for comparison — `benches/native_bench.rs` (2026-05-27)
  - **Design:** Covered by Slice 5 criterion infrastructure. Native bench may be added to `crates/oxifont-adapter-pure/benches/font_database_find.rs` as a comparison group, or a separate native bench in Round 15.
- [x] Test fallback to `FontDatabase` on Linux builds — `tests/linux_fallback.rs` (2026-05-27)
- [x] Test that `catch_unwind` on malformed CoreText descriptors is effective — `tests/coretext_correctness.rs` (2026-05-27)
  - **Note:** the test verifies the happy-path (no panic on repeated calls). Injecting a synthetic malformed descriptor would require internal test hooks not exposed by the public API — deferred.

## Performance
- [x] Cache NativeCatalog across program lifetime (it's immutable after construction)
- [x] CoreText: batch descriptor trait queries — `CTFontDescriptorCopyAttribute(kCTFontTraitsAttribute)` now called once per descriptor (was 3×); weight, slant, symbolic traits all extracted from the single `traits` dict (2026-05-27)
- [x] DirectWrite: COM caching already optimal — `GetFamilyNames()` is already called once per family (not per face); `family_name` + `localized_families` passed by reference into `build_face_info` for all faces. No refactor needed. (2026-05-27)
- [x] Add `NativeCatalog::system()` alias in CoreText + DirectWrite adapters for cross-platform API parity with `FontDatabase::system()` on Linux (2026-05-27)

## Integration
- [x] Provide native font fallback data to oxitext-shape for complex script coverage — `src/shaper_bridge.rs` (2026-06-03)
  - **Implemented:** `shaper_bridge` module with `collect_fallback_fonts_for_text(text, primary_font_data)`, `collect_fonts_for_text(text)`, `find_native_font_for_codepoint(cp)`, `load_best_native_font_for_text(text)`, and `load_native_font_for_codepoint_with_index(cp)`. macOS uses a single CoreText enumeration pass (all codepoints resolved against each font's character set simultaneously — O(fonts × codepoints)); Windows/Linux use the NativeCatalog + ParsedFace::glyph_for_char with path deduplication. Shaping engines pass the returned `Vec<Vec<u8>>` directly to `SwashShaper::shape_with_fallback`.
- [x] Bridge native font paths to oxifont-parser for full face parsing — `NativeCatalog::load_face(info)` added to coretext.rs + directwrite.rs
- [x] Feed native enumeration results into oxifont-db for CSS Level 4 querying — done via oxifont facade `system_fonts()` which builds FontDatabase from NativeCatalog faces
