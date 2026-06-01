# oxifont-parser — Pure-Rust TTF/OTF/TTC font parser for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-parser.svg)](https://crates.io/crates/oxifont-parser)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-parser` is the parsing layer of the OxiFont family. It wraps [`ttf-parser`](https://crates.io/crates/ttf-parser) with **owned byte storage** so that a [`ParsedFace`] outlives the original byte slice, auto-detects the container format (TTF, OTF, TTC), and implements the [`FontFace`](https://crates.io/crates/oxifont-core) trait from `oxifont-core`.

Font bytes are held in an `Arc<[u8]>`, making `ParsedFace` cheaply `Clone`, `Send`, and `Sync`. All frequently-accessed metadata (family name, weight, style, stretch, metrics, axes, glyph count, colour format, variable-font flag) is extracted and cached at construction time, so those accessors are O(1). Glyph-level queries re-parse the underlying `ttf_parser::Face` on demand because it borrows from the byte buffer. The crate is `#![forbid(unsafe_code)]` and Pure Rust.

## Installation

```toml
[dependencies]
oxifont-parser = "0.1.0"
```

## Quick Start

```rust,no_run
use oxifont_parser::ParsedFace;
use oxifont_core::FontFace as _;

let bytes = std::fs::read("/path/to/font.ttf")?;
let face = ParsedFace::parse(bytes, 0)?;

println!("{} weight={}", face.family_name(), face.weight());

if let Some(gid) = face.glyph_for_char('A') {
    println!("glyph for 'A' = {gid}, advance = {:?}", face.advance_width(gid));
}
# Ok::<(), oxifont_core::FontError>(())
```

### Loading from a path or `FaceInfo`

```rust,no_run
use oxifont_parser::{ParsedFace, face_count};
use std::path::Path;

// How many faces does a collection contain?
let bytes = std::fs::read("/path/to/collection.ttc")?;
let n = face_count(&bytes);

// Parse each face in turn.
for index in 0..n {
    let face = ParsedFace::from_path(Path::new("/path/to/collection.ttc"), index)?;
    println!("face {index}: {}", face.family_name());
}
# Ok::<(), oxifont_core::FontError>(())
```

### Builder with a variable-font axis

```rust,no_run
use oxifont_parser::ParsedFace;

let bytes = std::fs::read("/path/to/variable.ttf")?;
let face = ParsedFace::builder(bytes)
    .face_index(0)
    .variation("wght", 700.0)
    .build()?;

assert!(face.is_variable());
# Ok::<(), oxifont_core::FontError>(())
```

## API Overview

### Free functions

| Function | Description |
|----------|-------------|
| `face_count(data: &[u8]) -> u32` | Number of faces in a collection; returns `1` for plain TTF/OTF |

### `ParsedFace` — construction

| Method | Description |
|--------|-------------|
| `ParsedFace::parse(data, face_index)` | Parse TTF/OTF/TTC from anything `Into<Arc<[u8]>>` |
| `ParsedFace::from_bytes(Vec<u8>, face_index)` | Convenience wrapper around `parse` for an owned `Vec` |
| `ParsedFace::from_path(&Path, face_index)` | Read a file and parse the face at `face_index` |
| `ParsedFace::from_face_info(&FaceInfo)` | Load and parse the face described by a catalog record |
| `ParsedFace::builder(Vec<u8>)` | Create a `ParsedFaceBuilder` |

### `ParsedFace` — accessors and table queries

| Method | Description |
|--------|-------------|
| `as_bytes(&self)` / `raw_bytes(&self)` | Borrow the raw font bytes (the latter named for `oxifont-subset`) |
| `as_face_info(&self)` | Produce a lightweight `FaceInfo` (empty `path`) |
| `table_data(&self, tag) -> Option<&[u8]>` | Raw bytes of an SFNT table by 4-byte tag |
| `with_table_map(&self, f)` | Run a closure against a zero-copy `SfntTableMap` (TTC-aware) |
| `vertical_origin(&self, gid) -> Option<(i16, i16)>` | `(x, y)` glyph vertical origin from the `VORG` table |
| `gsub_feature_tags(&self) -> Vec<[u8;4]>` | OpenType feature tags in GSUB |
| `gpos_feature_tags(&self) -> Vec<[u8;4]>` | OpenType feature tags in GPOS |
| `supported_scripts(&self) -> Vec<[u8;4]>` | Union of GSUB + GPOS script tags (deduplicated) |
| `supported_languages(&self, script) -> Vec<[u8;4]>` | Non-default LangSys tags for a script in GSUB |
| `is_variable(&self) -> bool` | Whether an `fvar` table is present (cached, O(1)) |
| `is_cff(&self) -> bool` | Whether outlines are CFF (`CFF ` or `CFF2` present) |
| `variation_coordinates(&self, settings) -> Option<Self>` | Clone with recorded `(tag, value)` axis settings; `None` if not variable |
| `variation_settings(&self) -> &[([u8;4], f32)]` | The applied variation settings, if any |
| `preload(self) -> Self` | No-op today; reserved for a future glyph-level cache |

### `FontFace` implementation

`ParsedFace` implements every method of [`oxifont_core::FontFace`]. Cached, O(1) accessors: `family_name`, `style`, `weight`, `stretch`, `is_monospace`, `units_per_em`, `axes`, `metrics`, `glyph_count`, `color_glyph_format`, `postscript_name`. On-demand (re-parse) queries: `glyph_for_char`, `advance_width`, `outline` (collected into `Vec<GlyphOutline>`), `kern` (legacy `kern` table, horizontal non-variable subtables), `has_table`, `vertical_advance`.

### `ParsedFaceBuilder`

Obtained via `ParsedFace::builder(data)`. Defaults: `face_index = 0`, no variation settings.

| Method | Description |
|--------|-------------|
| `ParsedFaceBuilder::new(Vec<u8>)` | Seed a builder with raw bytes |
| `.face_index(u32)` | Set the zero-based TTC face index |
| `.variation(tag: &str, value: f32)` | Add an axis setting; tags are padded/truncated to 4 ASCII bytes |
| `.build() -> Result<ParsedFace, FontError>` | Parse and apply settings; surfaces deferred tag-validation errors |

Tag handling in `variation()`: tags shorter than four characters are padded with trailing spaces, longer tags are truncated to four bytes, and a non-ASCII tag causes `build()` to return [`FontError::ParseError`].

## Errors

Parsing returns [`oxifont_core::FontError`]:

| Variant | Cause |
|---------|-------|
| `UnsupportedFormat` | Magic bytes are unrecognised / data too short for a header |
| `IndexOutOfBounds { index, count }` | TTC face index is out of range |
| `ParseError(String)` | Malformed table data, or a non-ASCII variation tag |
| `IoError(Arc<std::io::Error>)` | File could not be read (`from_path` / `from_face_info`) |

## Cross-references

- [`oxifont-core`](../oxifont-core) — the `FontFace` trait and shared types this crate implements
- [`oxifont-subset`](../oxifont-subset) — consumes a `ParsedFace` (via `raw_bytes()` / `with_table_map`) to build subsets
- [`oxifont-db`](../oxifont-db) / [`oxifont-discovery`](../oxifont-discovery) — build catalogs of `FaceInfo` records that feed `ParsedFace::from_face_info`
- [`oxifont`](../..) — the top-level façade

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
