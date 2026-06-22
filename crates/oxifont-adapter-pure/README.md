# oxifont-adapter-pure — Pure-Rust filesystem font catalog for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-adapter-pure.svg)](https://crates.io/crates/oxifont-adapter-pure)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-adapter-pure` provides [`FontDatabase`], an in-memory catalog of font faces built by scanning directories on disk. It composes [`oxifont-discovery`](../oxifont-discovery) (recursive directory scan) and [`oxifont-parser`](../oxifont-parser) (TTF/OTF/TTC parsing) into a single [`FontCatalog`](../oxifont-core) implementation, and layers a CSS Fonts Level 4 matching engine on top. It is the **default, Pure-Rust** font backend of OxiFont — no native libraries, no FFI, no `unsafe` (`#![forbid(unsafe_code)]`).

`FontDatabase` is backed by a `Vec<FaceInfo>` for ordered storage plus a `HashMap<String, Vec<usize>>` index for O(1) family lookup by lowercase name, falling back to a linear substring scan to preserve `FontCatalog::find`'s documented case-insensitive substring semantics. It is the pure counterpart to [`oxifont-adapter-native`](../oxifont-adapter-native) (which uses CoreText/DirectWrite where available) and is itself the cross-platform fallback that the native adapter aliases on Linux and other targets.

## Installation

```toml
[dependencies]
oxifont-adapter-pure = "0.2.0"

# With the on-disk JSON metadata cache:
oxifont-adapter-pure = { version = "0.2.0", features = ["cache"] }
```

## Quick Start

```rust,no_run
use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FontCatalog as _, FontQuery};

# fn main() -> Result<(), oxifont_core::FontError> {
// Scan the OS system font directories.
let db = FontDatabase::system()?;
println!("found {} faces", db.faces().len());

// Substring match via the FontCatalog trait.
if let Some(face) = db.find(&FontQuery::new().family("Helvetica")) {
    println!("found: {}", face.family);
}

// Full CSS Level 4 matching (exact family + stretch/style/weight narrowing).
if let Some(face) = db.find_css(&FontQuery::new().family("sans-serif").weight(700)) {
    println!("css match: {} weight={}", face.family, face.weight);
}
# Ok(())
# }
```

### Building a catalog from known faces (no filesystem)

```rust
use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
use std::path::PathBuf;
use std::sync::Arc;

let face = FaceInfo {
    family: Arc::from("Arial"),
    post_script_name: String::new(),
    style: FontStyle::Normal,
    weight: 400,
    stretch: FontStretch::Normal,
    path: PathBuf::from("/dev/null"),
    face_index: 0,
    localized_families: Vec::new(),
};
let db = FontDatabase::from_faces(vec![face]);
let base = FontQuery::new().weight(400);
let result = db.find_with_fallback(&["Arial", "Helvetica", "sans-serif"], &base, "Hello");
assert!(result.is_some());
```

## API Overview

`FontDatabase` is `Debug`, `Default`, and `IntoIterator` (over `&FaceInfo`). It
implements [`oxifont_core::FontCatalog`].

### Constructors

| Method | Description |
|--------|-------------|
| `new() -> Self` | Empty database |
| `from_faces(Vec<FaceInfo>) -> Self` | Pre-populate from existing `FaceInfo` records (no scan) |
| `scan(paths) -> Result<Self, FontError>` | Recursively scan `paths` for font files (full parse) |
| `system() -> Result<Self, FontError>` | Scan the OS system font directories (full parse) |
| `scan_lazy(dirs) -> Result<Self, FontError>` | Metadata-only scan: reads only `name`, `OS/2`, `cmap` per file |
| `system_lazy() -> Result<Self, FontError>` | Metadata-only scan of system font dirs (10–50× faster; glyf/loca/hmtx never loaded) |
| `scan_cached(paths) -> Result<Self, FontError>` | *(feature `cache`)* Scan with an mtime-keyed JSON disk cache to skip re-parsing unchanged files |
| `system_cached() -> Result<Self, FontError>` | *(feature `cache`)* `scan_cached` over the system font dirs |

### Mutation (builder-style)

| Method | Description |
|--------|-------------|
| `add_dir(&mut self, path) -> &mut Self` | Scan a directory and add all faces found |
| `add_bytes(&mut self, bytes, family_hint) -> Result<usize, FontError>` | Parse in-memory bytes; adds every TTC sub-face (face 0 for TTF/OTF); returns count added |
| `remove(&mut self, path) -> usize` | Remove all faces with a matching path; returns count removed |
| `merge(&mut self, other: FontDatabase) -> &mut Self` | Merge all faces from another database |

### Query

| Method | Description |
|--------|-------------|
| `find(&FontQuery) -> Option<&FaceInfo>` *(trait)* | Case-insensitive **substring** family match (with index fast path) |
| `find_all(family) -> Vec<&FaceInfo>` | All faces whose family **exactly** matches (case-insensitive) |
| `find_css(&FontQuery) -> Option<&FaceInfo>` | Best match via CSS L4 §4.5 priority ordering (stretch → style → weight), with generic-family resolution |
| `resolve_generic_family(name, &FontQuery) -> Option<&FaceInfo>` | Resolve a CSS generic keyword (`sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`) to a concrete face |
| `find_with_fallback(families, base_query, text) -> Option<&FaceInfo>` | Try each family in order (each resolved via `find_css`) and return the first hit |
| `find_best_for_text(&FontQuery, text) -> Option<&FaceInfo>` | Convenience over `find_with_fallback` driven by `query.family` |
| `load_face(&FaceInfo) -> Result<ParsedFace, FontError>` | Load and fully parse the face described by a `FaceInfo` |
| `faces() -> &[FaceInfo]` *(trait)* | All faces in insertion order |
| `len()` / `is_empty()` | Face count helpers |

### CSS Fonts Level 4 matching

`find_css` and `find_with_fallback` apply the standard four-stage narrowing:

1. **Stretch** (§4.5.3) — nearest condensed/expanded width class.
2. **Style** (§4.5.4) — italic → oblique → normal preference ordering.
3. **Weight** (§4.5.5) — the full 400/500 special-casing plus nearest-below / nearest-above tiers.
4. **PostScript name** — optional exact-match refinement.

Generic CSS keywords resolve through a built-in alias table (e.g. `sans-serif` →
`Arial`, `Helvetica`, `DejaVu Sans`, `Liberation Sans`, …; `monospace` →
`Courier New`, `DejaVu Sans Mono`, …).

> The `text` parameter on `find_with_fallback` / `find_best_for_text` is accepted
> for forward-compatible cmap-coverage checking but is **not yet used** to filter
> candidates (`FaceInfo` carries no precomputed Unicode-range bitmask).

## Feature Flags

| Feature | Default | Pulls in | Description |
|---------|---------|----------|-------------|
| `cache` | no | `serde`, `serde_json`, `dirs`, `oxifont-core/serde` | Enables `scan_cached` / `system_cached`: an mtime-keyed JSON face cache at `<cache_dir>/oxifont/oxifont_face_cache.json`. Override the directory with the `OXIFONT_CACHE_DIR` environment variable. Cache failures degrade gracefully to a cold start. |

## Related Crates

- [`oxifont-discovery`](../oxifont-discovery) — directory scanning backing every constructor
- [`oxifont-parser`](../oxifont-parser) — TTF/OTF/TTC parsing and `ParsedFace`
- [`oxifont-core`](../oxifont-core) — `FontCatalog`, `FaceInfo`, `FontQuery`, `FontStyle`, `FontStretch`, `FontError`
- [`oxifont-adapter-native`](../oxifont-adapter-native) — OS-native counterpart (CoreText/DirectWrite); falls back to this crate on other platforms
- [`oxifont`](../oxifont) — facade crate; re-exports `FontDatabase` behind the default `pure` feature

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
