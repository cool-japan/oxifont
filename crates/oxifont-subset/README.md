# oxifont-subset — Pure-Rust OpenType font subsetter for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-subset.svg)](https://crates.io/crates/oxifont-subset)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-subset` is the subsetting layer of the OxiFont family. Given raw SFNT font bytes and a set of Unicode codepoints (or glyph IDs), it produces a new, minimal SFNT containing only the requested glyphs — plus `.notdef` and any transitively-referenced composite components — and rewrites every affected table so the result is a valid, standalone font.

The subsetter handles both TrueType (`glyf`/`loca`) and CFF/CFF2 outline formats, remaps glyph IDs to a dense space starting at 0, and rewrites the full table set: `glyf`, `loca`, `cmap`, `hmtx`/`vmtx`, `maxp`, `head`, `hhea`/`vhea`, `post`, `name`, layout tables (GSUB/GPOS/GDEF), `kern`, `OS/2`, variation tables (`gvar`, HVAR/VVAR), and colour tables (COLR, CPAL, CBDT/CBLC, sbix, SVG), plus MATH. Alongside subsetting, [`instance()`](#static-instancing) pins a variable face at one design location and produces a static font — the outlines and metrics evaluated at that location, with every variation table removed and glyph IDs untouched. It is `#![forbid(unsafe_code)]` and 100% Pure Rust. With the optional `parallel` feature the heavy independent table rewrites are dispatched to a Rayon thread pool; output is bit-for-bit identical to the sequential path.

## Installation

```toml
[dependencies]
oxifont-subset = "0.2.2"
```

With parallel table rewriting:

```toml
[dependencies]
oxifont-subset = { version = "0.2.2", features = ["parallel"] }
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
    .strip_hints(true)       // drop fpgm/prep/cvt
    .retain_names(false)     // keep only name IDs 0–6
    .drop_variations(true);  // drop fvar/avar/gvar/cvar/HVAR/VVAR/MVAR/STAT

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
| `subset_with_table_map(map, gid_set, cp_to_old_gid, opts) -> Result<(Vec<u8>, SubsetStats), _>` | As above but reuses a pre-parsed `oxifont_core::sfnt::SfntTableMap` (skips a second directory walk); a map from `parse_face` / `parse_at_offset` subsets that face of a collection |

`.notdef` (GID 0) is always retained implicitly, and the composite-component closure is always applied for TrueType fonts.

### Entry-point suffixes: `_at_face` and `_mapped`

Two orthogonal suffixes extend the entry points above.

| Suffix | Effect |
|--------|--------|
| `_at_face` | Takes a `face_index: u32` after `font_data`, selecting a face out of a `ttcf` collection — the form every stock Windows CJK family ships in (`msgothic.ttc`, `meiryo.ttc`, `YuGothM.ttc`, `msyh.ttc`, `msjh.ttc`, `simsun.ttc`). For a plain TTF/OTF the only valid index is `0` and the output is byte-identical to the base entry point. |
| `_mapped` | Returns a `SubsetGidMap` as an extra tuple element, recovering the old ↔ new glyph-ID renumbering the subset performed. |

| Function | Description |
|----------|-------------|
| `face_count(font_data) -> Result<u32, SubsetError>` | `1` for a plain TTF/OTF, `numFonts` for a `ttcf` collection; the valid `face_index` range |
| `subset_font_at_face(font_data, face_index, codepoints)` | `subset_font` for one face of a collection |
| `subset_font_with_options_at_face(font_data, face_index, codepoints, opts)` | `subset_font_with_options` for one face |
| `subset_by_gids_at_face(font_data, face_index, gids)` | `subset_by_gids` for one face |
| `subset_with_gid_set_at_face(font_data, face_index, old_gid_set, cp_to_old_gid, opts)` | Core engine for one face |
| `subset_font_with_options_mapped(font_data, codepoints, opts)` | …`+ SubsetGidMap` |
| `subset_by_gids_mapped(font_data, gids)` | …`+ SubsetGidMap` |
| `subset_with_gid_set_mapped(font_data, old_gid_set, cp_to_old_gid, opts)` | …`+ SubsetGidMap` |
| `subset_with_table_map_mapped(map, gid_set, cp_to_old_gid, opts)` | …`+ SubsetGidMap` |
| `subset_with_gid_set_at_face_mapped(font_data, face_index, old_gid_set, cp_to_old_gid, opts)` | Fully general: face selection *and* the map |

The base entry points **reject** a `ttcf` container (`SubsetError::InvalidFont`, "bad SFNT magic") rather than silently subsetting face 0 — a collection's faces are different fonts, so which one to embed is the caller's decision. A face index at or beyond `face_count` is `SubsetError::FaceIndexOutOfRange`, never a panic, and a collection header that is truncated, versioned unknown, declares zero or an unrepresentable `numFonts`, or points a face at bytes that are not an SFNT header is refused before any of it is trusted.

### `SubsetGidMap` — recovering the glyph renumbering

The subsetter renumbers retained glyphs densely from 0, in ascending old-GID order, *after* the composite-component closure has run — so the new IDs cannot be predicted from the requested glyph set alone. A PDF CIDFont embedded with `Identity-H` and `/CIDToGIDMap /Identity` has to emit exactly those new IDs as CIDs; `SubsetGidMap` is how you get them.

| Method | Description |
|--------|-------------|
| `.new_gid(old_gid) -> Option<u16>` | Subset GID (= CID under Identity) for an original GID |
| `.old_gid(new_gid) -> Option<u16>` | Original GID for a subset GID |
| `.contains_old_gid(old_gid) -> bool` | Whether an original glyph survived |
| `.new_to_old() -> &[u16]` | The whole assignment indexed by new GID; its length is the subset's glyph count |
| `.len()` / `.is_empty()` | Number of mapped glyphs (always ≥ 1: `.notdef`) |
| `.iter() -> impl Iterator<Item = (u16, u16)>` | `(old, new)` pairs in ascending old-GID order |

```rust,no_run
use std::collections::BTreeSet;
use oxifont_subset::subset_by_gids_mapped;

let font_data = std::fs::read("NotoSans-Regular.ttf")?;
let requested: BTreeSet<u16> = [42u16, 7].into_iter().collect();

let (subset, _stats, gid_map) = subset_by_gids_mapped(&font_data, &requested)?;

// The CID to write for the glyph that was old GID 42.
let cid = gid_map.new_gid(42).expect("requested glyphs are always mapped");
# let _ = (subset, cid);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Static instancing

`instance(font_data, face_index, coords) -> Result<Vec<u8>, SubsetError>` evaluates a `glyf`-flavoured
variable face at one **fully pinned** location and returns a complete static SFNT.

- `coords` is `(axis tag, value)` in the same **user** units `fvar` records (`wght = 700.0`,
  `wdth = 87.5`), not normalised F2Dot14. Values are clamped to each axis's `[min, max]`; an axis
  absent from the list pins at its `fvar` default; a tag that names no axis is
  `SubsetError::UnknownAxis`, because silently ignoring a typo would embed the default instance while
  reporting success.
- `font_data` may be a `ttcf` collection, selected by `face_index` exactly as `subset_font_at_face`
  does. The result is always a single-face SFNT at offset 0.
- **Glyph IDs do not move.** Same glyph count, same order — so `cmap`, `GSUB`, `GPOS`, `GDEF`, `kern`,
  `COLR`, `MATH` and `sbix` are carried over verbatim, and any glyph ID you already hold stays valid.
- Dropped: `fvar`, `avar`, `gvar`, `cvar`, `HVAR`, `VVAR`, `MVAR`, `STAT` (the face is no longer
  variable), `DSIG` (a signature over rewritten bytes), and `cvt `/`fpgm`/`prep`/`gasp` together with
  every per-glyph instruction stream — hint programs tuned against the default master mis-grid-fit an
  instanced outline, and leaving them without their `cvt `/`fpgm` is worse still.
- Rebuilt: `glyf`, `loca`, `hmtx`/`vmtx` (from the four phantom points, so an empty glyph whose
  advance varies is covered too), `hhea`/`vhea`, `head`, `maxp`, and `OS/2`
  `usWeightClass`/`usWidthClass`/`fsSelection`, `head.macStyle` and `post.italicAngle` from the pinned
  location.
- The function is a pure, byte-deterministic function of its inputs.

Because glyph IDs are preserved, instancing and subsetting compose by simply running one after the
other — the instanced bytes go straight into any entry point, at `face_index = 0`:

```rust,no_run
use std::collections::{BTreeMap, BTreeSet};
use oxifont_subset::{instance, subset_with_gid_set_at_face_mapped, SubsetOptions};

let font_data = std::fs::read("SegUIVar.ttf")?;

// 1. Pin the design location. Glyph IDs are unchanged, so a gid set collected
//    against the original face is still valid against the result.
let static_bytes = instance(&font_data, 0, &[(*b"wght", 700.0), (*b"opsz", 10.5)])?;

// 2. Subset the static bytes as usual. `drop_variations` is a no-op here — the
//    instancer already removed every variation table.
let gids: BTreeSet<u16> = [0u16, 36, 37, 68].into_iter().collect();
let opts = SubsetOptions::default()
    .retain_layout_tables(false)
    .retain_names(false);
let (bytes, stats, gid_map) =
    subset_with_gid_set_at_face_mapped(&static_bytes, 0, &gids, &BTreeMap::new(), &opts)?;
# let _ = (bytes, stats, gid_map);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Without step 1, a subset of a variable face embeds its **default master** while still advertising
axes; `SubsetOptions::drop_variations` removes the axes but cannot change which master the outlines
are. Instancing is the only way to embed a location that is not the default.

### `SubsetOptions` (builder)

| Field / method | Default | Description |
|----------------|---------|-------------|
| `strip_hints` / `.strip_hints(bool)` | `false` | Drop `fpgm`, `prep`, `cvt ` (TrueType hints) |
| `retain_layout_tables` / `.retain_layout_tables(bool)` | `true` | Keep `GSUB`, `GPOS`, `GDEF` |
| `retain_names` / `.retain_names(bool)` | `true` | Keep the full `name` table; `false` keeps only IDs 0–6 |
| `retain_codepoint_range` / `.retain_codepoint_range(lo, hi)` | `None` | Restrict the cmap scan to `[lo, hi]` (inclusive) |
| `drop_variations` / `.drop_variations(bool)` | `false` | Drop `fvar`, `avar`, `gvar`, `cvar`, `HVAR`, `VVAR`, `MVAR`, `STAT` — the retained outlines are then the source's *default master* |

`SubsetOptions::default()` provides the defaults above; all builder methods are `#[must_use]`. The struct is `#[non_exhaustive]`: build it from `default()` plus the builder methods rather than a struct literal.

### `SubsetStats`

| Field | Type | Description |
|-------|------|-------------|
| `original_size` | `usize` | Original font size in bytes |
| `subset_size` | `usize` | Subset font size in bytes |
| `glyphs_retained` | `u16` | Glyphs in the subset (including `.notdef`) |
| `tables_retained` | `Vec<[u8; 4]>` | 4-byte tags of all retained tables |
| `dropped_context_subtables` | `usize` | Advanced GSUB/GPOS subtables dropped as malformed or unmatchable under the subset |
| `cff_charstrings_verbatim` | `bool` | The `CFF `/`CFF2` charstrings were copied from the source instead of subset (CID-keyed or unparseable). The table is then correct only under the *original* glyph numbering — embed the original face or refuse. Always `false` for `glyf` outlines |

### `tables` module — SFNT directory read/write

| Item | Description |
|------|-------------|
| `read_table_directory(data) -> Result<HashMap<[u8;4], &[u8]>, SubsetError>` | Parse an SFNT directory at offset 0 (delegates to `SfntTableMap::parse`; refuses `ttcf`) |
| `read_table_directory_at_face(data, face_index) -> Result<HashMap<[u8;4], &[u8]>, SubsetError>` | As above for one face of a `ttcf` collection (delegates to `SfntTableMap::parse_face`) |
| `build_sfnt(&[([u8;4], Cow<[u8]>)]) -> Vec<u8>` | Assemble a sorted SFNT, computing the sfnt version from the outline table present, spec-conformant search fields, offsets, checksums, and `head.checkSumAdjustment` |
| `table_checksum(data) -> u32` | OpenType table checksum (big-endian u32 word sum) |
| `SFNT_VERSION_TRUETYPE` / `SFNT_VERSION_CFF` | `0x00010000` / `OTTO` — the two flavours `build_sfnt` emits |

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

### `pdf_subset` module — streaming accumulator for PDF/CID pipelines

`PdfFontSubsetter` accumulates codepoints and/or raw GIDs across multiple text-placement calls (e.g. while composing PDF pages) and produces one minimal subset on `finalize`, so a multi-page document is subset once instead of per page. It is not `Sync`; use `.merge()` to combine per-thread accumulators before finalizing.

| Method | Description |
|--------|-------------|
| `PdfFontSubsetter::new(font_data, opts)` | New accumulator with explicit `SubsetOptions` |
| `PdfFontSubsetter::new_at_face(font_data, face_index, opts)` | As above, subsetting one face of a `ttcf` collection |
| `PdfFontSubsetter::for_pdf(font_data)` / `::for_pdf_at_face(font_data, face_index)` | Preset matching `subset_font_for_pdf` |
| `PdfFontSubsetter::for_web(font_data)` / `::for_web_at_face(font_data, face_index)` | Preset matching `subset_font_for_web` |
| `.face_index() -> u32` | The face this accumulator will subset |
| `.add_codepoint(char)` / `.add_text(&str)` | Accumulate Unicode codepoints (resolved via `cmap` at `finalize`) |
| `.add_gid(u16)` / `.add_gids(&[u16])` | Accumulate raw GIDs directly, bypassing `cmap` (PDF Type3/CID workflows) |
| `.codepoint_count()` / `.gid_count()` / `.is_empty()` | Inspect accumulated state |
| `.codepoints() -> &BTreeSet<char>` / `.raw_gids() -> &BTreeSet<u16>` | Borrow the accumulated sets |
| `.merge(&mut other)` | Fold another accumulator's codepoints/GIDs into `self`, resetting `other` |
| `.finalize() -> Result<(Vec<u8>, SubsetStats), SubsetError>` | Resolve accumulated state through the standard subsetting pipeline |
| `.finalize_mapped() -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError>` | `finalize` plus the old ↔ new glyph-ID map (the CIDs to emit) |
| `.finalize_into_result() -> Result<PdfSubsetResult, SubsetError>` | `finalize_mapped`, wrapped into a `{ bytes, stats, gid_map }` struct |
| `.into_finalized() -> Result<(Vec<u8>, Vec<u8>, SubsetStats), SubsetError>` | Consumes `self`; returns `(original_font_data, subset_bytes, stats)` |
| `.reset()` | Clear accumulated codepoints/GIDs, keeping `font_data`/`opts` for reuse |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | — | No features enabled by default |
| `subset` | no | Marker feature for subsetting (no extra deps) |
| `parallel` | no | Dispatch independent table rewrites to a Rayon pool (`dep:rayon`); identical output |

## Errors

| `SubsetError` variant | Cause |
|-----------------------|-------|
| `InvalidFont(String)` | Structurally invalid font data (truncated header, malformed sub-table, a `ttcf` container handed to a non-`_at_face` entry point, …), or a requested subset whose format-4 `cmap` sub-table would exceed the ~8189-segment addressable size |
| `TableMissing([u8; 4])` | A required table (`cmap`, `glyf`, `loca`, `head`, `hhea`, `hmtx`, …) is absent |
| `FaceIndexOutOfRange { index, count }` | A `face_index` at or beyond `face_count(data)` |
| `UnknownAxis([u8; 4])` | A tag passed to `instance()` names no `fvar` axis |
| `Unsupported(&'static str)` | `instance()` was handed a face with no `fvar` axes, or one with `CFF`/`CFF2` outlines |
| `Io(std::io::Error)` | I/O error (file paths / tests); implements `From<std::io::Error>` |

## Cross-references

- [`oxifont-core`](../oxifont-core) — provides `SfntTableMap`, used for zero-copy directory parsing and the `subset_with_table_map` entry point
- [`oxifont-parser`](../oxifont-parser) — produces a `ParsedFace` whose `raw_bytes()` / `with_table_map` feed this crate
- [`oxifont`](../..) — the top-level façade that wires subsetting into the high-level API

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
