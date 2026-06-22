# OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont.svg)](https://crates.io/crates/oxifont)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/cool-japan/oxifont/blob/main/LICENSE)
[![MSRV: 1.89](https://img.shields.io/badge/rustc-1.89+-lightgray.svg)](#)

OxiFont is the COOLJAPAN Pure Rust **font discovery, parsing, subsetting, and web-font encoding** layer for the `oxi*` ecosystem.
It replaces the `fontconfig` + `freetype` C/C++ dependency pair with zero-FFI Rust under default features.

OxiFont covers: enumerating installed fonts on Linux/macOS/Windows, parsing TTF/OTF/TTC/WOFF/WOFF2 byte streams, exposing glyph
metrics, CMap, OS/2 and `name` table data, performing CSS Level 4 family/weight/style/stretch matching, subsetting fonts to a
Unicode codepoint set, and encoding the result as WOFF1 or WOFF2. Rasterization, hinting execution, shaping, and layout are
**out of scope** by design — they belong in `oxitext`.

## Status: 0.2.1 — Unreleased

Full implementation across all M0–M7 milestones. 10 crates, ~31 000 Rust SLOC, 955 tests passing on v0.2.1 (Unreleased) (excluding slow native CoreText/DirectWrite tests).

## Feature Flags

| Feature | Default | Description |
|---|:---:|---|
| `pure` | yes | `FontDatabase` from pure Rust filesystem scan |
| `discovery` | yes | `scan_dirs`, `system_font_dirs`, `user_font_dirs` |
| `db` | no | `FontDatabase` with CSS Level 4 `Query` engine |
| `woff1` | no | WOFF1 decode and encode |
| `woff2` | no | WOFF2 decode and encode |
| `subset` | no | Glyph subsetting (`subset_font`, `SubsetOptions`) |
| `bundled-noto` | no | Embedded Noto Sans/Serif Latin/Greek/Cyrillic |
| `bundled-noto-cjk-jp` | no | Embedded Noto Sans JP |
| `bundled-noto-cjk-kr` | no | Embedded Noto Sans KR |
| `bundled-noto-cjk-sc` | no | Embedded Noto Sans SC |
| `bundled-noto-cjk-tc` | no | Embedded Noto Sans TC |

## Quick Start

```toml
[dependencies]
oxifont = "0.2"
```

```rust,no_run
use oxifont::{FontDatabase, FontCatalog as _, FontQuery};

let db = FontDatabase::system().unwrap();
if let Some(face) = db.find(&FontQuery::new().family("Arial")) {
    println!("found: {} weight={}", face.family, face.weight);
}
```

### CSS Level 4 Query Engine

```toml
[dependencies]
oxifont = { version = "0.2", features = ["db"] }
```

```rust,no_run
use oxifont::db::{FontDatabase as Db, Query};

let mut db = Db::new();
db.load_dir(std::path::Path::new("/usr/share/fonts")).ok();
if let Some(face) = Query::new(&db).family("sans-serif").weight(700).match_best() {
    println!("css match: {} weight={}", face.family, face.weight);
}
```

### Parse a Font File

```rust,no_run
use oxifont::{load_font, FontFace as _};

let face = load_font("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").unwrap();
println!("family: {}", face.family_name());
println!("units/em: {}", face.units_per_em());
println!("glyph count: {}", face.glyph_count());
```

### Subset and Encode to WOFF2

```toml
[dependencies]
oxifont = { version = "0.2", features = ["subset", "woff2"] }
```

```rust,no_run
use std::collections::BTreeSet;
use oxifont::{subset::subset_font, webfont::encode_woff2};

let bytes = std::fs::read("NotoSans-Regular.ttf").unwrap();
let codepoints: BTreeSet<char> = "Hello, 世界!".chars().collect();
let subsetted = subset_font(&bytes, &codepoints).unwrap();
let woff2 = encode_woff2(&subsetted).unwrap();
std::fs::write("subset.woff2", woff2).unwrap();
```

## Workspace Crates

| Crate | Description |
|---|---|
| [`oxifont-core`](crates/oxifont-core/) | Core traits and types: `FontFace`, `FontCatalog`, `FaceInfo`, `FontQuery`, `FontStyle`, `FontStretch`, `FontMetrics`, `GlyphOutline`, `KerningPair`, `ColorGlyphFormat`, `VariationAxis`, `SfntTableMap` |
| [`oxifont-parser`](crates/oxifont-parser/) | TTF/OTF/TTC parsing via `ttf-parser`; full `FontFace` impl with metrics, outline extraction, kerning, color glyph detection, PostScript name |
| [`oxifont-discovery`](crates/oxifont-discovery/) | Pure Rust OS font directory scanner (macOS/Linux/Windows); `walkdir`-based; optional fontconfig XML config parsing |
| [`oxifont-adapter-pure`](crates/oxifont-adapter-pure/) | `FontDatabase` catalog from filesystem scan; CSS generic-family aliases; optional disk cache |
| [`oxifont-adapter-native`](crates/oxifont-adapter-native/) | CoreText (macOS) and DirectWrite (Windows) native font enumeration behind the `native` feature |
| [`oxifont-db`](crates/oxifont-db/) | In-memory indexed database; CSS Fonts Level 4 §4.5 matching; 60+ BCP-47 locale mappings; `Query` builder; optional binary disk cache |
| [`oxifont-subset`](crates/oxifont-subset/) | TrueType and CFF/CFF2 glyph subsetter; GSUB/GPOS/GDEF pruning; HVAR/VVAR rewriting; COLR/CPAL, CBDT, SVG, sbix, MATH subsetting; variable font support |
| [`oxifont-webfont`](crates/oxifont-webfont/) | WOFF1 + WOFF2 decode and encode; transformed glyf/loca/hmtx reconstruction; streaming WOFF2 decoder; font-format autodetection |
| [`oxifont-bundled`](crates/oxifont-bundled/) | Compile-time embedded SIL-OFL-1.1 Noto fonts (Sans, Serif, Italic, Mono; CJK JP/KR/SC/TC behind sub-features) |
| [`oxifont`](crates/oxifont/) | Facade crate re-exporting the ecosystem; `load_font`, `load_font_bytes`, `detect_format`, `decode_and_parse`, `prelude` |

## Architecture

```
oxifont (facade)
├── oxifont-core           (traits + types)
├── oxifont-parser         (ttf-parser binding)
├── oxifont-discovery      (fs scan)
├── oxifont-adapter-pure   (FontDatabase)
├── oxifont-db             (CSS query engine)         [db feature]
├── oxifont-subset         (subsetter)                [subset feature]
├── oxifont-webfont        (WOFF1/WOFF2)              [woff1/woff2 features]
└── oxifont-bundled        (embedded Noto fonts)      [bundled-* features]

oxifont-adapter-native (CoreText / DirectWrite)       [depend on directly; native feature]
```

All default features use **zero FFI**. Native platform APIs (CoreText, DirectWrite)
are exposed through `oxifont-adapter-native`, which is no longer re-exported by
the facade crate; depend on it directly and enable its `native` feature. `fontconfig`
and `freetype` are permanently off-limits under any feature or adapter.

## Replaces

| Eliminated C/C++ dependency | Replacement |
|---|---|
| `fontconfig` (family matching, system enumeration) | Pure Rust fs-scan in `oxifont-discovery` + CSS Level 4 matcher in `oxifont-db` |
| `freetype` (font parsing, glyph outlines, hinting) | `ttf-parser` in `oxifont-parser`; hinting interpreter deliberately excluded |
| `harfbuzz-sys` (text shaping) | Out of scope for OxiFont — belongs to OxiText |

## Compression Policy

All DEFLATE/zlib operations use `oxiarc-deflate`; all Brotli operations use
`oxiarc-brotli`. No `flate2`, `brotli`, `miniz_oxide`, or `zip` are used
anywhere in the dependency tree.

## Inter-Oxi

**Depends on:** `oxiarc-deflate` (WOFF1), `oxiarc-brotli` (WOFF2).

**Depended on by:** OxiText (glyph metrics for layout), oxigaf (PDF CFF/Type-0
font embedding), oximedia (subtitle / OSD rendering), oxigdal-symbology (map
labels), oxiphoton (image text overlay), OxiUI (GUI text rendering).

## License

`Apache-2.0` for all Rust code.
Bundled Noto fonts in `oxifont-bundled` are licensed under the
[SIL Open Font License 1.1](LICENSE-FONTS-OFL.txt).
