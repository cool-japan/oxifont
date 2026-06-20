# Changelog

All notable changes to OxiFont are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
OxiFont adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.3] - 2026-06-19

### Changed

- All nine workspace member crates bumped from `0.1.2` to `0.1.3` (`oxifont-core`, `oxifont-parser`, `oxifont-discovery`, `oxifont-adapter-pure`, `oxifont-db`, `oxifont-webfont`, `oxifont-subset`, `oxifont-adapter-native`, `oxifont-bundled`).

[0.1.3]: https://github.com/cool-japan/oxifont/releases/tag/v0.1.3

---

## [0.1.2] - 2026-06-10

### Added

- **`oxifont-bundled`: `compressed` feature — build-time zlib compression via `build.rs`** — `build.rs` now reads every `.ttf` file from `fonts/`, compresses them with `oxiarc-deflate::zlib_compress` (level 6), and writes `<name>.ttf.z` files to `$OUT_DIR`; when the `compressed` feature is enabled, `BundledFont` embeds the zlib bytes via `include_bytes!(concat!(env!("OUT_DIR"), "..."))` and decompresses on first parse, reducing embedded binary size.
- **`oxifont-bundled`: `BundledFont::decompressed_data()` works correctly with actual compressed data** — removed the forward-compatibility SFNT-magic bypass in `decompress_font`; the function now directly calls `oxiarc_deflate::zlib_decompress`; the magic bypass was designed for a future build script that has now landed.
- **`oxifont-webfont`: WOFF2 glyf/loca passthrough support** — enhanced glyf/loca table handling with improved passthrough logic for non-transformed tables, and improved reconstruction logic for transformed glyf tables.
- **`oxifont-bundled`: additional compressed feature tests** — added `compressed_tests` module with `compressed_data_is_not_raw_sfnt` and round-trip validity tests; updated `sans_regular_ttf_magic`, `sans_bold_ttf_magic`, `serif_regular_ttf_magic`, and `all_fonts_have_valid_ttf_magic` to use `decompressed_data()` so they work under both compressed and non-compressed builds.

### Changed

- `oxifont-bundled` `decompressed_data_length_matches_raw_data` test updated to correctly assert that stored bytes are smaller than decompressed bytes under the `compressed` feature, and equal lengths without the feature.
- `oxiarc-deflate` bumped from `0.3.2` to `0.3.3`.
- `oxiarc-brotli` bumped from `0.3.2` to `0.3.3`.

[0.1.2]: https://github.com/cool-japan/oxifont/releases/tag/v0.1.2

---

## [0.1.1] - 2026-06-04

### Added

- **`oxifont-adapter-native`: `shaper_bridge` module** — new cross-platform public module (`pub mod shaper_bridge`) providing `collect_fallback_fonts_for_text`, `collect_fonts_for_text`, `load_best_native_font_for_text`, `load_native_font_for_codepoint_with_index`, and `find_native_font_for_codepoint`; lets shaping engines (oxitext-shape, swash, rustybuzz) obtain raw font bytes for every missing codepoint in a single OS font enumeration pass, avoiding the N×M overhead of one query per codepoint
- **`oxifont-adapter-native` (macOS)**: `load_fallback_font_bytes(codepoint)` and `load_fallback_font_bytes_with_index(codepoint)` — return raw SFNT bytes (and TTC face index) for the first system font covering the given codepoint via CoreText; allows shaping engines to call `FontRef::from_index` directly without managing path-to-bytes conversion
- **`oxifont-adapter-pure`: `FontDatabase::font_bytes(&self, info)` → `Result<Vec<u8>, FontError>`** — exposes raw SFNT bytes for a catalogued face, serving as the integration point for `oxifont-subset::subset_font` and WOFF2 encoding without requiring callers to import `oxifont-subset` directly
- **`oxifont-adapter-pure` feature `db`**: `FontDatabase::into_db(self)` and `FontDatabase::as_db(&self)` — convert the pure-Rust filesystem catalog to an `oxifont_db::FontDatabase`, enabling CSS Fonts Level 4 queries (`oxifont_db::Query`) on the result of a directory scan
- **`oxifont-adapter-pure` feature `subset`**: `FontDatabase::subset_face(info, codepoints)` and `FontDatabase::subset_face_for_web(info, codepoints)` — convenience wrappers that chain `font_bytes()` with `oxifont_subset::subset_font` / `subset_font_for_web` in one call; the `_for_web` variant strips hints and trims name records for smaller web font downloads
- **`oxifont-parser`: `GlyphOutlineData` struct** and **`ParsedFace::outline_with_bbox(gid)` → `Option<GlyphOutlineData>`** — returns path commands together with the font's own authoritative ink bounding box (`x_min`/`y_min`/`x_max`/`y_max`) and `hmtx` advance width/LSB, enabling rasterisation without fontdue or any third-party hinting library
- **`oxifont-parser`: `FontCapabilities` impl for `ParsedFace`** — implements `gsub_features()`, `gpos_features()`, `supported_scripts()`, `supported_languages()`, and `has_feature([u8; 4])`, giving shaping engines (oxitext-shape) GSUB/GPOS feature metadata without hand-parsing raw table bytes
- **`oxifont-db`: `FontDatabase::locale_families_for(bcp47)` → `Vec<String>`** — returns locale-specific family names for the given BCP-47 tag by resolving it to a Windows LCID with progressive tag-shortening fallback; primary integration point for `oxitext-icu` locale-aware rendering
- **`oxifont-db`: `FontDatabase::faces_for_script(script_tag: &[u8; 4])` → `Vec<&FaceInfo>`** — returns all faces whose OS/2 Unicode range bits cover the requested OpenType script tag (e.g. `b"arab"`, `b"deva"`, `b"hani"`); used by `oxitext-shape` for per-script font selection
- **`oxifont-subset`: `pdf_subset` module** — new `PdfFontSubsetter` builder and `PdfSubsetResult` struct for incremental PDF font subsetting; accumulates codepoints and raw GIDs across pages via `add_codepoint`, `add_text`, `add_gid`, then produces subset SFNT bytes + CIDToGIDMap in a single `finalize()` call; also exposes `cmap_to_gid_map_pub` for external cmap parsing
- **`oxifont-webfont`: `build_sfnt_cow` and `detect_sfnt_version_cow`** — zero-copy SFNT assembly variants that accept `Cow<'_, [u8]>` table slices; non-transformed tables borrow directly from the decompressed WOFF2 buffer, eliminating one copy per table in the hot WOFF2 decode path
- **`oxifont-adapter-native`**: DirectWrite integration test file (`tests/directwrite.rs`) with 8 platform-gated tests covering catalog enumeration, well-known Windows fonts, weight ranges, family names, path existence, `system_with_options`, reload stability, and italic face detection
- **Fuzz targets** added for `oxifont-db` (`fuzz_query`), `oxifont-parser` (`fuzz_parse`, `fuzz_face_methods`), `oxifont-subset` (`fuzz_subset`, `fuzz_subset_by_gids`), and `oxifont-webfont` (`fuzz_woff1_decode`, `fuzz_woff2_decode`, `fuzz_detect_auto`)

### Changed

- `NativeError` (`oxifont-adapter-native`) marked `#[non_exhaustive]` — downstream match expressions must include a catch-all arm; enables future variants without a semver break
- `FontError` (`oxifont-core`) marked `#[non_exhaustive]` — same forward-compatibility guarantee for the shared error type
- `SfntError` (`oxifont-core`) marked `#[non_exhaustive]`
- `GlyphOutline` (`oxifont-core`) coordinate-system documentation expanded with Y-axis convention, screen-space conversion pattern, and `oxitext-raster` field mapping (`cx`/`cy` → `x1`/`y1`); doc-example extended with Y-flip transform demonstration
- `oxifont-webfont` WOFF2 decode path switched from owned-table `extract_and_transform_tables` to the new `extract_and_transform_tables_cow` path, reducing per-table allocations for non-transformed tables
- `oxicode` updated from 0.2.3 to 0.2.4
- `dashmap` dependency removed from workspace
- `woff2-patched`, `ttf2woff2`, and `bytes` added as dev-dependencies in `oxifont-webfont` for the new `woff2_compare` benchmark

---

## [0.1.0] — 2026-06-01

Initial release of the OxiFont workspace — 10 crates, ~28 000 Rust SLOC,
zero FFI under default features.

### New Crates

| Crate | Description |
|---|---|
| `oxifont-core` | Core trait surface (`FontFace`, `FontCatalog`, `FontCollection`, `NameTable`), shared types (`FaceInfo`, `FontQuery`, `FontStyle`, `FontStretch`, `FontMetrics`, `GlyphOutline`, `KerningPair`, `ColorGlyphFormat`, `VariationAxis`), `SfntTableMap` zero-copy table directory |
| `oxifont-parser` | TTF/OTF/TTC parsing via `ttf-parser`; `ParsedFace` implementing `FontFace` with full metrics, outline extraction, kerning, color-glyph detection, PostScript name, table queries, vertical advance |
| `oxifont-discovery` | Pure Rust OS font-directory scanner for macOS, Linux, and Windows; `walkdir`-based recursion, WOFF/WOFF2 awareness, optional `fontconfig` XML config parsing |
| `oxifont-adapter-pure` | `FontDatabase` catalog via filesystem scan; CSS generic-family alias resolution; optional JSON/binary disk cache |
| `oxifont-adapter-native` | CoreText (macOS) and DirectWrite (Windows) native font enumeration; weight mapping, symbolic traits, localized strings; platform FFI behind the `native` feature |
| `oxifont-db` | In-memory indexed font database; CSS Fonts Level 4 §4.5 family/style/weight/stretch matching; `Query` builder; 60+ BCP-47 to LCID locale mappings; `cache` feature for JSON/binary disk cache |
| `oxifont-subset` | TrueType and CFF/CFF2 glyph subsetter; composite glyph closure; cmap (format 4/12) rewriting; hmtx/vmtx/hhea/vhea rewriting; GSUB/GPOS/GDEF layout pruning; HVAR/VVAR delta-set index map rewriting; gvar per-glyph variation tuple subsetting; COLR/CPAL, CBDT/CBLC, SVG, sbix, MATH table subsetting |
| `oxifont-webfont` | WOFF1 decode + encode (zlib per-table via `oxiarc-deflate`); WOFF2 decode + encode (brotli via `oxiarc-brotli`); transformed glyf/loca/hmtx reconstruction; streaming WOFF2 decoder; font-format autodetection |
| `oxifont-bundled` | Compile-time embedded SIL-OFL-1.1 Noto font subsets (Noto Sans, Noto Serif, Noto Sans Italic, Noto Sans Mono; CJK JP/KR/SC/TC behind sub-features); compressed storage via `oxiarc-deflate` |
| `oxifont` | Facade re-export crate; `load_font`, `load_font_bytes`, `detect_format`, `decode_and_parse`; feature-gated modules for each subcrate; `prelude` module; `version()` |

### Highlights

- **Pure Rust by default**: all default features are 100% FFI-free; CoreText and DirectWrite are opt-in via `native`
- **WOFF1 + WOFF2 round-trip**: encode and decode are both implemented and tested with real TTF fixtures
- **Full subsetting pipeline**: Unicode codepoint set → subsetted SFNT bytes covering TrueType, CFF, CFF2, variable fonts, color fonts (COLR, CBDT, SVG, sbix), and OpenType Layout tables
- **CSS Level 4 query engine**: family/weight/style/stretch narrowing per specification, generic-alias resolution, variable-font `wght`-axis preference, locale-aware name reads
- **SfntTableMap**: shared zero-copy SFNT directory parser in `oxifont-core` eliminates redundant table walks in parser and subsetter
- **949 tests** pass across the workspace (5 slow, 0 failures)
- **MSRV 1.89**, edition 2021

### Compression / Encoding Policy

All zlib/DEFLATE operations use `oxiarc-deflate`; all Brotli operations use
`oxiarc-brotli`. No `flate2`, `brotli`, `miniz_oxide`, or `zip` crates are
used anywhere in the dependency tree.

[0.1.1]: https://github.com/cool-japan/oxifont/releases/tag/v0.1.1
