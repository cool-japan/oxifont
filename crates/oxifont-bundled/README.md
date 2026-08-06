# oxifont-bundled — Compile-time embedded Noto fonts for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-bundled.svg)](https://crates.io/crates/oxifont-bundled)
[![License](https://img.shields.io/badge/license-Apache--2.0%20AND%20OFL--1.1-blue.svg)](LICENSE)

`oxifont-bundled` ships a small set of [Noto](https://fonts.google.com/noto) fonts embedded directly in the compiled binary via `include_bytes!`. It is the OxiFont answer to "what font do I use when there are *no* system fonts?" — embedded targets, WASM, sandboxed containers, and CI pipelines all benefit from a guaranteed, queryable minimal font set.

The crate is **100% Pure Rust** and forbids `unsafe` code. The bundled font data is licensed under the **SIL Open Font License 1.1** (hence the package license `Apache-2.0 AND OFL-1.1`); the Rust code itself is Apache-2.0. Every font is **opt-in** behind a feature flag, so nothing is embedded unless you ask for it.

> **CJK fonts are not vendored.** The Noto CJK faces are ~16 MB each and are deliberately *not* shipped in this crate. The `bundled-noto-cjk-*` features expose a `noto_sans_<lang>_regular()` accessor; you supply the real font at build time (see [Bundling CJK fonts](#bundling-cjk-fonts)). When no font is supplied the accessor returns a typed `FontError::NotFound` — it **never** hands out empty or fabricated font bytes, and the provider omits the entry entirely.

## Installation

```toml
[dependencies]
# Latin/Greek/Cyrillic Sans + Serif + Mono + Italic
oxifont-bundled = { version = "0.2.3", features = ["bundled-noto"] }

# Add the Japanese accessor (supply the real font at build time — see below)
oxifont-bundled = { version = "0.2.3", features = ["bundled-noto-cjk-jp"] }
```

With no feature flags the crate compiles but embeds no font bytes; the catalog
and provider are present but empty.

## Quick Start

```rust,no_run
use oxifont_bundled::BundledCatalog;
use oxifont_core::{FontCatalog as _, FontQuery};

// A catalog over every font compiled in (those enabled by feature flags).
let catalog = BundledCatalog::default();
for face in catalog.faces() {
    println!("{} weight={}", face.family, face.weight);
}

// Query it like any other FontCatalog.
if let Some(face) = catalog.find(&FontQuery::new().family("Noto Sans").weight(700)) {
    println!("matched: {}", face.post_script_name);
}
```

### Direct access to a bundled font (feature `bundled-noto`)

```rust
# #[cfg(feature = "bundled-noto")]
# fn main() -> Result<(), oxifont_core::FontError> {
use oxifont_bundled::SANS_REGULAR;
use oxifont_core::FontFace as _;

assert_eq!(SANS_REGULAR.family_name(), "Noto Sans");
assert_eq!(SANS_REGULAR.weight(), 400);

// Lazily parse once; subsequent calls return the same cached Arc.
let face = SANS_REGULAR.parsed_face()?;
assert!(!face.family_name().is_empty());
# Ok(())
# }
# #[cfg(not(feature = "bundled-noto"))]
# fn main() {}
```

### Raw bytes via the provider

```rust
use oxifont_bundled::provider::BundledFontProvider;

let provider = BundledFontProvider::new();
for (name, bytes) in provider.font_data() {
    println!("{name}: {} bytes", bytes.len());
}
```

## API Overview

### `BundledFont` — a statically embedded font descriptor

A zero-copy descriptor holding a `'static` byte slice and lightweight metadata.
`Clone` (cloning resets the lazy parsed-face cache). Re-exported at the crate root.

| Field | Type | Description |
|-------|------|-------------|
| `family` | `&'static str` | Typographic family name (e.g. `"Noto Sans"`) |
| `postscript_name` | `&'static str` | PostScript name (e.g. `"NotoSans-Regular"`) |
| `data` | `&'static [u8]` | Raw font bytes embedded via `include_bytes!` |
| `weight` | `u16` | CSS weight (100–900) |
| `style` | `FontStyle` | Style classification |
| `stretch` | `FontStretch` | Width classification |
| `is_monospace` | `bool` | Whether all glyphs share the same advance width |
| `parsed` | `OnceLock<Arc<ParsedFace>>` | Lazily-initialised parsed-face cache |

| Method | Description |
|--------|-------------|
| `family_name() -> &'static str` | Typographic family name |
| `weight() -> u16` | CSS weight |
| `style() -> FontStyle` | Style classification |
| `data() -> &'static [u8]` | Raw embedded bytes |
| `decompressed_data() -> Result<Vec<u8>, FontError>` | Owned bytes; decompresses when the `compressed` feature is active (otherwise a copy) |
| `parse() -> Result<ParsedFace, FontError>` | Parse the bytes into a fresh `ParsedFace` |
| `parsed_face() -> Result<Arc<ParsedFace>, FontError>` | Lazily parse once, cache, and return a cloned `Arc` |

### `BundledCatalog` — a `FontCatalog` over the bundled fonts

Pre-builds a `Vec<FaceInfo>` at construction so it implements
`oxifont_core::FontCatalog`. `Debug`, `Clone`, `Default`. Re-exported at the crate root.

| Method | Description |
|--------|-------------|
| `new(fonts: &'static [&'static BundledFont]) -> Self` | Build a catalog from a static slice of font references |
| `default() -> Self` | Build from `ALL_FONT_REFS` (all compiled-in fonts) |
| `fonts() -> &'static [&'static BundledFont]` | The underlying static descriptors |
| `find_by_family(family) -> Option<&'static BundledFont>` | First font matching `family` (case-insensitive) |
| `fonts_by_family(family) -> impl Iterator<Item = &'static BundledFont>` | All fonts matching `family` (case-insensitive) |
| `faces() -> &[FaceInfo]` *(trait)* | Pre-built `FaceInfo` slice |
| `find(&FontQuery) -> Option<&FaceInfo>` *(trait)* | Match family/weight/style/stretch/PostScript-name (set fields AND; unset are wildcards) |

### `BundledFontProvider` — `(name, bytes)` registry

A handle over every compiled-in font as raw byte slices. `Debug`, `Clone`, `Default`.
Lives in the `provider` module.

| Method | Description |
|--------|-------------|
| `new() -> Self` | Construct (no I/O; all data is static) |
| `font_data() -> Vec<(&'static str, &'static [u8])>` | All bundled fonts as `(name, bytes)` (CJK entries appear only when a real font was supplied at build time; empty/fake fonts are never listed) |
| `by_name(name) -> Option<&'static [u8]>` | Bytes for one font by its stable name identifier |
| `ofl_license_text() -> &'static str` *(feature `bundled-noto`)* | The embedded SIL OFL 1.1 license text |

### Free functions, constants, and statics

| Item | Feature | Description |
|------|---------|-------------|
| `all() -> &'static [&'static BundledFont]` | — | All compiled-in fonts (alias for `ALL_FONT_REFS`) |
| `ALL_FONT_REFS: &[&BundledFont]` | — | Static slice of every enabled bundled font (empty if none) |
| `SANS_REGULAR`, `SANS_BOLD`, `SERIF_REGULAR`, `SANS_ITALIC`, `MONO_REGULAR` | `bundled-noto` | `BundledFont` constants (re-exported at crate root) |
| `NOTO_SANS_REGULAR`, `NOTO_SANS_BOLD`, `NOTO_SERIF_REGULAR`, `NOTO_SANS_ITALIC`, `NOTO_SANS_MONO_REGULAR` | `bundled-noto` | Raw `&[u8]` byte statics |
| `noto_sans_{jp,kr,sc,tc}_regular() -> Result<&'static [u8], FontError>` | `bundled-noto-cjk-{jp,kr,sc,tc}` | CJK accessor — real bytes when supplied at build time, else `FontError::NotFound` (never empty/fake) |
| `compressed::decompress_font(data) -> Result<Vec<u8>, FontError>` | — | Runtime decompression helper; identity pass-through unless `compressed` is active |

## Feature Flags

| Feature | Default | Embeds |
|---------|---------|--------|
| `bundled-noto` | no | Noto Sans Regular/Bold, Noto Serif Regular, Noto Sans Italic, Noto Sans Mono Regular (Latin/Greek/Cyrillic) |
| `bundled-noto-serif` | no | Implies `bundled-noto` |
| `bundled-noto-emoji` | no | Implies `bundled-noto` |
| `bundled-noto-cjk` | no | Enables all four CJK sub-features below |
| `bundled-noto-cjk-jp` | no | Noto Sans JP Regular accessor (Japanese) — font supplied at build time |
| `bundled-noto-cjk-kr` | no | Noto Sans KR Regular accessor (Korean) — font supplied at build time |
| `bundled-noto-cjk-sc` | no | Noto Sans SC Regular accessor (Simplified Chinese) — font supplied at build time |
| `bundled-noto-cjk-tc` | no | Noto Sans TC Regular accessor (Traditional Chinese) — font supplied at build time |
| `compressed` | no | Pulls in `oxiarc-deflate` and switches `decompress_font` to a real zlib decoder (build-time compression step is future work) |

## Bundling CJK fonts

The Noto CJK faces (Japanese, Korean, Simplified/Traditional Chinese) are ~16 MB
each and are **not** vendored in this crate. Enabling a `bundled-noto-cjk-*`
feature exposes the matching accessor; you supply the real font yourself at build
time. When no font is supplied the accessor returns `FontError::NotFound` and the
build still succeeds — no empty or fabricated font is ever produced.

1. **Download the real font** from the official Noto CJK release
   (SIL Open Font License 1.1):
   <https://github.com/notofonts/noto-cjk/releases>. You need the static
   `NotoSans{JP,KR,SC,TC}-Regular.ttf` (OTF works too). Alternatively, subset a
   larger CJK font with [`oxifont-subset`](../oxifont-subset) to the glyph set you
   actually render and supply the subsetted TTF.

2. **Point the build at it** using either mechanism (env var wins if both exist):

   ```bash
   # Environment variable (recommended — keeps the repo clean):
   export OXIFONT_NOTO_CJK_JP=/path/to/NotoSansJP-Regular.ttf
   cargo build -p oxifont-bundled --features bundled-noto-cjk-jp

   # …or drop the file in-tree at the conventional path:
   #   crates/oxifont-bundled/fonts/cjk-jp/NotoSansJP-Regular.ttf
   ```

   The env vars are `OXIFONT_NOTO_CJK_{JP,KR,SC,TC}`. `build.rs` validates the
   SFNT magic and refuses to bundle a file that is not a real TrueType/OpenType
   font.

3. **Read the bytes at runtime:**

   ```rust,ignore
   match oxifont_bundled::noto_sans_jp_regular() {
       Ok(bytes) => { /* real, validated SFNT font bytes */ }
       Err(oxifont_core::FontError::NotFound) => { /* not supplied at build time */ }
       Err(e) => return Err(e),
   }
   ```

**Licensing:** the Noto CJK fonts are distributed under the
[SIL Open Font License 1.1](https://scripts.sil.org/OFL), the same license as the
Latin Noto faces vendored here (`fonts/LICENSE-OFL.txt`). When you redistribute a
binary that embeds a CJK font you supplied, you must comply with the OFL, which
includes retaining the copyright and license notice for that font. Because the
CJK bytes come from *your* build, their licensing and attribution are your
responsibility; this crate ships no CJK font data of its own.

## Related Crates

- [`oxifont-core`](../oxifont-core) — `FontCatalog`, `FaceInfo`, `FontQuery`, `FontError`
- [`oxifont-parser`](../oxifont-parser) — used by `BundledFont::parse` / `parsed_face`
- [`oxifont`](../oxifont) — facade crate; re-exports this as the `bundled` module and adds `system_with_bundled()` / `bundled_fonts()` behind the `bundled-noto` feature

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)

Bundled font data is licensed under the SIL Open Font License 1.1 (see
`fonts/LICENSE-OFL.txt`).
