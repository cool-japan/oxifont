#![cfg_attr(not(feature = "mmap"), forbid(unsafe_code))]
#![warn(missing_docs)]

//! `oxifont-discovery` — Pure Rust OS font-directory scanner.
//!
//! Locates well-known font directories for Linux, macOS, and Windows without
//! calling `fontconfig`, `freetype`, or any native library. The scan walks
//! each directory with [`walkdir`] and parses discovered files with
//! [`oxifont_parser`].
//!
//! # Example
//! ```no_run
//! let dirs  = oxifont_discovery::system_font_dirs();
//! let faces = oxifont_discovery::scan_dirs(&dirs);
//! println!("found {} faces", faces.len());
//! ```

use std::path::{Path, PathBuf};

use oxifont_core::FontFace as _;
use oxifont_core::{FaceInfo, FontError};
use oxifont_parser::{face_count, ParsedFace};
use walkdir::WalkDir;

#[cfg(feature = "fontconfig")]
pub mod fontconfig;

pub(crate) mod sfnt_partial;

// ---------------------------------------------------------------------------
// Font format helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the extension (already lower-cased) is a parseable SFNT
/// format (TTF, OTF, TTC, or OTC).
fn is_sfnt_ext(ext: &str) -> bool {
    matches!(ext, "ttf" | "otf" | "ttc" | "otc")
}

/// Returns `true` if the extension is WOFF1.
fn is_woff1_ext(ext: &str) -> bool {
    ext == "woff"
}

/// Returns `true` if the extension is WOFF2.
fn is_woff2_ext(ext: &str) -> bool {
    ext == "woff2"
}

// ---------------------------------------------------------------------------
// Per-file parsing helper (SFNT path)
// ---------------------------------------------------------------------------

/// Parse all faces from an already-read byte buffer at `path`.
///
/// On success, appends one [`FaceInfo`] per sub-face to `out`.
/// On failure, returns the error message.
fn parse_sfnt_faces(
    path: &Path,
    bytes: std::sync::Arc<[u8]>,
    out: &mut Vec<FaceInfo>,
) -> Result<(), String> {
    let count = face_count(&bytes);
    for idx in 0..count {
        match ParsedFace::parse(bytes.clone(), idx) {
            Ok(face) => {
                let ps_name = face.postscript_name().unwrap_or_default().to_string();
                out.push(FaceInfo {
                    family: std::sync::Arc::from(face.family_name()),
                    post_script_name: ps_name,
                    style: face.style(),
                    weight: face.weight(),
                    stretch: face.stretch(),
                    path: path.to_path_buf(),
                    face_index: idx,
                    localized_families: Vec::new(),
                });
            }
            Err(e) => {
                // Sub-face parse error — report but continue if there are
                // other faces in the collection.
                if count == 1 {
                    return Err(e.to_string());
                }
                // For collections, skip the bad sub-face silently.
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// scan_file
// ---------------------------------------------------------------------------

/// Scan a single font file and return all faces it contains.
///
/// Handles TTF and OTF (one face each), TTC and OTC collections (multiple
/// faces), and — when the `woff1` / `woff2` features are enabled — WOFF1
/// and WOFF2 files (decoded to SFNT before parsing).
///
/// When a WOFF file is encountered but the corresponding feature is not
/// enabled, a single placeholder [`FaceInfo`] with an empty family name is
/// returned so callers can still discover the file's presence.
///
/// # Errors
/// Returns [`FontError::IoError`] if the file cannot be read, or
/// [`FontError::UnsupportedFormat`] for unrecognised extensions.
pub fn scan_file(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if is_sfnt_ext(&ext) {
        let bytes = std::fs::read(path)?;
        let arc: std::sync::Arc<[u8]> = bytes.into();
        let mut faces = Vec::new();
        parse_sfnt_faces(path, arc, &mut faces).map_err(FontError::ParseError)?;
        return Ok(faces);
    }

    if is_woff1_ext(&ext) {
        return scan_woff1_file(path);
    }

    if is_woff2_ext(&ext) {
        return scan_woff2_file(path);
    }

    Err(FontError::UnsupportedFormat)
}

/// Scan a WOFF1 file.
///
/// When the `woff1` feature is enabled the file is decoded to SFNT and all
/// faces are parsed. Otherwise a single placeholder [`FaceInfo`] (path only)
/// is returned.
#[cfg(feature = "woff1")]
fn scan_woff1_file(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let raw = std::fs::read(path)?;
    let sfnt =
        oxifont_webfont::decode_woff1(&raw).map_err(|e| FontError::ParseError(e.to_string()))?;
    let arc: std::sync::Arc<[u8]> = sfnt.into();
    let mut faces = Vec::new();
    parse_sfnt_faces(path, arc, &mut faces).map_err(FontError::ParseError)?;
    Ok(faces)
}

/// WOFF1 placeholder path — the `woff1` feature is not compiled in.
#[cfg(not(feature = "woff1"))]
fn scan_woff1_file(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    Ok(vec![woff_placeholder(path)])
}

/// Scan a WOFF2 file.
///
/// When the `woff2` feature is enabled the file is decoded to SFNT and all
/// faces are parsed. Otherwise a single placeholder [`FaceInfo`] (path only)
/// is returned.
#[cfg(feature = "woff2")]
fn scan_woff2_file(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let raw = std::fs::read(path)?;
    let sfnt =
        oxifont_webfont::decode_woff2(&raw).map_err(|e| FontError::ParseError(e.to_string()))?;
    let arc: std::sync::Arc<[u8]> = sfnt.into();
    let mut faces = Vec::new();
    parse_sfnt_faces(path, arc, &mut faces).map_err(FontError::ParseError)?;
    Ok(faces)
}

/// WOFF2 placeholder path — the `woff2` feature is not compiled in.
#[cfg(not(feature = "woff2"))]
fn scan_woff2_file(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    Ok(vec![woff_placeholder(path)])
}

/// Build a minimal placeholder [`FaceInfo`] for a WOFF file that could not be
/// decoded (feature not compiled in).
#[cfg(any(not(feature = "woff1"), not(feature = "woff2")))]
fn woff_placeholder(path: &Path) -> FaceInfo {
    use oxifont_core::{FontStretch, FontStyle};
    FaceInfo {
        family: std::sync::Arc::from(""),
        post_script_name: String::new(),
        style: FontStyle::Normal,
        weight: 400,
        stretch: FontStretch::Normal,
        path: path.to_path_buf(),
        face_index: 0,
        localized_families: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// ScanOptions
// ---------------------------------------------------------------------------

/// Options that control directory scanning behaviour.
///
/// Use the builder methods to configure and then pass to
/// [`scan_dirs_with_options`].
///
/// # Example
/// ```
/// let opts = oxifont_discovery::ScanOptions::default()
///     .include_woff(false)
///     .max_depth(3);
/// ```
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Whether to include WOFF1 (`.woff`) files in the scan.
    pub include_woff: bool,
    /// Whether to include WOFF2 (`.woff2`) files in the scan.
    pub include_woff2: bool,
    /// Whether to follow symbolic links during the walk.
    pub follow_symlinks: bool,
    /// Optional maximum directory depth for the walk (`None` = unlimited).
    pub max_depth: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_woff: true,
            include_woff2: true,
            follow_symlinks: true,
            max_depth: None,
        }
    }
}

impl ScanOptions {
    /// Set whether WOFF1 files are included.
    pub fn include_woff(mut self, v: bool) -> Self {
        self.include_woff = v;
        self
    }

    /// Set whether WOFF2 files are included.
    pub fn include_woff2(mut self, v: bool) -> Self {
        self.include_woff2 = v;
        self
    }

    /// Set whether symbolic links are followed during directory walks.
    pub fn follow_symlinks(mut self, v: bool) -> Self {
        self.follow_symlinks = v;
        self
    }

    /// Set the maximum recursion depth for directory walks.
    pub fn max_depth(mut self, d: usize) -> Self {
        self.max_depth = Some(d);
        self
    }

    /// Returns `true` if the lower-cased file extension should be processed
    /// given these options.
    fn accepts_ext(&self, ext: &str) -> bool {
        if is_sfnt_ext(ext) {
            return true;
        }
        if is_woff1_ext(ext) && self.include_woff {
            return true;
        }
        if is_woff2_ext(ext) && self.include_woff2 {
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// ScanResult
// ---------------------------------------------------------------------------

/// The result of a directory scan that captures both successes and failures.
///
/// Returned by [`scan_dirs_reporting`].
pub struct ScanResult {
    /// All successfully parsed font faces.
    pub faces: Vec<FaceInfo>,
    /// Files that were found but could not be parsed, with error messages.
    pub errors: Vec<(PathBuf, String)>,
    /// Total number of font-extension files touched (parsed or not).
    pub files_scanned: usize,
    /// Wall-clock time elapsed during the scan.
    pub elapsed: std::time::Duration,
}

impl ScanResult {
    /// Returns the total number of files that failed to parse.
    ///
    /// This is equivalent to `self.errors.len()` but provided as a named
    /// accessor for symmetry with [`Self::files_scanned`].
    pub fn total_errors(&self) -> usize {
        self.errors.len()
    }
}

// ---------------------------------------------------------------------------
// Public scan API
// ---------------------------------------------------------------------------

/// Returns platform-specific system font search directories.
///
/// The list is empty on platforms where none of the probed paths exist (e.g.
/// a CI container without system fonts). Never panics or touches the
/// filesystem — it only builds `PathBuf`s.
///
/// # Platform behaviour
/// - **macOS**: `/System/Library/Fonts`, `/Library/Fonts`.
/// - **Linux**: `/usr/share/fonts`, `/usr/local/share/fonts`, XDG data home.
///   Also includes Flatpak (`/var/lib/flatpak/exports/share/fonts`) and
///   Snap (`/snap/gtk-common-themes/current/share/fonts`) paths.
/// - **FreeBSD / NetBSD / OpenBSD**: `/usr/local/share/fonts`,
///   `/usr/share/fonts`, `/usr/X11R7/lib/X11/fonts`, `~/.fonts`.
/// - **Windows**: `%WINDIR%\Fonts`.
/// - **Android**: `/system/fonts`, `/data/fonts`.
///
/// User-level directories are returned by [`user_font_dirs`].
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
    }

    #[cfg(target_os = "linux")]
    {
        // When the fontconfig feature is enabled, prefer paths from fonts.conf.
        #[cfg(feature = "fontconfig")]
        {
            let fc_dirs = fontconfig::fontconfig_font_dirs();
            if !fc_dirs.is_empty() {
                return fc_dirs;
            }
        }

        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(xdg).join("fonts"));
        }
        // Flatpak system-wide export path
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/fonts"));
        // Flatpak per-user export path
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join(".local/share/flatpak/exports/share/fonts"));
        }
        // Snap gtk-common-themes
        dirs.push(PathBuf::from("/snap/gtk-common-themes/current/share/fonts"));
    }

    #[cfg(any(target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
    {
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        dirs.push(PathBuf::from("/usr/share/fonts"));
        // X11R7 fonts path common on NetBSD/pkgsrc systems
        dirs.push(PathBuf::from("/usr/X11R7/lib/X11/fonts"));
        // User-level .fonts directory (BSD convention mirrors Linux)
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".fonts"));
        }
    }

    #[cfg(target_os = "android")]
    {
        dirs.push(PathBuf::from("/system/fonts"));
        dirs.push(PathBuf::from("/data/fonts"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
    }

    dirs
}

/// Returns user-level font directories for the current platform.
///
/// These complement the system directories returned by [`system_font_dirs`].
///
/// | Platform | Paths |
/// |----------|-------|
/// | macOS    | `~/Library/Fonts` |
/// | Linux    | `~/.fonts`, `~/.local/share/fonts` |
/// | Windows  | `%LOCALAPPDATA%\Microsoft\Windows\Fonts` |
///
/// Returns an empty `Vec` if the home directory cannot be determined from
/// environment variables.
pub fn user_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join(".fonts"));
            dirs.push(PathBuf::from(&home).join(".local/share/fonts"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(localapp) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(localapp).join("Microsoft\\Windows\\Fonts"));
        }
    }

    dirs
}

/// Recursively scans `paths` for font files using default options.
///
/// Accepted extensions: `.ttf`, `.otf`, `.ttc`, `.otc`, `.woff`, `.woff2`.
///
/// Each discovered file is parsed; malformed files and parse errors are
/// silently skipped. TTC/OTC collections yield one [`FaceInfo`] per sub-face.
///
/// This is a convenience wrapper around [`scan_dirs_with_options`] with
/// [`ScanOptions::default`].
pub fn scan_dirs(paths: &[impl AsRef<Path>]) -> Vec<FaceInfo> {
    let paths_buf: Vec<PathBuf> = paths.iter().map(|p| p.as_ref().to_path_buf()).collect();
    scan_dirs_with_options(&paths_buf, &ScanOptions::default())
}

/// Recursively scans `paths` for font files, honouring the given [`ScanOptions`].
///
/// Each successfully parsed file contributes one [`FaceInfo`] per font face.
/// Files that fail to parse are silently dropped; use [`scan_dirs_reporting`]
/// to capture those errors.
pub fn scan_dirs_with_options(paths: &[PathBuf], opts: &ScanOptions) -> Vec<FaceInfo> {
    let mut results: Vec<FaceInfo> = Vec::new();

    for base in paths {
        let mut walker = WalkDir::new(base).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            // Use scan_file to unify SFNT and WOFF handling.
            if let Ok(faces) = scan_file(path) {
                results.extend(faces);
            }
        }
    }

    results
}

/// Recursively scans `paths` for font files reading only the `name`, `OS/2`,
/// and `cmap` tables from each file — much faster than a full parse when fonts
/// contain large `glyf`/`loca`/`hmtx` tables.
///
/// Populates all [`FaceInfo`] fields that those three tables provide (family,
/// PostScript name, style, weight, stretch). Fields that require other tables
/// (e.g. axis information from `fvar`) are left at their zero values.
///
/// Malformed or unreadable files are recorded in [`ScanResult::errors`].
/// WOFF/WOFF2 files are handled via a full parse (decoding is always needed).
///
/// # Example
/// ```no_run
/// let dirs = oxifont_discovery::system_font_dirs();
/// let result = oxifont_discovery::scan_dirs_metadata_only(&dirs);
/// println!("found {} faces in {}ms", result.faces.len(), result.elapsed.as_millis());
/// ```
pub fn scan_dirs_metadata_only(paths: &[PathBuf]) -> ScanResult {
    let start = std::time::Instant::now();
    let opts = ScanOptions::default();
    let mut faces: Vec<FaceInfo> = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();
    let mut files_scanned: usize = 0;

    for base in paths {
        let mut walker = WalkDir::new(base).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            files_scanned += 1;

            // For SFNT formats use the partial reader; for WOFF fall back to
            // the full scan_file path (decoding is always required for WOFF).
            if is_sfnt_ext(&ext) {
                match sfnt_partial::read_face_metadata_partial(path) {
                    Ok(file_faces) => faces.extend(file_faces),
                    Err(e) => errors.push((path.to_path_buf(), e.to_string())),
                }
            } else {
                match scan_file(path) {
                    Ok(file_faces) => faces.extend(file_faces),
                    Err(e) => errors.push((path.to_path_buf(), e.to_string())),
                }
            }
        }
    }

    ScanResult {
        faces,
        errors,
        files_scanned,
        elapsed: start.elapsed(),
    }
}

/// Read [`FaceInfo`] metadata for a single font file without a full parse.
///
/// For SFNT formats (TTF, OTF, TTC, OTC) this reads only the `name`, `OS/2`,
/// and `cmap` tables from disk, reconstructs a minimal SFNT in memory, and
/// delegates to `oxifont_parser` for field extraction. For WOFF and WOFF2 a
/// full parse is performed (decoding is always required).
///
/// # Errors
///
/// Returns [`FontError`] if the file cannot be read or is not parseable.
pub fn read_face_metadata_partial(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if is_sfnt_ext(&ext) {
        return sfnt_partial::read_face_metadata_partial(path);
    }

    // WOFF/WOFF2: full scan (decoding always required).
    scan_file(path)
}

/// Recursively scans `paths` for font files and returns a [`ScanResult`]
/// that captures both successful faces and per-file errors.
///
/// Uses default [`ScanOptions`] (all formats, symlinks followed, no depth
/// limit). Unlike [`scan_dirs`], malformed files are not silently dropped —
/// they appear in [`ScanResult::errors`] with a description of the failure.
pub fn scan_dirs_reporting(paths: &[PathBuf]) -> ScanResult {
    let start = std::time::Instant::now();
    let opts = ScanOptions::default();
    let mut faces: Vec<FaceInfo> = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();
    let mut files_scanned: usize = 0;

    for base in paths {
        let mut walker = WalkDir::new(base).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            files_scanned += 1;

            match scan_file(path) {
                Ok(file_faces) => faces.extend(file_faces),
                Err(e) => errors.push((path.to_path_buf(), e.to_string())),
            }
        }
    }

    ScanResult {
        faces,
        errors,
        files_scanned,
        elapsed: start.elapsed(),
    }
}

// ---------------------------------------------------------------------------
// Mtime tracking
// ---------------------------------------------------------------------------

/// A discovered font face together with its file path and last-modified time.
///
/// Returned by [`scan_dirs_with_mtime`]. The `mtime` field is useful for
/// cache invalidation: callers can compare `max(mtime)` against a stored
/// timestamp to decide whether to re-scan.
pub struct FaceWithMtime {
    /// The parsed face metadata.
    pub face: FaceInfo,
    /// The last-modified time of the file, or [`std::time::SystemTime::UNIX_EPOCH`]
    /// if the OS does not support mtime.
    pub mtime: std::time::SystemTime,
    /// Canonical path to the font file on disk.
    pub path: std::path::PathBuf,
}

/// Scans `paths` for font files, attaching the last-modified timestamp of
/// each file to the returned face records.
///
/// On platforms or filesystems that do not expose mtime,
/// [`std::time::SystemTime::UNIX_EPOCH`] is used as a safe fallback so the
/// function never panics.
///
/// # Cache invalidation pattern
/// ```no_run
/// let dirs = oxifont_discovery::system_font_dirs();
/// let faces = oxifont_discovery::scan_dirs_with_mtime(&dirs);
/// let newest = faces.iter().map(|f| f.mtime).max();
/// // Store `newest` and compare on future calls to skip re-parsing.
/// ```
pub fn scan_dirs_with_mtime(paths: &[impl AsRef<Path>]) -> Vec<FaceWithMtime> {
    let opts = ScanOptions::default();
    let mut results: Vec<FaceWithMtime> = Vec::new();

    for base in paths {
        let mut walker = WalkDir::new(base.as_ref()).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            // Retrieve mtime; fall back to UNIX_EPOCH on error.
            let mtime = std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            if let Ok(faces) = scan_file(path) {
                for face in faces {
                    results.push(FaceWithMtime {
                        face,
                        mtime,
                        path: path.to_path_buf(),
                    });
                }
            }
        }
    }

    results
}

/// Returns the maximum last-modified time across all font files found in
/// `paths`, without building [`FaceInfo`] records.
///
/// This is the fastest way to detect whether a font directory has changed
/// since the last cache write. Returns [`std::time::SystemTime::UNIX_EPOCH`]
/// when no font files are found or when mtime is not available.
pub fn max_mtime_of_dirs(paths: &[impl AsRef<Path>]) -> std::time::SystemTime {
    let opts = ScanOptions::default();
    let mut max_time = std::time::SystemTime::UNIX_EPOCH;

    for base in paths {
        let mut walker = WalkDir::new(base.as_ref()).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            let mtime = std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            if mtime > max_time {
                max_time = mtime;
            }
        }
    }

    max_time
}

// ---------------------------------------------------------------------------
// Progress reporting
// ---------------------------------------------------------------------------

/// Scans `paths` for font files, invoking `progress` after each font-extension
/// file is processed.
///
/// The callback receives `(faces_so_far, current_path)` — the number of faces
/// accumulated so far and the path of the file just processed (regardless of
/// whether parsing succeeded). This lets callers render live progress bars or
/// log scan activity during large scans.
///
/// `progress` is `FnMut`, so closures that mutate captured state (e.g. a
/// counter) are accepted.
///
/// Files that fail to parse are silently skipped (as in [`scan_dirs`]), but
/// the progress callback is still invoked for them.
///
/// # Example
/// ```no_run
/// let dirs = oxifont_discovery::system_font_dirs();
/// let mut count = 0usize;
/// let faces = oxifont_discovery::scan_dirs_with_progress(&dirs, |n, path| {
///     count += 1;
///     eprintln!("scanned {} faces, current: {}", n, path.display());
/// });
/// ```
pub fn scan_dirs_with_progress<F>(paths: &[impl AsRef<Path>], mut progress: F) -> Vec<FaceInfo>
where
    F: FnMut(usize, &Path),
{
    let opts = ScanOptions::default();
    let mut results: Vec<FaceInfo> = Vec::new();

    for base in paths {
        let mut walker = WalkDir::new(base.as_ref()).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            if let Ok(faces) = scan_file(path) {
                results.extend(faces);
            }

            // Invoke progress callback after processing each font-extension file.
            progress(results.len(), path);
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Background (non-blocking) scanning
// ---------------------------------------------------------------------------

/// Scan font directories in a background thread.
///
/// Returns immediately with a [`std::thread::JoinHandle`] that resolves to a
/// [`ScanResult`] when scanning completes. Useful for non-blocking font
/// enumeration at application startup — the caller can continue other work
/// while the OS walk and parse proceed concurrently.
///
/// The background thread captures `dirs` by value, so no lifetime constraints
/// are imposed on the caller.
///
/// ```no_run
/// use oxifont_discovery::{system_font_dirs, scan_dirs_background};
///
/// let handle = scan_dirs_background(system_font_dirs());
/// // Do other work while fonts scan in background...
/// let result = handle.join().expect("scan thread panicked");
/// println!("Found {} faces", result.faces.len());
/// ```
pub fn scan_dirs_background(dirs: Vec<PathBuf>) -> std::thread::JoinHandle<ScanResult> {
    std::thread::spawn(move || scan_dirs_reporting(&dirs))
}

/// Scan system font directories in a background thread.
///
/// Convenience wrapper around [`scan_dirs_background`] that uses
/// [`system_font_dirs`] as the directory list. Returns immediately; join the
/// handle to obtain the [`ScanResult`].
///
/// ```no_run
/// use oxifont_discovery::scan_system_fonts_background;
///
/// let handle = scan_system_fonts_background();
/// // Do other work while fonts scan in background...
/// let result = handle.join().expect("scan thread panicked");
/// println!("Found {} faces", result.faces.len());
/// ```
pub fn scan_system_fonts_background() -> std::thread::JoinHandle<ScanResult> {
    scan_dirs_background(system_font_dirs())
}

// ---------------------------------------------------------------------------
// Parallel scanning (rayon feature)
// ---------------------------------------------------------------------------

/// Collect all font-extension file paths from `root` without parsing them.
///
/// Used internally by [`scan_dirs_parallel`] to separate the sequential
/// filesystem walk from the parallel parsing step.
#[cfg(feature = "rayon")]
fn collect_font_paths(root: &Path) -> Vec<PathBuf> {
    let opts = ScanOptions::default();
    let mut walker = WalkDir::new(root).follow_links(opts.follow_symlinks);
    if let Some(depth) = opts.max_depth {
        walker = walker.max_depth(depth);
    }

    walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let path = entry.into_path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if opts.accepts_ext(&ext) {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

/// Parse all font faces from a single file path, returning an empty `Vec` on
/// any error.
///
/// This is the per-file unit used by both the sequential and parallel scan
/// paths so that the parsing logic is not duplicated.
#[cfg(feature = "rayon")]
fn parse_font_file(path: &Path) -> Vec<FaceInfo> {
    scan_file(path).unwrap_or_default()
}

/// Scan font directories in parallel using rayon.
///
/// Only available when the `rayon` feature is enabled.
///
/// Equivalent to [`scan_dirs`] but separates the filesystem walk (sequential,
/// I/O bound) from the font parsing (CPU bound) and runs the parsing step with
/// a rayon parallel iterator. On systems with many CPU cores and large font
/// directories this can meaningfully reduce wall-clock scan time.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "rayon")]
/// # {
/// let dirs = oxifont_discovery::system_font_dirs();
/// let faces = oxifont_discovery::scan_dirs_parallel(&dirs);
/// println!("found {} faces (parallel scan)", faces.len());
/// # }
/// ```
#[cfg(feature = "rayon")]
pub fn scan_dirs_parallel(paths: &[impl AsRef<Path> + Sync]) -> Vec<FaceInfo> {
    use rayon::prelude::*;

    // Collect all matching font file paths sequentially (filesystem walks do
    // not benefit from parallelism due to OS-level serialisation).
    let font_paths: Vec<PathBuf> = paths
        .iter()
        .flat_map(|p| collect_font_paths(p.as_ref()))
        .collect();

    // Parse each file in parallel — this is the CPU-bound step.
    font_paths
        .par_iter()
        .flat_map(|path| parse_font_file(path))
        .collect()
}

// ---------------------------------------------------------------------------
// Memory-mapped font I/O (mmap feature)
// ---------------------------------------------------------------------------

/// Read a font file using a memory-mapped mapping.
///
/// The mapping is read-only. The returned `Vec<u8>` is a copy of the mapped
/// region — callers do not need to manage the lifetime of the mapping.
///
/// # Safety rationale
/// `memmap2::MmapOptions::map` is `unsafe` because the OS may modify the
/// underlying file between the mapping being created and the bytes being read,
/// potentially causing torn reads. Here we immediately copy the mapped bytes
/// into an owned `Vec`, which contains the risk to a narrow window. Callers
/// must not hold references into the mapping across file-modification
/// operations (this API does not expose such references).
///
/// # Errors
/// Returns an [`std::io::Error`] if the file cannot be opened or mapped.
#[cfg(feature = "mmap")]
pub fn read_font_file_mmap(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    // SAFETY: mapping is read-only; we copy immediately so the caller cannot
    // observe torn reads after this function returns.
    #[allow(unsafe_code)]
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    Ok(mmap.to_vec())
}

/// Scan font directories using memory-mapped I/O for font file loading.
///
/// Functionally equivalent to [`scan_dirs_reporting`] but uses
/// [`read_font_file_mmap`] instead of `std::fs::read` when loading SFNT
/// font bytes, which may improve performance on large font directories by
/// deferring page-fault costs.
///
/// Only available when the `mmap` feature is enabled.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "mmap")]
/// # {
/// let dirs = oxifont_discovery::system_font_dirs();
/// let result = oxifont_discovery::scan_dirs_mmap(&dirs);
/// println!("found {} faces via mmap scan", result.faces.len());
/// # }
/// ```
#[cfg(feature = "mmap")]
pub fn scan_dirs_mmap(dirs: &[PathBuf]) -> ScanResult {
    let start = std::time::Instant::now();
    let opts = ScanOptions::default();
    let mut faces: Vec<FaceInfo> = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();
    let mut files_scanned: usize = 0;

    for base in dirs {
        let mut walker = WalkDir::new(base).follow_links(opts.follow_symlinks);
        if let Some(depth) = opts.max_depth {
            walker = walker.max_depth(depth);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if !opts.accepts_ext(&ext) {
                continue;
            }

            files_scanned += 1;

            // For SFNT formats use mmap; fall through to scan_file for WOFF.
            if is_sfnt_ext(&ext) {
                match read_font_file_mmap(path) {
                    Ok(bytes) => {
                        let arc: std::sync::Arc<[u8]> = bytes.into();
                        let mut file_faces = Vec::new();
                        match parse_sfnt_faces(path, arc, &mut file_faces) {
                            Ok(()) => faces.extend(file_faces),
                            Err(e) => errors.push((path.to_path_buf(), e)),
                        }
                    }
                    Err(e) => errors.push((path.to_path_buf(), e.to_string())),
                }
            } else {
                match scan_file(path) {
                    Ok(file_faces) => faces.extend(file_faces),
                    Err(e) => errors.push((path.to_path_buf(), e.to_string())),
                }
            }
        }
    }

    ScanResult {
        faces,
        errors,
        files_scanned,
        elapsed: start.elapsed(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw bytes of the test fixture TTF embedded at compile time.
    static TTF_FIXTURE: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    // Write the fixture TTF to a unique temp file, returning its path.
    // Caller is responsible for cleanup (or not — it's in temp_dir).
    fn write_temp_ttf(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("oxifont_disco_{suffix}.ttf"));
        std::fs::write(&p, TTF_FIXTURE).expect("failed to write temp TTF");
        p
    }

    #[test]
    fn test_scan_single_ttf_file() {
        let path = write_temp_ttf("single");
        let faces = scan_file(&path).expect("scan_file must succeed for valid TTF");
        assert_eq!(faces.len(), 1, "TTF contains exactly one face");
        assert!(!faces[0].family.is_empty(), "family name must not be empty");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_user_font_dirs_non_empty() {
        let dirs = user_font_dirs();
        // On macOS/Linux $HOME is always set in a development shell;
        // on CI containers it is typically set too.
        assert!(
            !dirs.is_empty(),
            "user_font_dirs() must return at least one directory"
        );
    }

    #[test]
    fn test_extensions_include_woff() {
        let opts = ScanOptions::default();
        assert!(opts.accepts_ext("woff"), "default opts must accept .woff");
        assert!(opts.accepts_ext("woff2"), "default opts must accept .woff2");
        assert!(opts.accepts_ext("ttf"), "default opts must accept .ttf");
        assert!(opts.accepts_ext("otf"), "default opts must accept .otf");
        assert!(opts.accepts_ext("ttc"), "default opts must accept .ttc");
        assert!(opts.accepts_ext("otc"), "default opts must accept .otc");
    }

    #[test]
    fn test_scan_options_builder() {
        let opts = ScanOptions::default()
            .include_woff(false)
            .include_woff2(false)
            .follow_symlinks(false)
            .max_depth(5);

        assert!(!opts.include_woff);
        assert!(!opts.include_woff2);
        assert!(!opts.follow_symlinks);
        assert_eq!(opts.max_depth, Some(5));

        // Verify filtered extensions
        assert!(
            !opts.accepts_ext("woff"),
            "woff disabled must not be accepted"
        );
        assert!(
            !opts.accepts_ext("woff2"),
            "woff2 disabled must not be accepted"
        );
        assert!(opts.accepts_ext("ttf"), "ttf always accepted");
    }

    #[test]
    fn test_scan_file_unsupported_extension() {
        let p = std::env::temp_dir().join("oxifont_disco_test.xyz");
        std::fs::write(&p, b"garbage").ok();
        let res = scan_file(&p);
        assert!(
            matches!(res, Err(FontError::UnsupportedFormat)),
            "unknown extension must return UnsupportedFormat"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_scan_dirs_with_options_max_depth() {
        // Create a temp dir with a TTF one level deep; max_depth=1 should
        // still find it (root counts as depth 0, contents as depth 1).
        let base = std::env::temp_dir().join("oxifont_disco_depth_test");
        let _ = std::fs::create_dir_all(&base);
        let font_path = base.join("sub.ttf");
        std::fs::write(&font_path, TTF_FIXTURE).expect("write sub.ttf");

        let opts = ScanOptions::default().max_depth(1);
        let results = scan_dirs_with_options(std::slice::from_ref(&base), &opts);
        assert!(!results.is_empty(), "should find the TTF at depth 1");

        let _ = std::fs::remove_file(&font_path);
        let _ = std::fs::remove_dir(&base);
    }

    #[test]
    fn test_scan_dirs_reporting_counts_files() {
        let base = std::env::temp_dir().join("oxifont_disco_reporting_test");
        let _ = std::fs::create_dir_all(&base);

        // Write two valid TTFs and one corrupt file.
        std::fs::write(base.join("a.ttf"), TTF_FIXTURE).ok();
        std::fs::write(base.join("b.ttf"), TTF_FIXTURE).ok();
        std::fs::write(base.join("bad.ttf"), b"not a font").ok();

        let result = scan_dirs_reporting(std::slice::from_ref(&base));
        assert_eq!(result.files_scanned, 3, "3 .ttf files should be counted");
        assert_eq!(result.faces.len(), 2, "2 valid TTFs → 2 faces");
        assert_eq!(result.errors.len(), 1, "1 corrupt file → 1 error");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_scan_result_elapsed_and_total_errors() {
        let base = std::env::temp_dir().join("oxifont_disco_elapsed_test");
        let _ = std::fs::create_dir_all(&base);

        // Write one valid TTF and one corrupt file.
        std::fs::write(base.join("good.ttf"), TTF_FIXTURE).ok();
        std::fs::write(base.join("bad.ttf"), b"not a font").ok();

        let result = scan_dirs_reporting(std::slice::from_ref(&base));

        // elapsed must be a non-negative duration (Duration is always >= 0).
        // We can only assert that it doesn't panic and represents real time.
        let _ = result.elapsed.as_nanos();

        // total_errors() is a convenience accessor for errors.len().
        assert_eq!(
            result.total_errors(),
            result.errors.len(),
            "total_errors() must equal errors.len()"
        );
        assert_eq!(result.total_errors(), 1, "one corrupt file → one error");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(any(not(feature = "woff1"), not(feature = "woff2")))]
    #[test]
    fn test_woff_placeholder_when_feature_disabled() {
        // This test validates the placeholder path for systems where
        // woff1/woff2 features may not be enabled. We test the helper
        // function directly by calling woff_placeholder.
        let p = PathBuf::from("/fake/path/font.woff");
        let info = woff_placeholder(&p);
        assert_eq!(info.path, p);
        assert!(info.family.is_empty());
        assert_eq!(info.face_index, 0);
    }

    #[test]
    fn test_system_font_dirs_returns_vec() {
        // system_font_dirs() must not panic and must return a Vec (possibly
        // empty on platforms with no probed paths).
        let dirs = system_font_dirs();
        // On macOS we expect at least one entry.
        #[cfg(target_os = "macos")]
        assert!(
            !dirs.is_empty(),
            "macOS must have at least one system font dir"
        );
        // On other platforms just confirm it doesn't panic.
        let _ = dirs;
    }
}
