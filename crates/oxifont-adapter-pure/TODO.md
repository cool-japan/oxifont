# oxifont-adapter-pure TODO

## Status
Composes `oxifont-discovery` and `oxifont-parser` into a `FontCatalog` implementation requiring no native libraries. Provides `FontDatabase` with `system()`, `scan()`, and `from_faces()` constructors plus `load_face()`. CSS Fonts Level 4 matching via `find_css()`. Generic family resolution (sans-serif/serif/monospace/cursive/fantasy). ~290 SLOC. Full CSS §4.5 matching + test coverage.

## Core Implementation
- [x] Add font fallback chain support: `find_with_fallback(families, base_query, text) -> Option<&FaceInfo>` that tries multiple families until a face is found (~60 SLOC). `text` is accepted but cmap coverage check is deferred (FaceInfo has no unicode_ranges; per-file loading is too expensive in hot paths).
- [x] Implement CSS Fonts Level 4 matching (stretch/style/weight priority) instead of simple linear scan (~80 SLOC)
- [x] Add `FontDatabase::add_dir(path)` for incrementally adding font directories after construction (~15 SLOC)
- [x] Add `FontDatabase::add_bytes(data: Vec<u8>)` for loading in-memory fonts (~20 SLOC)
- [x] Add `FontDatabase::remove(path)` for removing fonts from the catalog (~15 SLOC)
- [x] Implement caching: store FaceInfo to disk and reload on next startup if mtime unchanged (~60 SLOC). Implemented via `feature = "cache"` using `serde_json` (FaceInfo has `serde` derives behind `oxifont-core/serde` feature). `scan_cached`/`system_cached` methods added. Cache pruning and atomic writes included.
- [x] Add `find_all(query) -> Vec<&FaceInfo>` returning all matching faces instead of just the first (~10 SLOC)
- [x] Add generic family resolution (sans-serif/serif/monospace -> concrete families) (~30 SLOC)

## API Improvements
- [x] Implement `IntoIterator` for `&FontDatabase` to iterate over all faces
- [x] Add `Debug` derive for `FontDatabase`
- [x] Add `FontDatabase::len()` and `FontDatabase::is_empty()` convenience methods
- [x] Add `FontDatabase::merge(other: FontDatabase)` for combining catalogs
- [x] Add `FontDatabase::from_faces(faces: Vec<FaceInfo>)` for programmatic construction

## Testing
- [x] Test `system()` returns a non-empty catalog on CI machines with system fonts
- [x] Test `scan()` with a fixture directory containing known fonts
- [x] Test `find()` correctly matches family name (case-insensitive substring)
- [x] Test `find()` correctly matches style and weight filters
- [x] Test `find_css` case-insensitive family matching (`test_find_css_case_insensitive_family`)
- [x] Test `find_with_fallback` multiple families: first absent → second returned (`test_find_css_multiple_families`)
- [x] Test `find_best_for_text` delegates to CSS matching and generic resolution
- [x] Test `load_face()` successfully parses the face pointed to by FaceInfo
- [x] Test empty scan directory produces empty (not error) catalog
- [x] Test `scan_cached`: cold start, cache hit consistency, stale-entry pruning, empty-dir handling

## Performance
- [x] Build a family-name index (HashMap) for O(1) family lookup instead of O(n) linear scan (~30 SLOC)
- [x] Benchmark scan time and `FontDatabase::find` with 5000+ faces (planned 2026-05-26)
  - **Design:** `benches/font_database_find.rs` — build synthetic `FontDatabase` with 5000 faces (varying family/weight); bench `find(&query)` for exact-hit and miss; bench 5000-face construction. Requires `criterion.workspace = true` in `[dev-dependencies]` + `[[bench]] name = "font_database_find" harness = false`. Workspace criterion dep added by Slice 5.
- [x] Evaluate lazy font loading: only parse metadata on first query (done 2026-05-27)
  - **Goal:** `FontDatabase::system_lazy()` returns immediately with `FaceInfo` metadata (no full parse), deferring `ParsedFace::parse` to `load_face(info)` call time. Cuts cold-start memory + parse time on systems with hundreds of fonts.
  - **Design:** New `FontDatabase::system_lazy() -> Result<Self, AdapterError>` and `scan_lazy(dirs: &[PathBuf])` that call `oxifont_discovery::scan_dirs_metadata_only` (partial SFNT reader: header + name/OS2/cmap tables only, populates unicode_ranges). `FontDatabase` gains `was_lazy: bool`. `load_face(info: &FaceInfo) -> Result<ParsedFace, AdapterError>` reads `std::fs::read(&info.path)?` and parses on demand.
  - **Files:** `src/lib.rs` (add `system_lazy`, `scan_lazy`, `load_face`).
  - **Tests:** `tests/lazy_loading.rs` (new) — `system_lazy()` returns db; `load_face(&info)` returns `ParsedFace`; metadata fields (family, style, weight, stretch) match eager-scan output.
  - **Risk:** Lazy db has same FaceInfo fields as eager db — unicode_ranges populated via cmap read in discovery.

## Integration
- [x] Serve as the default font backend for oxitext's `Pipeline::new(font_db)` — `oxitext::Pipeline::new(&oxifont::FontDatabase)` already implemented; `pipeline_integration_pattern_via_font_bytes` test in `tests/subset_integration.rs` verifies the pattern.
- [x] Provide font data access for oxifont-subset operations — `font_bytes()` always available; `subset_face()` and `subset_face_for_web()` added behind `feature = "subset"` (wraps `oxifont_subset::subset_font` / `subset_font_for_web`). Tests in `tests/subset_integration.rs`.
- [x] Bridge to oxifont-db for CSS Level 4 queries when the `db` feature is enabled — `into_db()` / `as_db()` implemented behind `feature = "db"`. Tests in `tests/integration.rs`.
