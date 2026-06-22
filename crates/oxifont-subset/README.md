# oxifont-subset — Pure-Rust OpenType font subsetter for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-subset.svg)](https://crates.io/crates/oxifont-subset)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-subset` is the subsetting layer of the OxiFont family. Given raw SFNT font bytes and a set of Unicode codepoints (or glyph IDs), it produces a new, minimal SFNT containing only the requested glyphs — plus `.notdef` and any transitively-referenced composite components — and rewrites every affected table so the result is a valid, standalone font.

The subsetter handles both TrueType (`glyf`/`loca`) and CFF/CFF2 outline formats, remaps glyph IDs to a dense space starting at 0, and rewrites the full table set: `glyf`, `loca`, `cmap`, `hmtx`/`vmtx`, `maxp`, `head`, `hhea`/`vhea`, `post`, `name`, layout tables (GSUB/GPOS/GDEF), `kern`, `OS/2`, variation tables (`gvar`, HVAR/VVAR), and colour tables (COLR, CPAL, CBDT/CBLC, sbix, SVG), plus MATH. It is `#![forbid(unsafe_code)]` and 100% Pure Rust. With the optional `parallel` feature the heavy independent table rewrites are dispatched to a Rayon thread pool; output is bit-for-bit identical to the sequential path.

## Installation

```toml
[dependencies]
oxifont-subset = "0.2.0"
```

With parallel table rewriting:

```toml
[dependencies]
oxifont-subset = { version = "0.2.0", features = ["parallel"] }
```

## Quick Start

```rust,no_run
use std::collections::BTreeSet;
use oxifont_subset::subset_font;

let font_data = std::fs::read("NotoSans-Regular.ttf")?;
let cps: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();

let subset_bytes = subset_font(&font_data, &cps)?;
std::fs::write("NotoSans-ABC.ttf", &subset_bytes)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### With options and statistics

```rust,no_run
use std::collections::BTreeSet;
use oxifont_subset::{subset_font_with_options, SubsetOptions};

let font_data = std::fs::read("NotoSans-Regular.ttf")?;
let cps: BTreeSet<char> = "Hello".chars().collect();

let opts = SubsetOptions::default()
    .strip_hints(true)    // drop fpgm/prep/cvt
    .retain_names(false); // keep only name IDs 0–6

let (bytes, stats) = subset_font_with_options(&font_data, &cps, &opts)?;
println!(
    "{} -> {} bytes, {} glyphs retained",
    stats.original_size, stats.subset_size, stats.glyphs_retained
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## API Overview

### High-level entry points

| Function | Description |
|----------|-------------|
| `subset_font(font_data, codepoints) -> Result<Vec<u8>, SubsetError>` | Subset by codepoints with default options |
| `subset_font_with_options(font_data, codepoints, opts) -> Result<(Vec<u8>, SubsetStats), _>` | Subset by codepoints with explicit options + stats |
| `subset_by_gids(font_data, gids) -> Result<Vec<u8>, SubsetError>` | Subset by an explicit set of old GIDs (empty `cmap`; PDF/print) |
| `subset_font_for_web(font_data, codepoints) -> Result<Vec<u8>, _>` | Preset: `strip_hints = true`, `retain_names = false` |
| `subset_font_for_pdf(font_data, codepoints) -> Result<Vec<u8>, _>` | Preset: `strip_hints = false`, `retain_names = true` |

### Lower-level / zero-copy entry points

| Function | Description |
|----------|-------------|
| `subset_with_gid_set(font_data, old_gid_set, cp_to_old_gid, opts) -> Result<(Vec<u8>, SubsetStats), _>` | Core engine: pre-computed old-GID set + codepoint→old-GID map |
| `subset_with_table_map(map, gid_set, cp_to_old_gid, opts) -> Result<(Vec<u8>, SubsetStats), _>` | As above but reuses a pre-parsed `oxifont_core::sfnt::SfntTableMap` (skips a second directory walk) |

`.notdef` (GID 0) is always retained implicitly, and the composite-component closure is always applied for TrueType fonts.

### `SubsetOptions` (builder)

| Field / method | Default | Description |
|----------------|---------|-------------|
| `strip_hints` / `.strip_hints(bool)` | `false` | Drop `fpgm`, `prep`, `cvt ` (TrueType hints) |
| `retain_layout_tables` / `.retain_layout_tables(bool)` | `true` | Keep `GSUB`, `GPOS`, `GDEF` |
| `retain_names` / `.retain_names(bool)` | `true` | Keep the full `name` table; `false` keeps only IDs 0–6 |
| `retain_codepoint_range` / `.retain_codepoint_range(lo, hi)` | `None` | Restrict the cmap scan to `[lo, hi]` (inclusive) |

`SubsetOptions::default()` provides the defaults above; all builder methods are `#[must_use]`.

### `SubsetStats`

| Field | Type | Description |
|-------|------|-------------|
| `original_size` | `usize` | Original font size in bytes |
| `subset_size` | `usize` | Subset font size in bytes |
| `glyphs_retained` | `u16` | Glyphs in the subset (including `.notdef`) |
| `tables_retained` | `Vec<[u8; 4]>` | 4-byte tags of all retained tables |

### `tables` module — SFNT directory read/write

| Item | Description |
|------|-------------|
| `read_table_directory(data) -> Result<HashMap<[u8;4], &[u8]>, SubsetError>` | Parse an SFNT directory (delegates to `SfntTableMap`) |
| `build_sfnt(&[([u8;4], Cow<[u8]>)]) -> Vec<u8>` | Assemble a sorted SFNT, computing offsets, checksums, and `head.checkSumAdjustment` |
| `table_checksum(data) -> u32` | OpenType table checksum (big-endian u32 word sum) |

### Per-table rewriters (public submodules)

Each module exposes the rewriter used by the pipeline; they are public so advanced callers can rewrite individual tables.

| Module | Public function(s) | Purpose |
|--------|--------------------|---------|
| `glyf` | `rewrite_glyf_loca`, `collect_composite_components` | Rebuild `glyf`+`loca`; gather composite component GIDs |
| `cmap` | `rewrite_cmap` | Build a new `cmap` from codepoint→new-GID |
| `cff` | `rewrite_cff`, `rewrite_cff2` | Subset CFF / CFF2 CharStrings |
| `colr` | `rewrite_colr` | COLR v0 base/layer GID remap (v1+ preserved) |
| `cbdt` | `rewrite_cbdt_cblc` | Paired CBDT/CBLC colour bitmap subsetting |
| `sbix` | `rewrite_sbix` | Rebuild Apple `sbix` strike arrays |
| `svg` | `rewrite_svg` | Drop SVG document index entries for removed GIDs |
| `kern` | `rewrite_kern` | Prune kerning pairs and remap GIDs |
| `os2` | `rewrite_os2`, `read_unicode_ranges` | Rewrite Unicode-range bits & first/last char |
| `math` | `rewrite_math` | MATH Coverage remapping |
| `otl` | `rewrite_gsub`, `rewrite_gsub_subtable` | GSUB GID-reference rewriting |
| `otl_gpos` | `rewrite_gpos`, `rewrite_gpos_subtable` | GPOS GID-reference rewriting |
| `layout` | `read_coverage`, `write_coverage`, `remap_coverage`, `read_classdef`, `write_classdef`, `remap_classdef`, `rewrite_gdef` | Coverage / ClassDef / GDEF helpers |
| `gvar` | `rewrite_gvar` | Per-glyph variation data rewrite |
| `varfont` | `rewrite_hvar_vvar` | HVAR / VVAR delta-set index map rewrite |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | — | No features enabled by default |
| `subset` | no | Marker feature for subsetting (no extra deps) |
| `parallel` | no | Dispatch independent table rewrites to a Rayon pool (`dep:rayon`); identical output |

## Errors

| `SubsetError` variant | Cause |
|-----------------------|-------|
| `InvalidFont(String)` | Structurally invalid font data (truncated header, malformed sub-table, …) |
| `TableMissing([u8; 4])` | A required table (`cmap`, `glyf`, `loca`, `head`, `hhea`, `hmtx`, …) is absent |
| `Io(std::io::Error)` | I/O error (file paths / tests); implements `From<std::io::Error>` |

## Cross-references

- [`oxifont-core`](../oxifont-core) — provides `SfntTableMap`, used for zero-copy directory parsing and the `subset_with_table_map` entry point
- [`oxifont-parser`](../oxifont-parser) — produces a `ParsedFace` whose `raw_bytes()` / `with_table_map` feed this crate
- [`oxifont`](../..) — the top-level façade that wires subsetting into the high-level API

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
