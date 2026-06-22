# oxifont-core — Core traits and types for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-core.svg)](https://crates.io/crates/oxifont-core)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-core` defines the public trait surface and shared data types that every other crate in the OxiFont family depends on. It contains **no parsing logic and no external dependencies** — concrete font parsers live in `oxifont-parser`, subsetting in `oxifont-subset`, and database/discovery in `oxifont-db` / `oxifont-discovery`.

The crate is Pure Rust with `#![forbid(unsafe_code)]` and is `no_std`-compatible when `alloc` is available (the `std` feature, enabled by default, unlocks `std::io::Error` conversion and the `FaceInfo::path` field). All glyph, metric, weight, style, and color-format types are defined here so that backends share one vocabulary.

## Installation

```toml
[dependencies]
oxifont-core = "0.2.0"
```

By default the `std` feature is enabled. For a `no_std` build:

```toml
[dependencies]
oxifont-core = { version = "0.2.0", default-features = false }
```

## Quick Start

```rust
use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};

// Implement `FontFace` for your parsed font type.
struct MyFace;

impl FontFace for MyFace {
    fn family_name(&self) -> &str { "MyFont" }
    fn style(&self) -> FontStyle { FontStyle::Normal }
    fn weight(&self) -> u16 { 400 }
    fn is_monospace(&self) -> bool { false }
    fn units_per_em(&self) -> u16 { 1000 }
    fn glyph_for_char(&self, c: char) -> Option<u16> {
        if c == 'A' { Some(36) } else { None }
    }
    fn advance_width(&self, gid: u16) -> Option<u16> {
        if gid == 36 { Some(611) } else { None }
    }
    fn axes(&self) -> &[VariationAxis] { &[] }
}

let face = MyFace;
assert_eq!(face.family_name(), "MyFont");
assert_eq!(face.glyph_for_char('A'), Some(36));
assert_eq!(face.stretch(), FontStretch::Normal); // default method
```

### Building a query

```rust
use oxifont_core::{FontQuery, FontStyle, FontStretch};

let q = FontQuery::new()
    .family("Arial")
    .style(FontStyle::Italic)
    .weight(700)
    .stretch(FontStretch::SemiCondensed);

assert_eq!(q.family.as_deref(), Some("Arial"));
assert_eq!(q.weight, Some(700));
```

## API Overview

### `FontFace` trait

The central abstraction: a parsed, in-memory font face that answers metric and glyph queries without returning lifetimed references (easy to move across threads/async). Required methods have no default; the rest provide sensible defaults.

| Method | Default | Description |
|--------|---------|-------------|
| `family_name(&self)` | — | Typographic family name (`&str`) |
| `style(&self)` | — | `FontStyle` classification |
| `weight(&self)` | — | CSS weight (100–900) |
| `is_monospace(&self)` | — | True when all advances are equal |
| `units_per_em(&self)` | — | Design units per EM (1000/2048) |
| `glyph_for_char(&self, c)` | — | Map a `char` to a glyph ID |
| `advance_width(&self, gid)` | — | Horizontal advance in design units |
| `axes(&self)` | — | `&[VariationAxis]` from `fvar` (empty if static) |
| `stretch(&self)` | `Normal` | CSS font-stretch classification |
| `metrics(&self)` | `None` | `FontMetrics` (OS/2 + hhea + post + head) |
| `outline(&self, gid)` | `None` | `Vec<GlyphOutline>` path commands |
| `kern(&self, l, r)` | `None` | Kerning value (kern table / GPOS PairPos) |
| `glyph_count(&self)` | `0` | Total glyphs (from `maxp`) |
| `color_glyph_format(&self)` | `None` | `ColorGlyphFormat` if any |
| `has_color_glyphs(&self)` | derives | True if `color_glyph_format()` is `Some` |
| `postscript_name(&self)` | `None` | PostScript name (name ID 6) |
| `has_table(&self, tag)` | `false` | Whether a 4-byte-tagged table is present |
| `vertical_advance(&self, gid)` | `None` | Vertical advance in design units |

### `FontCatalog` trait

A queryable collection of [`FaceInfo`] records.

| Method | Description |
|--------|-------------|
| `faces(&self)` | All `&[FaceInfo]` records in the catalog |
| `find(&self, query)` | First `&FaceInfo` matching a `FontQuery`, or `None` |

### `NameTable` trait

Object-safe access to the OpenType `name` table (copyright, family, PostScript name, license, etc.).

| Method | Description |
|--------|-------------|
| `name_record(&self, name_id, language)` | Best-match `String` for a name ID + BCP-47 language (English fallback) |
| `all_name_records(&self)` | Every record as `Vec<(name_id, language_tag, value)>` |

### `FontCapabilities` trait

Object-safe introspection of GSUB/GPOS layout tables.

| Method | Default | Description |
|--------|---------|-------------|
| `gsub_features(&self)` | — | 4-byte GSUB feature tags (`Vec<[u8;4]>`) |
| `gpos_features(&self)` | — | 4-byte GPOS feature tags |
| `supported_scripts(&self)` | — | Script tags present in GSUB/GPOS |
| `supported_languages(&self, script)` | — | Language-system tags for a script |
| `has_feature(&self, tag)` | derives | True if `tag` is in GSUB or GPOS |

### `FontCollection` trait

Abstracts TrueType/OpenType Collections (`.ttc` / `.otc`). Has an associated `type Face`.

| Method | Default | Description |
|--------|---------|-------------|
| `face_count(&self)` | — | Number of faces in the collection |
| `face_at(&self, index)` | — | `Result<Self::Face, FontError>` for a zero-based index |
| `faces(&self)` | derives | Iterator over `Result<Self::Face, FontError>` for all indices |

### `FaceInfo` struct

Lightweight, cheap-to-clone on-disk face metadata (no glyph data) used to build indices.

| Field | Type | Description |
|-------|------|-------------|
| `family` | `Arc<str>` | Typographic family name |
| `post_script_name` | `String` | PostScript name (empty if unavailable) |
| `style` | `FontStyle` | Italic / oblique / normal |
| `weight` | `u16` | CSS weight (100–900) |
| `stretch` | `FontStretch` | CSS font-stretch |
| `path` | `PathBuf` | Absolute path on disk (requires `std`) |
| `face_index` | `u32` | Index within a TTC (0 for TTF/OTF) |
| `localized_families` | `Vec<String>` | All localized family names |

### `FontQuery` struct & builder

A builder-style filter for matching `FaceInfo` inside a `FontCatalog`. All fields are `Option` wildcards.

| Field / method | Description |
|----------------|-------------|
| `FontQuery::new()` | Empty query (matches everything) |
| `family` / `.family(s)` | Family name (case-insensitive substring) |
| `style` / `.style(s)` | Desired `FontStyle` |
| `weight` / `.weight(w)` | Exact CSS weight |
| `stretch` / `.stretch(s)` | Desired `FontStretch` |
| `postscript_name` / `.postscript_name(s)` | Exact PostScript name (name ID 6) |

### `FontStyle` enum

CSS-ordered style: `Normal < Italic < Oblique`. Implements `Ord`, `Hash`, `Default` (= `Normal`).

| Associated function | Description |
|---------------------|-------------|
| `FontStyle::css_preference_score(requested, available)` | CSS Fonts L4 §4.5 style-matching score (`i32`); higher is a better match |

### `FontStretch` enum — 9 variants

CSS font-stretch / width (`#[repr(u8)]`, values 1–9, `Default` = `Normal`). Variants: `UltraCondensed`(1), `ExtraCondensed`(2), `Condensed`(3), `SemiCondensed`(4), `Normal`(5), `SemiExpanded`(6), `Expanded`(7), `ExtraExpanded`(8), `UltraExpanded`(9). Implements `Display` (kebab-case keywords).

| Method | Description |
|--------|-------------|
| `FontStretch::from_width_class(u8)` | Convert OS/2 `usWidthClass` (clamps to 1–9) |
| `stretch.to_width_class()` | Numeric width class (1–9) |

### `EmbeddingLicense` enum — 6 variants

Embedding policy derived from OS/2 `fsType`. Variants: `Installable`, `Restricted`, `PrintAndPreview`, `Editable`, `NoSubsetting`, `BitmapOnly`.

| Method | Description |
|--------|-------------|
| `EmbeddingLicense::from_fs_type(u16)` | Parse from `fsType`; modifier bits (`NoSubsetting`, `BitmapOnly`) take precedence |

### `FontMetrics` struct

Font-wide metrics (design units) from OS/2, hhea, head, and post: `units_per_em`, `ascender`, `descender`, `line_gap`, `cap_height: Option<i16>`, `x_height: Option<i16>`, `underline_position`, `underline_thickness`, `strikeout_position`, `strikeout_thickness`.

### `GlyphOutline` enum

A glyph path command (coordinates in design units): `MoveTo { x, y }`, `LineTo { x, y }`, `QuadTo { cx, cy, x, y }`, `CubicTo { cx1, cy1, cx2, cy2, x, y }`, `Close`.

### `KerningPair` struct

A kerning adjustment: `left_gid: u16`, `right_gid: u16`, `value: i16` (negative = tighter).

### `ColorGlyphFormat` enum — 5 variants

`ColrV0`, `ColrV1`, `Cbdt`, `Sbix`, `Svg`.

### `VariationAxis` struct

A variable-font axis record (`fvar`): `tag: [u8; 4]`, `min_value: f32`, `default_value: f32`, `max_value: f32`, `name: String`.

### `sfnt` module — zero-copy SFNT table directory

| Item | Description |
|------|-------------|
| `SfntTableMap<'a>` | Zero-copy view of a single per-face SFNT table directory (backed by a `BTreeMap`) |
| `SfntTableMap::parse(data)` | Parse a plain per-face SFNT (TTF/OTF); `ttcf` is rejected |
| `SfntTableMap::parse_at_offset(data, off)` | Parse a TTC-embedded SFNT at `off` (absolute table offsets) |
| `map.table(tag)` | Zero-copy `&[u8]` for a 4-byte tag, or `None` |
| `map.tags()` | Iterator over sorted table tags |
| `map.raw()` | Original raw per-face SFNT bytes |
| `map.num_tables()` | Number of tables in the directory |
| `map.sfnt_version` | The SFNT version/magic field (`u32`) |

### `FontError` variants

| Variant | Description |
|---------|-------------|
| `ParseError(String)` | Font bytes could not be parsed |
| `IoError(Arc<std::io::Error>)` | I/O failure reading a font file (`std` only; `Arc` makes `FontError` cheaply `Clone`) |
| `NotFound` | No matching face found |
| `UnsupportedFormat` | Not TTF/OTF/TTC |
| `IndexOutOfBounds { index, count }` | Face index beyond collection size |

### `SfntError` variants

| Variant | Description |
|---------|-------------|
| `Truncated` | Buffer too short for the header or directory |
| `BadMagic(u32)` | SFNT version is not a recognised per-face magic (`ttcf` rejected) |
| `DuplicateTag([u8; 4])` | A tag appears more than once |
| `OutOfBounds([u8; 4])` | A table's `offset + length` exceeds the buffer |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Enables `std::io::Error → FontError` conversion and the `FaceInfo::path` (`PathBuf`) field |
| `serde` | no | Derives `Serialize`/`Deserialize` on the public data types |

## Cross-references

`oxifont-core` is the foundation for the whole family. See:

- [`oxifont-parser`](../oxifont-parser) — TTF/OTF/TTC parsing that produces `FontFace` implementations
- [`oxifont-subset`](../oxifont-subset) — OpenType font subsetting
- [`oxifont-db`](../oxifont-db) — in-memory indexed font database with CSS Level 4 querying
- [`oxifont-discovery`](../oxifont-discovery) — OS font-directory discovery
- [`oxifont`](../..) — the top-level façade that re-exports these crates

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
