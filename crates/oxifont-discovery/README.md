# oxifont-discovery — Pure-Rust OS font-directory discovery for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-discovery.svg)](https://crates.io/crates/oxifont-discovery)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-discovery` is the system-font discovery layer of the OxiFont family. It locates well-known font directories on Linux, macOS, Windows, the BSDs, and Android, then walks them with [`walkdir`](https://crates.io/crates/walkdir) and parses every discovered file with [`oxifont-parser`](https://crates.io/crates/oxifont-parser) — **without calling `fontconfig`, `freetype`, or any native library**.

The scanner handles TTF/OTF (one face each), TTC/OTC collections (multiple faces), and — behind the `woff1` / `woff2` features — WOFF and WOFF2 files (decoded to SFNT before parsing). It offers fast paths for large font trees: metadata-only scans that read just the `name`, `OS/2`, and `cmap` tables; optional Rayon parallelism; optional memory-mapped I/O; background-thread scanning; and mtime tracking for cache invalidation. Each scan produces [`oxifont_core::FaceInfo`] records. The crate is Pure Rust and `#![forbid(unsafe_code)]` (the lone `unsafe` block for `mmap` is feature-gated).

## Installation

```toml
[dependencies]
oxifont-discovery = "0.1.0"
```

WOFF1 + WOFF2 decoding is on by default. To add parallel scanning, memory-mapped I/O, or fontconfig `fonts.conf` parsing:

```toml
[dependencies]
oxifont-discovery = { version = "0.1.0", features = ["rayon", "mmap", "fontconfig"] }
```

## Quick Start

```rust,no_run
let dirs = oxifont_discovery::system_font_dirs();
let faces = oxifont_discovery::scan_dirs(&dirs);

println!("found {} faces", faces.len());
for face in faces.iter().take(5) {
    println!("  {} (weight {})", &*face.family, face.weight);
}
```

### Reporting scan with timing and errors

```rust,no_run
let dirs = oxifont_discovery::system_font_dirs();
let result = oxifont_discovery::scan_dirs_reporting(&dirs);

println!(
    "{} files scanned, {} faces, {} errors, {} ms",
    result.files_scanned,
    result.faces.len(),
    result.total_errors(),
    result.elapsed.as_millis(),
);
```

### Background scanning

```rust,no_run
use oxifont_discovery::scan_system_fonts_background;

let handle = scan_system_fonts_background();
// ... do other work while fonts scan ...
let result = handle.join().expect("scan thread panicked");
println!("found {} faces", result.faces.len());
```

## API Overview

### Directory location

| Function | Description |
|----------|-------------|
| `system_font_dirs() -> Vec<PathBuf>` | Platform system-font directories (never touches the filesystem) |
| `user_font_dirs() -> Vec<PathBuf>` | Per-user font directories (`~/Library/Fonts`, `~/.fonts`, etc.) |

`system_font_dirs` covers macOS, Linux (incl. XDG, Flatpak, Snap paths), the BSDs, Android, and Windows. On Linux with the `fontconfig` feature it prefers directories parsed from `fonts.conf`.

### Single-file scanning

| Function | Description |
|----------|-------------|
| `scan_file(&Path) -> Result<Vec<FaceInfo>, FontError>` | Parse one font file (TTF/OTF/TTC/OTC, and WOFF/WOFF2 when enabled) |
| `read_face_metadata_partial(&Path) -> Result<Vec<FaceInfo>, FontError>` | Fast metadata read (SFNT: only `name`/`OS/2`/`cmap`; WOFF: full parse) |

### Directory scanning

| Function | Returns | Description |
|----------|---------|-------------|
| `scan_dirs(&[impl AsRef<Path>])` | `Vec<FaceInfo>` | Recursive scan, default options; bad files silently skipped |
| `scan_dirs_with_options(&[PathBuf], &ScanOptions)` | `Vec<FaceInfo>` | Recursive scan honouring `ScanOptions` |
| `scan_dirs_reporting(&[PathBuf])` | `ScanResult` | Captures successes, per-file errors, count, and elapsed time |
| `scan_dirs_metadata_only(&[PathBuf])` | `ScanResult` | Metadata-only fast scan (SFNT partial read) |
| `scan_dirs_with_progress(&[impl AsRef<Path>], FnMut(usize, &Path))` | `Vec<FaceInfo>` | Invokes a callback after each file |
| `scan_dirs_with_mtime(&[impl AsRef<Path>])` | `Vec<FaceWithMtime>` | Attaches each file's last-modified time |
| `max_mtime_of_dirs(&[impl AsRef<Path>]) -> SystemTime` | — | Fastest way to detect directory changes (no `FaceInfo` built) |

### Background & feature-gated scanning

| Function | Feature | Description |
|----------|---------|-------------|
| `scan_dirs_background(Vec<PathBuf>) -> JoinHandle<ScanResult>` | — | Scan on a background thread |
| `scan_system_fonts_background() -> JoinHandle<ScanResult>` | — | Background scan of the system directories |
| `scan_dirs_parallel(&[impl AsRef<Path> + Sync]) -> Vec<FaceInfo>` | `rayon` | Sequential walk, parallel parse |
| `read_font_file_mmap(&Path) -> io::Result<Vec<u8>>` | `mmap` | Read a file via a read-only memory map (copied to an owned `Vec`) |
| `scan_dirs_mmap(&[PathBuf]) -> ScanResult` | `mmap` | Reporting scan using memory-mapped I/O for SFNT files |

### `ScanOptions` (builder)

| Field / method | Default | Description |
|----------------|---------|-------------|
| `include_woff` / `.include_woff(bool)` | `true` | Include `.woff` files |
| `include_woff2` / `.include_woff2(bool)` | `true` | Include `.woff2` files |
| `follow_symlinks` / `.follow_symlinks(bool)` | `true` | Follow symbolic links during the walk |
| `max_depth` / `.max_depth(usize)` | `None` | Maximum directory recursion depth |

Accepted extensions are always `.ttf`, `.otf`, `.ttc`, `.otc`; `.woff` / `.woff2` are gated by the corresponding options.

### `ScanResult`

| Field / method | Type | Description |
|----------------|------|-------------|
| `faces` | `Vec<FaceInfo>` | Successfully parsed faces |
| `errors` | `Vec<(PathBuf, String)>` | Files that failed to parse, with messages |
| `files_scanned` | `usize` | Total font-extension files touched |
| `elapsed` | `Duration` | Wall-clock scan time |
| `total_errors()` | `usize` | Convenience accessor for `errors.len()` |

### `FaceWithMtime`

Returned by `scan_dirs_with_mtime`: `face: FaceInfo`, `mtime: SystemTime` (falls back to `UNIX_EPOCH`), `path: PathBuf`.

### `fontconfig` module (feature `fontconfig`)

| Function | Description |
|----------|-------------|
| `fontconfig_font_dirs() -> Vec<PathBuf>` | Parse `/etc/fonts/fonts.conf` + the user config, following `<include>` directives (cycle-safe); deduplicated, order-preserving |
| `parse_conf(&Path, &mut HashSet<PathBuf>, &mut Vec<PathBuf>)` | Parse a single fontconfig XML file, appending `<dir>` paths and recursing into includes |

Handles `~` / `$HOME` expansion and `prefix="xdg"` attributes on `<dir>` and `<include>` elements. Built on the Pure-Rust [`quick-xml`](https://crates.io/crates/quick-xml) parser.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `woff1` | yes | Decode `.woff` files via `oxifont-webfont` |
| `woff2` | yes | Decode `.woff2` files via `oxifont-webfont` |
| `rayon` | no | Parallel parsing (`scan_dirs_parallel`) |
| `fontconfig` | no | Parse `fonts.conf` for Linux font directories (`dep:quick-xml`, `dep:dirs`) |
| `mmap` | no | Memory-mapped file I/O (`dep:memmap2`); enables the only `unsafe` block |

When a WOFF file is encountered but its feature is not enabled, `scan_file` returns a single placeholder `FaceInfo` (path set, empty family) so the file is still discoverable.

## Errors

Single-file functions return [`oxifont_core::FontError`]:

| Variant | Cause |
|---------|-------|
| `IoError(Arc<std::io::Error>)` | File could not be read |
| `UnsupportedFormat` | Extension is not a recognised font format |
| `ParseError(String)` | Font (or WOFF) data could not be parsed |

Directory functions never fail as a whole — per-file failures are silently skipped (`scan_dirs*`) or collected into `ScanResult::errors` (the reporting / metadata / mmap variants).

## Cross-references

- [`oxifont-core`](../oxifont-core) — provides the `FaceInfo` records this crate emits and the `FontFace` trait used during parsing
- [`oxifont-parser`](../oxifont-parser) — the TTF/OTF/TTC parser backing every scan
- [`oxifont-webfont`](../oxifont-webfont) — WOFF1/WOFF2 decoders used by the `woff1` / `woff2` features
- [`oxifont-db`](../oxifont-db) — an indexed database with CSS Level 4 querying; an alternative ingestion path that can consume discovered faces
- [`oxifont`](../..) — the top-level façade

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
