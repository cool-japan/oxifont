# oxifont-bundled — Compile-time embedded Noto fonts for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-bundled.svg)](https://crates.io/crates/oxifont-bundled)
[![License](https://img.shields.io/badge/license-Apache--2.0%20AND%20OFL--1.1-blue.svg)](LICENSE)

`oxifont-bundled` ships a small set of [Noto](https://fonts.google.com/noto) fonts embedded directly in the compiled binary via `include_bytes!`. It is the OxiFont answer to "what font do I use when there are *no* system fonts?" — embedded targets, WASM, sandboxed containers, and CI pipelines all benefit from a guaranteed, queryable minimal font set.

The crate is **100% Pure Rust** and forbids `unsafe` code. The bundled font data is licensed under the **SIL Open Font License 1.1** (hence the package license `Apache-2.0 AND OFL-1.1`); the Rust code itself is Apache-2.0. Every font is **opt-in** behind a feature flag, so nothing is embedded unless you ask for it.

> **CJK fonts are placeholders.** The `bundled-noto-cjk-*` features compile, but the embedded CJK files are intentionally zero-byte stubs. To use them, drop the real `NotoSans{JP,KR,SC,TC}-Regular.ttf` files into `crates/oxifont-bundled/fonts/cjk-*/` and rebuild with the matching feature. The provider API silently omits zero-length placeholders.

## Installation

```toml
[dependencies]
# Latin/Greek/Cyrillic Sans + Serif + Mono + Italic
oxifont-bundled = { version = "0.1.0", features = ["bundled-noto"] }

# Add Japanese (requires the real font file — see note above)
oxifont-bundled = { version = "0.1.0", features = ["bundled-noto-cjk-jp"] }
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
| `font_data() -> Vec<(&'static str, &'static [u8])>` | All bundled fonts as `(name, bytes)` (zero-byte CJK placeholders omitted) |
| `by_name(name) -> Option<&'static [u8]>` | Bytes for one font by its stable name identifier |
| `ofl_license_text() -> &'static str` *(feature `bundled-noto`)* | The embedded SIL OFL 1.1 license text |

### Free functions, constants, and statics

| Item | Feature | Description |
|------|---------|-------------|
| `all() -> &'static [&'static BundledFont]` | — | All compiled-in fonts (alias for `ALL_FONT_REFS`) |
| `ALL_FONT_REFS: &[&BundledFont]` | — | Static slice of every enabled bundled font (empty if none) |
| `SANS_REGULAR`, `SANS_BOLD`, `SERIF_REGULAR`, `SANS_ITALIC`, `MONO_REGULAR` | `bundled-noto` | `BundledFont` constants (re-exported at crate root) |
| `NOTO_SANS_REGULAR`, `NOTO_SANS_BOLD`, `NOTO_SERIF_REGULAR`, `NOTO_SANS_ITALIC`, `NOTO_SANS_MONO_REGULAR` | `bundled-noto` | Raw `&[u8]` byte statics |
| `NOTO_SANS_{JP,KR,SC,TC}_REGULAR` | `bundled-noto-cjk-{jp,kr,sc,tc}` | Raw `&[u8]` CJK byte statics (zero-length placeholders by default) |
| `compressed::decompress_font(data) -> Result<Vec<u8>, FontError>` | — | Runtime decompression helper; identity pass-through unless `compressed` is active |

## Feature Flags

| Feature | Default | Embeds |
|---------|---------|--------|
| `bundled-noto` | no | Noto Sans Regular/Bold, Noto Serif Regular, Noto Sans Italic, Noto Sans Mono Regular (Latin/Greek/Cyrillic) |
| `bundled-noto-serif` | no | Implies `bundled-noto` |
| `bundled-noto-emoji` | no | Implies `bundled-noto` |
| `bundled-noto-cjk` | no | Enables all four CJK sub-features below |
| `bundled-noto-cjk-jp` | no | Noto Sans JP Regular (Japanese) — placeholder until the real file is supplied |
| `bundled-noto-cjk-kr` | no | Noto Sans KR Regular (Korean) — placeholder |
| `bundled-noto-cjk-sc` | no | Noto Sans SC Regular (Simplified Chinese) — placeholder |
| `bundled-noto-cjk-tc` | no | Noto Sans TC Regular (Traditional Chinese) — placeholder |
| `compressed` | no | Pulls in `oxiarc-deflate` and switches `decompress_font` to a real zlib decoder (build-time compression step is future work) |

## Related Crates

- [`oxifont-core`](../oxifont-core) — `FontCatalog`, `FaceInfo`, `FontQuery`, `FontError`
- [`oxifont-parser`](../oxifont-parser) — used by `BundledFont::parse` / `parsed_face`
- [`oxifont`](../oxifont) — facade crate; re-exports this as the `bundled` module and adds `system_with_bundled()` / `bundled_fonts()` behind the `bundled-noto` feature

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)

Bundled font data is licensed under the SIL Open Font License 1.1 (see
`fonts/LICENSE-OFL.txt`).
