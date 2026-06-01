# oxifont-db TODO

## Status
In-memory indexed font database with CSS Fonts Level 4 hybrid query engine. Provides `FontDatabase` with family-name secondary index, `FaceInfo` with PostScript name/stretch/variable axes/locale families, and `Query` builder implementing CSS stretch/style/weight narrowing with fontconfig generic-alias resolution and variable-font `wght` preference. JSON disk cache behind `cache` feature. 6 source files, ~430 SLOC. Production-quality matching but missing several advanced features.

## Core Implementation
- [x] Add `ital` axis preference in variable font matching (currently only `wght` axis is checked) (~20 SLOC)
- [x] Add `wdth` (width/stretch) axis preference for variable fonts covering stretch ranges (~20 SLOC)
- [x] Implement font fallback chains: `Query::match_with_fallback(text: &str)` tries the matched font, then walks a system fallback list for uncovered codepoints (~80 SLOC)
- [x] Add codepoint coverage queries: `FaceInfo::covers_char(char) -> bool` by loading cmap lazily (~40 SLOC)
- [x] Add Unicode script coverage: determine which scripts a face supports via cmap + GSUB ScriptList (~60 SLOC)
- [x] Expand generic alias table with CJK-specific families: `sans-serif-cjk`, `serif-cjk`, plus per-locale defaults (Noto Sans CJK JP, PingFang SC, etc.) (~40 SLOC)
- [x] Add `Query::locale(bcp47)` to prefer locale-specific family names during matching (~25 SLOC)
- [x] Add `FontDatabase::remove_face(id: u32)` for dynamic font unloading (~20 SLOC)
- [x] Add `FontDatabase::face_by_id(id: u32) -> Option<&FaceInfo>` for direct ID lookup (~10 SLOC)
- [x] Add `FontDatabase::faces_by_family(name: &str) -> &[FaceInfo]` public accessor (~10 SLOC)
- [x] Implement binary-encoded cache format (replace serde_json with a compact binary format) for faster load times (~80 SLOC)
  - **Goal:** Replace JSON-based font database cache with oxicode binary format for faster cold start. (planned 2026-05-25)
  - **Design:** Add `oxicode` to workspace deps + oxifont-db Cargo.toml under `cache` feature. Implement `save_cache_binary`/`load_cache_binary` with 8-byte magic+version header (`OXDB` + u32). `system_cached()` tries binary first, falls back to JSON. Wire `cache` feature to actually enable serde_json conditionally.
  - **Files:** `Cargo.toml` (workspace), `crates/oxifont-db/Cargo.toml`, `crates/oxifont-db/src/db.rs`.
  - **Tests:** `crates/oxifont-db/tests/cache_binary.rs` gated `#[cfg(feature="cache")]`
- [x] Add cache versioning and invalidation: detect font file changes via mtime/hash (~40 SLOC)
- [x] Add `FontDatabase::load_system_fonts_async()` for non-blocking font loading (~30 SLOC) — implemented as `load_system_fonts_bg()` (thread-based via `std::thread::spawn`; true async requires an async-runtime dep and remains deferred)
- [x] Parse OS/2 unicode ranges (ulUnicodeRange1-4) for fast codepoint coverage approximation without loading cmap (~40 SLOC)

## API Improvements
- [x] Make `Query` support multiple family fallback: `.family("Helvetica").family("Arial").family("sans-serif")` chains (already accumulates, but document the semantics)
- [x] Add `Query::match_all() -> Vec<&FaceInfo>` returning all candidates in preference order
- [x] Add `Query::oblique(bool)` to distinguish oblique from italic in style matching
- [x] Implement `Display` for `FaceInfo` with a human-readable summary
- [x] Add `FontDatabase::stats() -> DbStats` returning face count, family count, cache status

## Testing
- [x] Test CSS weight ordering for edge cases: weight=400 picks 500 before 300
- [x] Test CSS weight ordering for weight=350 (below 400: nearest below first)
- [x] Test CSS weight ordering for weight=600 (above 500: nearest above first)
- [x] Test stretch narrowing: condensed preference picks condensed over normal
- [x] Test style narrowing: italic requested picks italic/oblique, falls back to normal
- [x] Test generic alias resolution: "sans-serif" expands to Arial/Helvetica/etc.
- [x] Test variable font preference: VF with wght 100-900 beats static w400 for weight=400
- [x] Test locale-specific family name lookup (ja-JP -> Japanese family name)
- [x] Test BCP-47 to LCID mapping for all entries in the static table
- [x] Test cache serialization round-trip (behind `cache` feature)
- [ ] Add fuzzing target for Query matching with random weight/stretch/style combinations
- [x] Benchmark `Query::match_best` with 5000+ faces loaded (planned 2026-05-26)
  - **Design:** Bench lives in `crates/oxifont-adapter-pure/benches/font_database_find.rs` (Slice 5 infrastructure). This item tracks the oxifont-db-specific optimization work in Slice 7.

## Performance
- [x] Add PostScript name index (HashMap) for O(1) PostScript name lookups (~15 SLOC)
- [x] Pre-sort faces by weight within each family for binary-search during weight narrowing (~20 SLOC)
- [x] Use `SmallVec` for the by_family index when families typically have <8 faces
- [x] Benchmark and optimize the CSS weight ordering sort (2026-05-27)
- [x] Profile hot-path allocation in `Query::match_best` and reduce allocations (2026-05-27)
  - **Goal:** Reduce `Query::match_best` to zero or one heap allocations per query. Target: ~30% faster throughput on 5000-face catalogs.
  - **Design:** Converted all `Vec<usize>` accumulators in `ordered_indices`, `narrow_by_stretch`, `narrow_by_style`, `css_weight_order`, and `ordered_by_weight_preference` to `SmallVec<[usize; 16]>` (inline capacity 16). Eliminated the O(n²) `result.contains(&idx)` dedup in `ordered_by_weight_preference` by replacing it with a `SmallVec<[bool; 64]>` seen-bitset (O(1) per insert). Skipped the `[Option<&FaceInfo>; 9]` slot approach as it does not preserve non-standard weight ordering (e.g., faces at 350/380 must be ranked by actual proximity, not by slot). The `[Option<&FaceInfo>; 9]` approach would silently regress correctness for non-standard weights.
  - **Files:** `src/query.rs` (SmallVec throughout + O(1) dedup), `tests/match_best_allocations.rs` (13 new tests).
  - **Tests:** Standard CSS cascade correctness, non-standard weight ordering (350/380, 250/270, 620/650), edge weights (1, 50, 100, 150, 250, 550, 850, 950, 1000 — no panic), large family (20 faces, exercises SmallVec heap spill). All 132 tests pass, zero clippy warnings.
  - **Risk:** Weight-snap boundary cases (e.g., 550). Mitigation: explicit per-weight tests added in `tests/match_best_allocations.rs`.

## Integration
- [ ] Serve as the indexed backend for oxifont facade crate's `db` feature module
- [ ] Provide font metadata to oxitext-shape for automatic font selection per script
- [ ] Feed locale-aware family names to oxitext-icu for locale-specific rendering pipelines
- [x] Coordinate cache invalidation with oxifont-discovery's mtime tracking
