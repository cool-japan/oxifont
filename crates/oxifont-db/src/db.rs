//! [`FontDatabase`] — in-memory indexed font database.
//!
//! # Design
//!
//! The database owns a flat `Vec<FaceInfo>` of all known faces and two
//! secondary indexes for fast family lookup.  Faces are never removed; IDs are
//! monotonically increasing u32 values.
//!
//! # Cache strategy
//!
//! The cold-scan cost of `/usr/share/fonts` on large Linux installs can exceed
//! 200 ms.  An **opt-in disk cache** is gated behind the `cache` Cargo feature.
//! The primary cache is a binary oxicode file at `~/.cache/oxifont/db_v1.bin`
//! with a magic header (`OXDB`) and version field for format safety.  A legacy
//! JSON file (`db_v1.json`) is tried as a fallback when the binary file is
//! absent or corrupt.  Only `Source::File` faces are persisted; `Source::Memory`
//! entries are not stored because their bytes cannot be round-tripped.

/// Magic bytes for the binary cache file format.
#[cfg(feature = "cache")]
const CACHE_MAGIC: &[u8; 4] = b"OXDB";

/// Binary cache format version.  Increment when the serialised layout changes.
/// Version 2: added `unicode_ranges: u128` field to [`FaceInfo`].
/// Version 3: added 8-byte `max_mtime` sentinel for mtime-based invalidation.
#[cfg(feature = "cache")]
const CACHE_VERSION: u32 = 3;

use smallvec::SmallVec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::DbError;
use crate::face::{FaceInfo, Source};
use crate::load;

/// Aggregate statistics about a [`FontDatabase`] instance.
///
/// Returned by [`FontDatabase::stats`].
pub struct DbStats {
    /// Total number of font faces currently in the database.
    pub face_count: usize,
    /// Number of distinct font families (case-insensitive).
    pub family_count: usize,
    /// Path of the disk cache that was loaded or saved, if any.
    pub cache_path: Option<std::path::PathBuf>,
}

/// In-memory indexed font database.
///
/// # Example
/// ```no_run
/// use oxifont_db::FontDatabase;
/// let mut db = FontDatabase::new();
/// db.load_file(std::path::Path::new("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")).ok();
/// println!("{} faces loaded", db.faces().len());
/// ```
pub struct FontDatabase {
    faces: Vec<FaceInfo>,
    /// Case-folded family name → indices into `faces`.
    ///
    /// Font families typically have fewer than 8 faces (Regular, Bold, Italic,
    /// BoldItalic, etc.), so `SmallVec<[usize; 8]>` avoids heap allocation for
    /// the common case while degenerating to a heap allocation for large families.
    by_family: HashMap<String, SmallVec<[usize; 8]>>,
    /// PostScript name → index into `faces`.
    by_postscript: HashMap<String, usize>,
    next_id: u32,
    /// Path of the disk cache that backed this database, if any.
    cache_path: Option<PathBuf>,
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDatabase {
    /// Creates a new, empty database.
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            by_family: HashMap::new(),
            by_postscript: HashMap::new(),
            next_id: 0,
            cache_path: None,
        }
    }

    /// Returns aggregate statistics about this database.
    ///
    /// The `cache_path` field reflects the disk cache that was used to
    /// populate this database (if any); it is `None` when the database was
    /// built purely from in-memory data or a cold scan.
    pub fn stats(&self) -> DbStats {
        DbStats {
            face_count: self.faces.len(),
            family_count: self.by_family.len(),
            cache_path: self.cache_path.clone(),
        }
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Inserts a [`FaceInfo`] record into the database, assigning a unique ID
    /// and updating the family and PostScript-name indices.  Returns the
    /// assigned face index.
    ///
    /// This method is primarily intended for testing and for loading
    /// pre-serialised cache records.
    pub fn add_face(&mut self, mut face: FaceInfo) -> usize {
        face.id = self.next_id;
        self.next_id += 1;
        let idx = self.faces.len();
        let family_key = face.family.to_lowercase();
        self.by_family.entry(family_key).or_default().push(idx);
        if !face.post_script_name.is_empty() {
            self.by_postscript
                .entry(face.post_script_name.clone())
                .or_insert(idx);
        }
        self.faces.push(face);
        idx
    }

    /// Load all valid faces from `data` bytes, treating them as a
    /// `Source::Memory` blob.  Returns the number of new faces added.
    fn load_bytes_as_memory(&mut self, data: Vec<u8>) -> usize {
        let count = load::face_count(&data);
        let mut added = 0usize;
        for index in 0..count {
            if let Some(info) = load::parse_face_info(&data, index, Source::Memory(data.clone())) {
                self.add_face(info);
                added += 1;
            }
        }
        added
    }

    /// Load all valid faces from `data` bytes, treating them as residing at
    /// `path` on disk.  The bytes are not retained in memory.
    fn load_bytes_as_file(&mut self, data: Vec<u8>, path: PathBuf) -> usize {
        let count = load::face_count(&data);
        let mut added = 0usize;
        for index in 0..count {
            if let Some(info) = load::parse_face_info(&data, index, Source::File(path.clone())) {
                self.add_face(info);
                added += 1;
            }
        }
        added
    }

    // ------------------------------------------------------------------
    // Public loading API
    // ------------------------------------------------------------------

    /// Load all font faces from raw bytes held in memory.
    ///
    /// Returns the number of faces added.
    pub fn load_bytes(&mut self, data: Vec<u8>) -> usize {
        self.load_bytes_as_memory(data)
    }

    /// Load all font faces from a file on disk.
    ///
    /// Returns the number of faces added, or a [`DbError`] if the file cannot
    /// be read (parse failures for individual faces are silently skipped).
    pub fn load_file(&mut self, path: &Path) -> Result<usize, DbError> {
        let data = std::fs::read(path)?;
        Ok(self.load_bytes_as_file(data, path.to_owned()))
    }

    /// Recursively load all font files from a directory tree.
    ///
    /// Files with extensions `.ttf`, `.otf`, and `.ttc` are processed; others
    /// are silently ignored.  Individual file I/O errors are also silently
    /// ignored so that a single unreadable font does not abort a directory
    /// scan.
    ///
    /// Family indexes are sorted by weight after all files have been loaded.
    pub fn load_dir(&mut self, dir: &Path) -> Result<(), DbError> {
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());
            match ext.as_deref() {
                Some("ttf") | Some("otf") | Some("ttc") => {
                    let _ = self.load_file(path); // best-effort
                }
                _ => {}
            }
        }
        self.sort_family_index();
        Ok(())
    }

    /// Load system fonts from the OS default font directories.
    ///
    /// On macOS: `/Library/Fonts`, `/System/Library/Fonts`, and
    /// `$HOME/Library/Fonts`.
    ///
    /// On Linux: `/usr/share/fonts` and `/usr/local/share/fonts`.
    ///
    /// On Windows: `C:\Windows\Fonts`.
    ///
    /// Directory errors are silently ignored (the system directory may not
    /// exist on all configurations).
    ///
    /// Family indexes are sorted by weight after all directories have been
    /// scanned.
    pub fn load_system_fonts(&mut self) -> Result<(), DbError> {
        let dirs = system_font_dirs();
        for d in &dirs {
            // Use load_file directly to avoid redundant per-dir sort.
            let _ = self.load_dir_unsorted(d);
        }
        self.sort_family_index();
        Ok(())
    }

    /// Load system fonts in a background thread.
    ///
    /// Spawns a new OS thread that calls [`Self::load_system_fonts`] and
    /// returns the populated database.  Directory and parse errors are
    /// silently ignored (same best-effort policy as [`Self::load_system_fonts`]).
    ///
    /// This is a thread-based (non-async) alternative that avoids blocking the
    /// calling thread.  The returned [`std::thread::JoinHandle`] resolves to a
    /// fully loaded `FontDatabase` once the background scan completes.
    ///
    /// # Example
    /// ```no_run
    /// let handle = oxifont_db::FontDatabase::load_system_fonts_bg();
    /// // ... do other work while fonts load ...
    /// let db = handle.join().expect("font loading thread panicked");
    /// println!("{} faces loaded", db.stats().face_count);
    /// ```
    pub fn load_system_fonts_bg() -> std::thread::JoinHandle<Self> {
        std::thread::spawn(|| {
            let mut db = Self::new();
            let _ = db.load_system_fonts();
            db
        })
    }

    /// Like [`Self::load_dir`] but skips the final `sort_family_index` call.
    ///
    /// Used internally to batch multiple directory scans before a single sort.
    fn load_dir_unsorted(&mut self, dir: &Path) -> Result<(), DbError> {
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());
            match ext.as_deref() {
                Some("ttf") | Some("otf") | Some("ttc") => {
                    let _ = self.load_file(path); // best-effort
                }
                _ => {}
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Convenience constructors
    // ------------------------------------------------------------------

    /// Build a database from a `Vec` of pre-parsed [`crate::face::FaceInfo`] records.
    ///
    /// This is the primary integration point for adapters that produce their
    /// own `FaceInfo` values (e.g. after bridging from `oxifont_core::FaceInfo`).
    /// Each face is inserted via [`Self::add_face`], which assigns unique IDs
    /// and rebuilds the family and PostScript-name indices.
    pub fn from_faces(faces: Vec<FaceInfo>) -> Self {
        let mut db = Self::new();
        for face in faces {
            db.add_face(face);
        }
        db
    }

    /// Build a fresh database from the system font directories.
    pub fn system() -> Result<Self, DbError> {
        let mut db = Self::new();
        db.load_system_fonts()?;
        Ok(db)
    }

    /// Build a database from the system fonts, using the optional disk cache.
    ///
    /// Requires the `cache` feature.  Tries the binary cache (`db_v1.bin`)
    /// first for speed, then falls back to the legacy JSON cache (`db_v1.json`)
    /// for forward compatibility.  On a complete cache miss the system fonts are
    /// scanned and both the binary and JSON caches are written.
    ///
    /// Cache files live in `$XDG_CACHE_HOME/oxifont/` (or `~/.cache/oxifont/`).
    #[cfg(feature = "cache")]
    pub fn system_cached() -> Result<Self, DbError> {
        let cache_dir = oxifont_core::platform_dirs::cache_dir()
            .ok_or_else(|| DbError::Cache("cannot determine cache directory".to_string()))?
            .join("oxifont");

        let bin_path = cache_dir.join("db_v1.bin");
        let json_path = cache_dir.join("db_v1.json");

        // Compute max mtime across all system font directories so we can
        // detect when fonts have been installed or removed since the cache
        // was written.
        let current_max_mtime = max_mtime_for_font_dirs(&system_font_dirs());

        // Try binary cache first (fastest path).
        if bin_path.exists() {
            if let Some((faces, cached_mtime)) = load_cache_binary(&bin_path) {
                if cached_mtime >= current_max_mtime {
                    let mut db = Self::new();
                    for face in faces {
                        db.add_face(face);
                    }
                    db.cache_path = Some(bin_path);
                    return Ok(db);
                }
                // mtime mismatch — fall through to rescan
            }
        }

        // Fall back to JSON cache (legacy / cross-version compatibility).
        if json_path.exists() {
            if let Ok(mut db) = Self::load_cache(&json_path) {
                // Opportunistically write the faster binary cache.
                let _ = save_cache_binary(&bin_path, db.faces(), current_max_mtime);
                db.cache_path = Some(bin_path);
                return Ok(db);
            }
        }

        // Full rescan — write both caches on success.
        let mut db = Self::system()?;
        let _ = save_cache_binary(&bin_path, db.faces(), current_max_mtime);
        let _ = db.save_cache(&json_path);
        db.cache_path = Some(bin_path);
        Ok(db)
    }

    // ------------------------------------------------------------------
    // Cache serialisation (feature = "cache")
    // ------------------------------------------------------------------

    /// Serialise the database to a JSON file at `path`.
    ///
    /// Only `Source::File` faces are persisted; `Source::Memory` faces are
    /// omitted because their bytes are not stored alongside the metadata.
    ///
    /// Requires the `cache` feature.
    #[cfg(feature = "cache")]
    pub fn save_cache(&self, path: &Path) -> Result<(), DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialisable: Vec<&FaceInfo> = self
            .faces
            .iter()
            .filter(|f| matches!(f.source, Source::File(_)))
            .collect();
        let json =
            serde_json::to_string(&serialisable).map_err(|e| DbError::Cache(e.to_string()))?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Deserialise a database from a JSON file written by [`Self::save_cache`].
    ///
    /// Requires the `cache` feature.
    #[cfg(feature = "cache")]
    pub fn load_cache(path: &Path) -> Result<Self, DbError> {
        let json = std::fs::read_to_string(path)?;
        let faces: Vec<FaceInfo> =
            serde_json::from_str(&json).map_err(|e| DbError::Cache(e.to_string()))?;
        let mut db = Self::new();
        for face in faces {
            db.add_face(face);
        }
        Ok(db)
    }

    /// Serialise the database to the oxicode binary format at `path`.
    ///
    /// The file begins with a 4-byte magic (`OXDB`) followed by a 4-byte
    /// little-endian version field, an 8-byte mtime sentinel, then the
    /// oxicode-encoded payload.  Only `Source::File` faces are persisted.
    ///
    /// A `max_mtime` of `0` is stored when no meaningful mtime is available
    /// (cache written without mtime tracking).
    ///
    /// Requires the `cache` feature.
    #[cfg(feature = "cache")]
    pub fn save_cache_binary(&self, path: &Path) -> Result<(), DbError> {
        save_cache_binary(path, self.faces(), 0)
    }

    /// Deserialise a database from a binary cache file written by
    /// [`Self::save_cache_binary`].
    ///
    /// Returns `None` if the file is absent, corrupt, or was written with a
    /// different format version.
    ///
    /// Requires the `cache` feature.
    #[cfg(feature = "cache")]
    pub fn load_cache_binary(path: &Path) -> Option<Self> {
        let (faces, _) = load_cache_binary(path)?;
        let mut db = Self::new();
        for face in faces {
            db.add_face(face);
        }
        Some(db)
    }

    // ------------------------------------------------------------------
    // Query API
    // ------------------------------------------------------------------

    /// Returns a slice of all face records in the database.
    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// Returns all faces whose lower-cased family name matches `family_lower`.
    pub(crate) fn faces_by_family_lower(&self, family_lower: &str) -> &[usize] {
        self.by_family
            .get(family_lower)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Returns all faces whose family name matches `name` (case-insensitive).
    pub fn faces_by_family(&self, name: &str) -> Vec<&FaceInfo> {
        let key = name.to_lowercase();
        self.by_family
            .get(&key)
            .map(|indices| indices.iter().map(|&i| &self.faces[i]).collect())
            .unwrap_or_default()
    }

    /// Returns the face with the given numeric ID, or `None` if not found.
    ///
    /// When the database has never had a face removed, the ID equals the
    /// index into the internal storage (O(1)).  After a [`Self::remove_face`]
    /// call, a linear scan is performed as a fallback because the ID-to-index
    /// mapping may have shifted.
    pub fn face_by_id(&self, id: u32) -> Option<&FaceInfo> {
        // Fast path: ID still equals index (common case).
        if let Some(face) = self.faces.get(id as usize) {
            if face.id == id {
                return Some(face);
            }
        }
        // Fallback linear scan (after removals).
        self.faces.iter().find(|f| f.id == id)
    }

    /// Remove the face with the given ID from the database.
    ///
    /// Returns `true` if the face was found and removed, `false` otherwise.
    ///
    /// This method also removes the face from the family index
    /// (`by_family`) and PostScript-name index (`by_postscript`), reclaiming
    /// any map entries that become empty.
    ///
    /// **Note:** After removal, calls to [`Self::face_by_id`] for IDs that were
    /// assigned *after* the removed face will fall back to a linear scan because
    /// the stored indices shift down.
    pub fn remove_face(&mut self, id: u32) -> bool {
        // Find the position of the face in `self.faces`.
        let Some(pos) = self.faces.iter().position(|f| f.id == id) else {
            return false;
        };

        let face = self.faces.remove(pos);

        // Remove from by_postscript index.
        if !face.post_script_name.is_empty() {
            self.by_postscript.remove(&face.post_script_name);
        }

        // Remove from by_family index and fix up all indices > pos.
        let mut empty_keys: Vec<String> = Vec::new();
        for (key, indices) in &mut self.by_family {
            // Remove the entry pointing to the removed face (if any).
            indices.retain(|i| *i != pos);
            // Shift down every index that was above the removed position.
            for idx in indices.iter_mut() {
                if *idx > pos {
                    *idx -= 1;
                }
            }
            if indices.is_empty() {
                empty_keys.push(key.clone());
            }
        }
        for key in empty_keys {
            self.by_family.remove(&key);
        }

        // Reindex by_postscript: all stored indices > pos must shift down.
        for idx in self.by_postscript.values_mut() {
            if *idx > pos {
                *idx -= 1;
            }
        }

        true
    }

    /// Sort each family's face list by weight in ascending order.
    ///
    /// Call this after bulk-loading faces to enable efficient weight-based
    /// matching.  Individual [`Self::add_face`] calls do not maintain sorted
    /// order; you must call this method again after any batch of insertions.
    pub fn sort_family_index(&mut self) {
        for indices in self.by_family.values_mut() {
            indices.sort_unstable_by_key(|&i| self.faces[i].weight);
        }
    }

    /// Looks up a face by its PostScript name (name ID 6).
    ///
    /// The lookup is case-sensitive and exact.  Returns `None` when no face
    /// with that PostScript name was loaded.
    pub fn find_by_postscript_name(&self, name: &str) -> Option<&FaceInfo> {
        self.by_postscript
            .get(name)
            .and_then(|&idx| self.faces.get(idx))
    }

    // ------------------------------------------------------------------
    // Locale-aware family enumeration
    // ------------------------------------------------------------------

    /// Returns all distinct locale-specific family names available in the
    /// database for the given BCP-47 locale tag.
    ///
    /// This is the primary integration point for locale-aware rendering
    /// pipelines (e.g. `oxitext-icu`): given a locale such as `"ja-JP"` or
    /// `"zh-CN"`, callers receive an ordered, deduplicated list of the family
    /// names that fonts in the database advertise for that locale via their
    /// OpenType `name` table records.
    ///
    /// The BCP-47 tag is matched against the Windows LCID table in
    /// [`crate::locale`].  If the full tag has no entry (e.g. `"ja-JP-x-example"`),
    /// the function progressively strips trailing subtags (`"ja-JP"`, then `"ja"`)
    /// until a match is found or the tag is exhausted.
    ///
    /// Returns an empty `Vec` when no faces carry locale metadata for `bcp47`.
    ///
    /// # Example
    /// ```no_run
    /// use oxifont_db::FontDatabase;
    ///
    /// let db = FontDatabase::system().unwrap();
    /// for family in db.locale_families_for("ja-JP") {
    ///     println!("Japanese family: {family}");
    /// }
    /// ```
    pub fn locale_families_for(&self, bcp47: &str) -> Vec<String> {
        // Resolve the LCID, trying progressively shorter subtags.
        let bcp47_lower = bcp47.to_lowercase();
        let lcid = {
            let mut tag: &str = &bcp47_lower;
            let mut found = None;
            loop {
                if let Some(id) = crate::locale::bcp47_to_lcid(tag) {
                    found = Some(id);
                    break;
                }
                match tag.rfind('-') {
                    Some(pos) => tag = &tag[..pos],
                    None => break,
                }
            }
            found
        };

        let Some(lcid) = lcid else {
            return Vec::new();
        };

        // Collect distinct family names for the resolved LCID.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut families: Vec<String> = Vec::new();
        for face in &self.faces {
            for (id, name) in &face.locale_families {
                if *id == lcid && seen.insert(name.clone()) {
                    families.push(name.clone());
                }
            }
        }
        families
    }

    /// Returns all faces in the database that support the given OpenType
    /// script tag, as determined by the OS/2 Unicode range bits.
    ///
    /// This is the integration point for per-script font selection in
    /// `oxitext-shape`: given a script tag such as `b"arab"`, `b"deva"`, or
    /// `b"hani"`, callers receive all faces whose `supported_scripts_approx`
    /// includes that tag.
    ///
    /// Coverage is **approximate** (derived from OS/2 range bits).  Faces
    /// whose `unicode_ranges` is `0` (unknown) are included because they may
    /// cover the requested script.
    ///
    /// Returns an empty slice when no faces match.
    ///
    /// # Example
    /// ```no_run
    /// use oxifont_db::FontDatabase;
    ///
    /// let db = FontDatabase::system().unwrap();
    /// for face in db.faces_for_script(b"arab") {
    ///     println!("Arabic-capable font: {}", face.family);
    /// }
    /// ```
    pub fn faces_for_script(&self, script_tag: &[u8; 4]) -> Vec<&FaceInfo> {
        self.faces
            .iter()
            .filter(|face| {
                // Faces with unknown ranges are included conservatively.
                if face.unicode_ranges == 0 {
                    return true;
                }
                face.supported_scripts_approx()
                    .iter()
                    .any(|t| t == script_tag)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Binary cache helpers (feature = "cache")
// ---------------------------------------------------------------------------

/// Write `faces` to a binary oxicode cache file at `path`.
///
/// File layout:
/// - bytes 0..4   : magic `b"OXDB"`
/// - bytes 4..8   : `CACHE_VERSION` as little-endian `u32`
/// - bytes 8..16  : `max_mtime` as little-endian `u64` (seconds since Unix epoch)
/// - bytes 16..   : oxicode-encoded `Vec<FaceInfo>` payload (file-sourced only)
///
/// The parent directory is created if it does not exist.
#[cfg(feature = "cache")]
fn save_cache_binary(path: &Path, faces: &[FaceInfo], max_mtime: u64) -> Result<(), DbError> {
    use oxicode::serde::encode_to_vec;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file_faces: Vec<&FaceInfo> = faces
        .iter()
        .filter(|f| matches!(f.source, Source::File(_)))
        .collect();

    let payload = encode_to_vec(&file_faces, oxicode::config::standard())
        .map_err(|e| DbError::Cache(e.to_string()))?;

    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(CACHE_MAGIC);
    out.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(&max_mtime.to_le_bytes());
    out.extend_from_slice(&payload);

    std::fs::write(path, &out)?;
    Ok(())
}

/// Load faces from a binary oxicode cache file at `path`.
///
/// Returns `Some((faces, max_mtime))` on success, or `None` if the file
/// cannot be read, the magic does not match, the version is unknown, or the
/// payload fails to decode.  `max_mtime` is the Unix-epoch timestamp recorded
/// at write time (or `0` for caches written without mtime tracking).
#[cfg(feature = "cache")]
fn load_cache_binary(path: &Path) -> Option<(Vec<FaceInfo>, u64)> {
    use oxicode::serde::decode_owned_from_slice;

    let data = std::fs::read(path).ok()?;
    if data.len() < 16 {
        return None;
    }
    if &data[0..4] != CACHE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version != CACHE_VERSION {
        return None;
    }
    let max_mtime = u64::from_le_bytes([
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
    ]);
    let (faces, _) =
        decode_owned_from_slice::<Vec<FaceInfo>, _>(&data[16..], oxicode::config::standard())
            .ok()?;
    Some((faces, max_mtime))
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

/// Returns the maximum modification time (seconds since Unix epoch) across all
/// font files reachable from `dirs`.
///
/// Only files with `.ttf`, `.otf`, or `.ttc` extensions are considered.
/// Returns `0` when no files are found or on any error.
#[cfg(feature = "cache")]
fn max_mtime_for_font_dirs(dirs: &[PathBuf]) -> u64 {
    use std::time::SystemTime;

    let mut max: u64 = 0;
    for dir in dirs {
        let walker = walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());
        for entry in walker {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());
            match ext.as_deref() {
                Some("ttf") | Some("otf") | Some("ttc") => {}
                _ => continue,
            }
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(dur) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                        let secs = dur.as_secs();
                        if secs > max {
                            max = secs;
                        }
                    }
                }
            }
        }
    }
    max
}

/// Returns the OS-specific list of system font directories.
fn system_font_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut dirs = vec![
            PathBuf::from("/Library/Fonts"),
            PathBuf::from("/System/Library/Fonts"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
        dirs
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        vec![PathBuf::from("C:\\Windows\\Fonts")]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        vec![]
    }
}
