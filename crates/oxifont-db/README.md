# oxifont-db — In-memory indexed font database with CSS Level 4 querying for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-db.svg)](https://crates.io/crates/oxifont-db)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-db` is the font-database layer of the OxiFont family. It builds an in-memory index of font faces (loaded from files, directories, or the OS system-font directories) and answers font-matching queries using a hybrid of the **CSS Fonts Module Level 4 §4.5** matching algorithm and **fontconfig**-style generic-alias resolution.

The store is a flat `Vec<FaceInfo>` with secondary case-folded family-name and PostScript-name indices for fast lookup. Each `FaceInfo` record carries weight, italic/oblique flag, CSS stretch, monospace flag, variable-font axes, per-locale family names (keyed by Windows LCID), and packed OS/2 Unicode-range bits — all extracted from the binary tables at load time. The optional `cache` feature persists the database to disk (Pure-Rust `oxicode` binary format, with a legacy JSON fallback) so cold scans of large font trees are paid only once. The crate is `#![forbid(unsafe_code)]` and Pure Rust.

## Installation

```toml
[dependencies]
oxifont-db = "0.2.0"
```

With the opt-in disk cache:

```toml
[dependencies]
oxifont-db = { version = "0.2.0", features = ["cache"] }
```

## Quick Start

```rust,no_run
use oxifont_db::{FontDatabase, Query};

let mut db = FontDatabase::new();
db.load_dir(std::path::Path::new("/usr/share/fonts")).ok();

if let Some(face) = Query::new(&db)
    .family("sans-serif")
    .weight(700)
    .italic(false)
    .match_best()
{
    println!("best match: {} weight={}", face.family, face.weight);
}
```

### Loading system fonts (with cache)

```rust,no_run
# #[cfg(feature = "cache")]
# fn run() -> Result<(), oxifont_db::DbError> {
use oxifont_db::FontDatabase;

// Tries the binary cache, then JSON, then a full system scan (writing both).
let db = FontDatabase::system_cached()?;
println!("{} faces, {} families", db.stats().face_count, db.stats().family_count);
# Ok(())
# }
```

## API Overview

### `FontDatabase`

The in-memory indexed store.

| Method | Description |
|--------|-------------|
| `FontDatabase::new()` / `default()` | Empty database |
| `FontDatabase::from_faces(Vec<FaceInfo>)` | Build from pre-parsed records |
| `FontDatabase::system()` | Build from the OS system-font directories |
| `FontDatabase::system_cached()` *(cache)* | Build from cache if fresh, else scan and write caches |
| `load_bytes(&mut self, Vec<u8>)` | Load faces from in-memory bytes (`Source::Memory`); returns count |
| `load_file(&mut self, &Path)` | Load faces from a file; returns count |
| `load_dir(&mut self, &Path)` | Recursively load `.ttf`/`.otf`/`.ttc` (best-effort) |
| `load_system_fonts(&mut self)` | Scan the OS system-font directories |
| `load_system_fonts_bg() -> JoinHandle<Self>` | Scan system fonts on a background thread |
| `add_face(&mut self, FaceInfo) -> usize` | Insert a record (assigns ID, updates indices) |
| `remove_face(&mut self, id) -> bool` | Remove by ID, reclaiming index entries |
| `sort_family_index(&mut self)` | Sort each family's faces by weight (call after bulk loads) |
| `faces(&self) -> &[FaceInfo]` | All face records |
| `faces_by_family(&self, name) -> Vec<&FaceInfo>` | Faces for a family (case-insensitive) |
| `face_by_id(&self, id) -> Option<&FaceInfo>` | Look up by numeric ID |
| `find_by_postscript_name(&self, name) -> Option<&FaceInfo>` | Exact, case-sensitive PostScript-name lookup |
| `stats(&self) -> DbStats` | Aggregate statistics |

Cache methods (require the `cache` feature): `save_cache(&Path)` / `load_cache(&Path)` (JSON), and `save_cache_binary(&Path)` / `load_cache_binary(&Path)` (Pure-Rust `oxicode` binary, magic `OXDB`). Only `Source::File` faces are persisted.

### OS system-font directories

`load_system_fonts` / `system()` scan:

| Platform | Directories |
|----------|-------------|
| macOS | `/Library/Fonts`, `/System/Library/Fonts`, `$HOME/Library/Fonts` |
| Linux | `/usr/share/fonts`, `/usr/local/share/fonts` |
| Windows | `C:\Windows\Fonts` |

### `DbStats`

| Field | Type | Description |
|-------|------|-------------|
| `face_count` | `usize` | Total faces in the database |
| `family_count` | `usize` | Distinct families (case-insensitive) |
| `cache_path` | `Option<PathBuf>` | Disk cache that backed this database, if any |

### `FaceInfo`

The primary per-face record (clonable, serde-serialisable for the cache).

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u32` | Unique ID assigned by the database |
| `family` | `String` | Typographic family name |
| `post_script_name` | `String` | PostScript name (name ID 6) |
| `weight` | `u16` | CSS weight (100–900) |
| `italic` | `bool` | Italic or oblique |
| `stretch` | `u8` | CSS stretch (1 = ultra-condensed … 9 = ultra-expanded) |
| `monospaced` | `bool` | All glyphs share one advance |
| `source` | `Source` | `File(PathBuf)` or `Memory(Vec<u8>)` |
| `face_index` | `u32` | Index within a TTC (0 for TTF/OTF) |
| `variable_axes` | `Vec<VariationAxis>` | `fvar` axes (empty for static) |
| `locale_families` | `Vec<(u16, String)>` | Per-locale family names keyed by Windows LCID |
| `unicode_ranges` | `u128` | Packed OS/2 `ulUnicodeRange1..4` (0 = unknown) |

| Method | Description |
|--------|-------------|
| `family_for_locale(&self, bcp47) -> &str` | Locale-specific family name, falling back to `family` |
| `covers_weight(&self, weight) -> bool` | Whether a `wght` axis covers `weight` |
| `covers_char_approx(&self, c) -> bool` | Approximate coverage via OS/2 range bits (`true` when unknown) |
| `supported_scripts_approx(&self) -> Vec<[u8;4]>` | Approximate OpenType script tags from range bits |

Implements `Display` (`"<family> <style> (weight: …, path: …)"`).

### `Source` enum

`File(PathBuf)` — bytes on disk (the normal case, the only kind persisted to cache); `Memory(Vec<u8>)` — inline bytes (for small test fixtures).

### `Query<'a>` — CSS Level 4 + fontconfig matcher

Builder-style query against a `FontDatabase`. Defaults: weight 400, non-italic, normal stretch (5), no locale.

| Method | Description |
|--------|-------------|
| `Query::new(&db)` | New query with default parameters |
| `.family(name)` | Add a family to the priority-ordered fallback list (generics expanded) |
| `.weight(u16)` | Desired CSS weight |
| `.italic(bool)` | Whether an italic face is desired |
| `.oblique(bool)` | Soft preference for oblique over true italic |
| `.stretch(u8)` | Desired CSS stretch (1–9) |
| `.locale(bcp47)` | Soft tie-break preference for a BCP-47 locale |
| `.match_best() -> Option<&FaceInfo>` | Best-matching face |
| `.match_all() -> Vec<&FaceInfo>` | All passing faces, best first |
| `.match_with_fallback(text) -> Vec<&FaceInfo>` | Fallback chain covering every codepoint in `text` |

Recognised generic family names (case-insensitive, fontconfig-style): `sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`, `cjk-sans-serif`, `cjk-serif`. Each expands to an ordered concrete-candidate list before the CSS narrowing phases (stretch → style → weight), after which a multi-dimensional tie-break (locale, variable-axis coverage, oblique) selects the winner. Variable fonts whose `wght` axis covers the requested weight are preferred over static faces.

### `locale` module

| Function | Description |
|----------|-------------|
| `bcp47_to_lcid(&str) -> Option<u16>` | Case-insensitive BCP-47 → Windows LCID (e.g. `"ja-JP"` → `0x0411`) |

### `DbError` variants

| Variant | Description |
|---------|-------------|
| `Io(std::io::Error)` | File read or directory-scan failure (implements `From<io::Error>`) |
| `ParseError(String)` | A font file could not be parsed |
| `Cache(String)` | Cache (de)serialisation or path resolution failure (`cache` feature) |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | — | No features enabled by default |
| `cache` | no | Disk cache: Pure-Rust `oxicode` binary format (`db_v1.bin`, magic `OXDB`) with a legacy `serde_json` fallback (`db_v1.json`), stored under `$XDG_CACHE_HOME/oxifont/` |

## Notes

- `VariationAxis` is re-exported from [`oxifont-core`]. The deprecated alias `VariableAxis` remains for backward compatibility — prefer `VariationAxis`.
- Conversions to/from [`oxifont_core::FaceInfo`] are provided (`From`/`TryFrom`); the db→core direction fails for `Source::Memory` records because the core type requires an on-disk path.

## Cross-references

- [`oxifont-core`](../oxifont-core) — provides `VariationAxis`, the `FontStyle`/`FontStretch` enums, and the slim `FaceInfo` this crate bridges to
- [`oxifont-parser`](../oxifont-parser) — the underlying TTF/OTF/TTC parser; `ParsedFace::from_face_info` consumes core `FaceInfo` records derived from this database
- [`oxifont-discovery`](../oxifont-discovery) — OS font-directory discovery, an alternative ingestion path
- [`oxifont`](../..) — the top-level façade

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
