# oxifont-discovery TODO

## Status
Pure Rust OS font-directory scanner. Enumerates well-known font directories for macOS, Linux, and Windows without native library dependencies. Uses `walkdir` for recursive scanning and `oxifont-parser` for parsing discovered files. ~132 SLOC. Functional but lacks parallelism, caching, and broad format support.

## Core Implementation
- [x] Add parallel directory scanning using rayon for faster enumeration on large font directories (~40 SLOC)
- [x] Support OTC (OpenType Collection) file extension in addition to TTC (~5 SLOC)
- [x] Support WOFF/WOFF2 files in system directories (some Linux distros package webfonts) (~15 SLOC)
- [x] Add `user_font_dirs()` returning user-configurable font search paths from XDG_DATA_DIRS and environment variables (~30 SLOC)
- [x] Add Android font directory support (`/system/fonts/`, `/system/font/`) (~10 SLOC)
- [x] Add FreeBSD/NetBSD font directory support (`/usr/local/share/fonts/`) (~10 SLOC)
- [x] Implement font file change detection via mtime tracking for cache invalidation (~50 SLOC)
- [x] Add `scan_file(path: &Path) -> Vec<FaceInfo>` for scanning a single file (~15 SLOC)
- [x] Add `scan_dirs_with_progress(paths, callback)` for progress reporting during large scans (~25 SLOC)
- [x] Support fontconfig configuration parsing (`/etc/fonts/fonts.conf`) for discovering additional font directories on Linux (~80 SLOC)
- [x] Add Flatpak/Snap font directory detection on Linux (`~/.var/app/*/data/fonts/`) (~15 SLOC)

## API Improvements
- [x] Return a `ScanResult` struct with `faces: Vec<FaceInfo>` and `errors: Vec<(PathBuf, FontError)>` instead of silently swallowing errors
- [x] Add `ScanOptions` builder: max_depth, follow_symlinks, file_extensions filter, parallel flag
- [x] Add `scan_dirs_async()` for non-blocking font enumeration in async contexts
- [x] Return scan statistics (total files scanned, total faces found, time elapsed, errors skipped)

## Testing
- [x] Test `system_font_dirs()` returns non-empty list on macOS CI
- [x] Test scanning a temp directory with known TTF fixtures
- [x] Test TTC collection scanning produces multiple FaceInfo entries per file
- [x] Test that symlink following works correctly (and doesn't loop)
- [x] Test handling of unreadable files (permission denied) gracefully skips them
- [x] Benchmark scan time on /System/Library/Fonts (macOS) and /usr/share/fonts (Linux)

## Performance
- [x] Use rayon's parallel iterator for filesystem traversal (~20 SLOC)
- [x] Pre-filter by file extension before reading file contents to avoid unnecessary I/O
- [x] Memory-map font files instead of `fs::read` for large font directories (~30 SLOC)
- [x] Add optional mmap feature behind a cargo feature gate

## Integration
- [x] Feed scan results into oxifont-db for indexed querying — done via oxifont facade `system_fonts()` which builds FontDatabase from scan results
- [x] Coordinate with oxifont-adapter-pure to share the same scan pipeline (done 2026-05-27)
  - **Goal:** Add `scan_dirs_metadata_only(paths: &[PathBuf]) -> ScanResult` that reads only SFNT header + `name`/`OS/2`/`cmap` tables per font file (via `File::seek` + `read_exact`), populating all `FaceInfo` fields including `unicode_ranges` from cmap. Eager `scan_dirs` and lazy `scan_dirs_metadata_only` share `read_face_metadata_partial`.
  - **Design:** New `src/sfnt_partial.rs` (~100 lines) — reads SFNT table directory entries, seeks to `name`/`OS/2`/`cmap` offsets, reads those 3 tables into `Vec<u8>` buffers, feeds them through extracted pure parsing helpers. `scan_dirs_metadata_only` iterates files, calls `read_face_metadata_partial`, skips on error (same as eager path). Adapter-pure's `scan_lazy` calls this function.
  - **Files:** `src/lib.rs` (add `scan_dirs_metadata_only`, `read_face_metadata_partial`), `src/sfnt_partial.rs` (new, ~100 lines).
  - **Tests:** `tests/discovery.rs` extension — `scan_dirs_metadata_only` on temp dir with fixture TTF returns `FaceInfo` with non-empty family and populated `unicode_ranges`; re-scan after file modification reflects changes.
  - **Risk:** Some fonts have name/OS2/cmap tables at unusual offsets — seek-based read handles this; directory walk is authoritative.
- [x] Provide scan progress events for oxifont facade crate UI integration — `scan_dirs_with_progress()` already implemented
