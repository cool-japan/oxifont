# Changelog

All notable changes to OxiFont are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
OxiFont adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **860 tests** pass across the workspace (21 slow, 0 failures)
- **MSRV 1.89**, edition 2021

### Compression / Encoding Policy

All zlib/DEFLATE operations use `oxiarc-deflate`; all Brotli operations use
`oxiarc-brotli`. No `flate2`, `brotli`, `miniz_oxide`, or `zip` crates are
used anywhere in the dependency tree.
