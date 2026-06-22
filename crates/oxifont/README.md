# oxifont — The COOLJAPAN Pure-Rust font facade

[![Crates.io](https://img.shields.io/crates/v/oxifont.svg)](https://crates.io/crates/oxifont)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont` is the top-level facade for the OxiFont ecosystem: Pure-Rust font discovery, parsing, subsetting, and web-font encoding. A single dependency re-exports the most commonly needed items from the `oxifont-*` subcrates, with optional functionality gated behind feature flags. Every subcrate can also be used independently.

Out of the box (`default = ["pure", "discovery"]`) you get filesystem font discovery and a CSS-aware [`FontDatabase`] — no native libraries, no FFI, no C/C++ dependencies. Opt-in features add a CSS Level 4 query engine, WOFF1/WOFF2 codecs, glyph subsetting, and compile-time bundled Noto fonts.

## Installation

```toml
[dependencies]
# Default: pure-Rust filesystem discovery + FontDatabase
oxifont = "0.2.0"

# CSS Level 4 query engine
oxifont = { version = "0.2.0", features = ["db"] }

# WOFF2 decode/encode + glyph subsetting pipeline
oxifont = { version = "0.2.0", features = ["woff2", "subset"] }

# Bundled Noto fonts for environments without system fonts (WASM, CI, containers)
oxifont = { version = "0.2.0", features = ["bundled-noto"] }
```

## Quick Start

```rust,no_run
use oxifont::{FontDatabase, FontCatalog as _, FontQuery};

# fn main() -> Result<(), oxifont::FontError> {
let db = FontDatabase::system()?;
if let Some(face) = db.find(&FontQuery::new().family("Arial")) {
    println!("found: {} (weight {})", face.family, face.weight);
}
# Ok(())
# }
```

### Load and inspect a font file

```rust,no_run
# fn main() -> Result<(), oxifont::FontError> {
use oxifont::FontFace as _;

let face = oxifont::load_font("/System/Library/Fonts/Helvetica.ttc")?;
println!("{} weight={}", face.family_name(), face.weight());
# Ok(())
# }
```

### Detect, decode, and parse any container

```rust,no_run
# fn main() -> Result<(), oxifont::FontError> {
// Works for TTF/OTF/TTC always; WOFF1/WOFF2 with the matching feature.
let data = std::fs::read("font.woff2")?;
assert_eq!(oxifont::detect_format(&data), oxifont::FontFormat::Woff2);
let face = oxifont::decode_and_parse(&data)?;
# let _ = face;
# Ok(())
# }
```

## Architecture

Each subcrate in the OxiFont ecosystem is usable on its own; the facade simply
re-exports the most commonly needed items under one dependency.

| Subcrate | Role | Surfaced in `oxifont` as |
|----------|------|--------------------------|
| [`oxifont-core`](../oxifont-core) | Core traits (`FontFace`, `FontCatalog`) and shared types | re-exported at crate root |
| [`oxifont-parser`](../oxifont-parser) | TTF/OTF/TTC parsing | `parser` module + top-level `ParsedFace`, `face_count` |
| [`oxifont-discovery`](../oxifont-discovery) | Filesystem font-directory scanning | `discovery` module *(feature `discovery`)* |
| [`oxifont-adapter-pure`](../oxifont-adapter-pure) | Pure-Rust catalog from filesystem scan | `FontDatabase` *(feature `pure`)* |
| [`oxifont-db`](../oxifont-db) | In-memory indexed DB with CSS matching | `db` module *(feature `db`)* |
| [`oxifont-subset`](../oxifont-subset) | TrueType/CFF glyph subsetting | `subset` module *(feature `subset`)* |
| [`oxifont-webfont`](../oxifont-webfont) | WOFF1/WOFF2 encode & decode | `webfont` module *(features `woff1`/`woff2`)* |
| [`oxifont-bundled`](../oxifont-bundled) | Compile-time embedded Noto fonts | `bundled` module *(feature `bundled-noto`)* |

## Feature Flags

| Feature | Default | Enables |
|---------|---------|---------|
| `pure` | yes | `FontDatabase` from filesystem scan (via `oxifont-adapter-pure`) |
| `discovery` | yes | `discovery` module: `system_font_dirs`, `scan_dirs`, `scan_file`, `user_font_dirs`, `ScanOptions`, `ScanResult` |
| `db` | no | `db` module: `db::FontDatabase` with the CSS Level 4 `db::Query` engine; enables `system_fonts()` |
| `woff1` | no | `webfont` module: WOFF1 encode/decode; enables WOFF1 in `decode_and_parse` |
| `woff2` | no | `webfont` module: WOFF2 encode/decode; enables WOFF2 in `decode_and_parse` |
| `subset` | no | `subset` module: glyph subsetting (with `woff2`, enables `subset_and_encode_woff2`) |
| `bundled-noto` | no | `bundled` module: embedded Noto Sans/Serif/Mono bytes; enables `system_with_bundled()`, `bundled_fonts()` |
| `bundled-noto-serif` | no | Embedded Noto Serif (implies `bundled-noto`) |
| `bundled-noto-emoji` | no | Embedded Noto Emoji (implies `bundled-noto`) |
| `bundled-noto-cjk` | no | All four Noto CJK languages |
| `bundled-noto-cjk-jp` | no | Embedded Noto Sans CJK JP (Japanese) |
| `bundled-noto-cjk-kr` | no | Embedded Noto Sans CJK KR (Korean) |
| `bundled-noto-cjk-sc` | no | Embedded Noto Sans CJK SC (Simplified Chinese) |
| `bundled-noto-cjk-tc` | no | Embedded Noto Sans CJK TC (Traditional Chinese) |

## Re-exported Types at Crate Root

From [`oxifont-core`](../oxifont-core) (unconditional):

- Traits: `FontFace`, `FontCatalog`
- Types: `FaceInfo`, `FontError`, `FontMetrics`, `FontQuery`, `FontStretch`, `FontStyle`, `GlyphOutline`, `KerningPair`, `VariationAxis`, `ColorGlyphFormat`

From [`oxifont-parser`](../oxifont-parser) (unconditional): `ParsedFace`, `face_count`.

From [`oxifont-adapter-pure`](../oxifont-adapter-pure) *(feature `pure`)*: `FontDatabase`.

## Top-Level Functions

| Function | Feature | Description |
|----------|---------|-------------|
| `load_font(path) -> Result<ParsedFace, FontError>` | — | Read a file and parse face 0 |
| `load_font_bytes(data, face_index) -> Result<ParsedFace, FontError>` | — | Parse a specific face from in-memory bytes |
| `detect_format(data) -> FontFormat` | — | Identify the container from the first 4 magic bytes |
| `decode_and_parse(data) -> Result<ParsedFace, FontError>` | — | Detect, decode (WOFF1/2 if enabled), and parse into a `ParsedFace` |
| `system_fonts() -> Result<db::FontDatabase, FontError>` | `db` + `pure` | Populate a CSS `db::FontDatabase` from a pure filesystem scan |
| `system_fonts_with_bundled_fallback() -> Result<db::FontDatabase, FontError>` | `db` + `bundled-noto` | `system_fonts()` that injects bundled Noto fonts when discovery finds none |
| `system_with_bundled() -> bundled::provider::BundledFontProvider` | `bundled-noto` | A provider pre-loaded with the embedded Noto fonts |
| `bundled_fonts() -> bundled::BundledCatalog` | `bundled-noto` | The built-in bundled font catalog |
| `subset_and_encode_woff2(font_data, codepoints) -> Result<Vec<u8>, SubsetEncodeError>` | `subset` + `woff2` | Subset a font to the given chars, then encode as WOFF2 |
| `version() -> &'static str` | — | The `oxifont` crate version string |

### `FontFormat` (top level)

Enum returned by `detect_format`: `TrueType`, `OpenType`, `TrueTypeCollection`,
`Woff1`, `Woff2`, `Unknown`. Implements `Display`.

### `SubsetEncodeError` (feature `subset` + `woff2`)

Error from `subset_and_encode_woff2`. Implements `Display`, `std::error::Error`,
and `From` for both inner error types.

| Variant | Description |
|---------|-------------|
| `Subset(oxifont_subset::SubsetError)` | The subsetting step failed |
| `Encode(oxifont_webfont::WebFontError)` | The WOFF2 encoding step failed |

## Modules

### `parser`

```rust
use oxifont::parser::{ParsedFace, ParsedFaceBuilder};
```

### `discovery` *(feature `discovery`, default)*

```rust,no_run
use oxifont::discovery::{scan_dirs, scan_file, system_font_dirs, user_font_dirs, ScanOptions, ScanResult};
let dirs = system_font_dirs();
let faces = scan_dirs(&dirs);
# let _ = (faces, ScanOptions::default());
```

### `db` *(feature `db`)* — CSS Level 4 query engine

```rust,no_run
use oxifont::db::{FontDatabase as Db, Query};

let mut database = Db::new();
database.load_dir(std::path::Path::new("/usr/share/fonts")).ok();
if let Some(face) = Query::new(&database).family("sans-serif").weight(700).match_best() {
    println!("css match: {} weight={}", face.family, face.weight);
}
```

Re-exports `DbError`, `FaceInfo`, `FontDatabase`, `Query`, `Source`, and
`VariationAxis` from [`oxifont-db`](../oxifont-db).

> Note: `db::FaceInfo` is a **distinct** type from the crate-root `FaceInfo` (it
> carries an extended field set tailored to CSS matching). Likewise `db::FontDatabase`
> (CSS-indexed) is independent from the crate-root `FontDatabase` (filesystem scan).

### `webfont` *(feature `woff1` or `woff2`)*

Re-exports everything from [`oxifont-webfont`](../oxifont-webfont): `decode_woff2`,
`encode_woff2`, `decode_woff1`, `encode_woff1`, `decode_auto`, `WebFontError`, and more.

### `subset` *(feature `subset`)*

Re-exports everything from [`oxifont-subset`](../oxifont-subset), including
`subset_font` and `SubsetError`.

### `bundled` *(feature `bundled-noto`)*

Re-exports everything from [`oxifont-bundled`](../oxifont-bundled): `BundledCatalog`,
`BundledFont`, `all`, `ALL_FONT_REFS`, the `provider` module, and the font constants.

## Core Traits vs Database Types

- [`FontFace`] is the core trait for an individual face (outlines, metrics, glyph
  access). It is implemented by [`ParsedFace`]. Obtain one with `load_font`,
  `load_font_bytes`, or `decode_and_parse`.
- [`FontCatalog`] is the trait for a searchable font collection. It is implemented
  by the crate-root [`FontDatabase`] (feature `pure`, filesystem scan), by
  `db::FontDatabase` (feature `db`, CSS-indexed), and by `bundled::BundledCatalog`
  (feature `bundled-noto`).
- [`FaceInfo`] is a lightweight descriptor (on-disk path, family, weight, style) —
  it does **not** hold font bytes. Use `load_font` or a catalog's `load_face` to
  obtain the full `ParsedFace`.

## Prelude

```rust
use oxifont::prelude::*;
```

Imports the core traits (`FontFace`, `FontCatalog`), the shared types (`FaceInfo`,
`FontError`, `FontMetrics`, `FontQuery`, `FontStretch`, `FontStyle`, `GlyphOutline`,
`VariationAxis`, `ColorGlyphFormat`), and `ParsedFace`.

## Subset + WOFF2 Pipeline

```rust,no_run
# #[cfg(all(feature = "subset", feature = "woff2"))]
# fn main() -> Result<(), oxifont::SubsetEncodeError> {
use std::collections::BTreeSet;

let font_data = std::fs::read("NotoSans-Regular.ttf").unwrap();
let codepoints: BTreeSet<char> = "Hello".chars().collect();
let woff2 = oxifont::subset_and_encode_woff2(&font_data, &codepoints)?;
assert_eq!(&woff2[0..4], b"wOF2");
# Ok(())
# }
# #[cfg(not(all(feature = "subset", feature = "woff2")))]
# fn main() {}
```

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
