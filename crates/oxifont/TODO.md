# oxifont (facade) TODO

## Status
Facade crate re-exporting the OxiFont ecosystem. Re-exports `oxifont-core` types unconditionally, `FontDatabase` from `oxifont-adapter-pure` behind `pure` feature, `oxifont-db` behind `db` feature, `NativeCatalog` behind `native` feature, webfont decoding behind `woff1`/`woff2` features, and subsetting behind `subset` feature. Bundled font catalog (`bundled_fonts()`) and system-with-bundled-fallback (`system_fonts_with_bundled_fallback()`) added behind `bundled-noto` / `db+bundled-noto` features. New feature stubs: `bundled-noto-serif`, `bundled-noto-emoji`, `bundled-noto-cjk` propagated. ~130 SLOC. Zero clippy warnings across all feature combos.

## Core Implementation
- [x] Add `oxifont::load_font(path) -> Result<ParsedFace, FontError>` top-level convenience function (~10 SLOC)
- [x] Add `oxifont::load_font_bytes(bytes, face_index) -> Result<ParsedFace, FontError>` (~10 SLOC)
- [x] Add `oxifont::system_fonts() -> FontDatabase` combining native adapter when available, falling back to pure (~15 SLOC)
- [x] Re-export `oxifont-parser::ParsedFace` and `face_count` unconditionally (~3 SLOC)
- [x] Add `parser` feature module re-exporting all oxifont-parser public API (~5 SLOC)
- [x] Add `discovery` feature module re-exporting oxifont-discovery functions (~5 SLOC)
- [x] Add `oxifont::detect_format(data) -> FontFormat` combining webfont format detection with SFNT detection (~20 SLOC)
- [x] Add `oxifont::decode_and_parse(data) -> Result<ParsedFace, FontError>` auto-detecting WOFF1/2 and decoding before parsing (~20 SLOC)
- [x] Add `oxifont::bundled_fonts() -> BundledCatalog` returning the built-in bundled font catalog (feature `bundled-noto`)
- [x] Add `oxifont::system_fonts_with_bundled_fallback() -> Result<db::FontDatabase, FontError>` that injects bundled fonts when system scan returns zero faces (features `db` + `bundled-noto`)

## API Improvements
- [x] Add a prelude module: `use oxifont::prelude::*` importing the most commonly used types and traits
- [x] Document feature flag matrix in crate-level docs with a compatibility table
- [x] Add `oxifont::version()` returning the crate version string
- [x] Ensure all re-exported types have consistent documentation

## Testing
- [x] Integration test: discover system fonts -> query by family -> load face -> read metrics
- [x] Integration test: decode WOFF2 -> parse -> subset -> verify round-trip
- [x] Test all feature combinations compile cleanly: `--no-default-features`, `--features=db`, `--features=native`, etc.
- [x] Test `system_with_native()` on macOS CI — `system_with_native_returns_faces_on_macos` + `native_catalog_load_face_bridge` in tests/facade.rs

## Performance
- [ ] N/A (facade crate, performance lives in subcrates)

## Integration
- [ ] Serve as the single dependency for oxitext's font needs
- [ ] Ensure API surface is stable enough for semver compatibility at 0.2.0
- [x] Document the relationship between oxifont-core traits and oxifont-db types
