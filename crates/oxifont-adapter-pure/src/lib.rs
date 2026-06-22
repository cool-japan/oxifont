#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `oxifont-adapter-pure` — Pure Rust [`FontDatabase`] built on filesystem
//! scanning.
//!
//! Composes [`oxifont_discovery`] (directory scan) and [`oxifont_parser`]
//! (TTF/OTF/TTC parsing) into a [`FontCatalog`]
//! implementation that requires no native libraries.
//!
//! # Features
//! - `cache`: Enable disk-based face-metadata caching. Uses `serde_json` to
//!   persist [`FaceInfo`] records; requires `oxifont-core/serde`. When enabled,
//!   [`FontDatabase::system_cached`] and [`FontDatabase::scan_cached`] are
//!   available.
//! - `db`: Enable the bridge to [`oxifont_db`]. Adds [`FontDatabase::into_db`]
//!   and [`FontDatabase::as_db`] which convert this catalog to an
//!   [`oxifont_db::FontDatabase`] for CSS Fonts Level 4 matching via
//!   [`oxifont_db::Query`].
//!
//! # Integration with oxitext
//!
//! [`FontDatabase`] can serve as the font backend for `oxitext`'s pipeline.
//! The `oxitext::Pipeline::new(font_db)` constructor accepts an
//! `&oxifont::FontDatabase` (which is a type alias for
//! `oxifont_adapter_pure::FontDatabase` when using the `pure` Cargo feature).
//!
//! ```ignore
//! use oxifont_adapter_pure::FontDatabase;
//! use oxifont::FontDatabase as OxiFont;  // requires oxifont `pure` feature
//! // This block is only illustrative; oxitext is in a separate crate
//! ```
//!
//! # Subsetting integration
//!
//! Use [`FontDatabase::font_bytes`] to retrieve raw SFNT bytes for a face,
//! then pass them to `oxifont_subset::subset_font` for glyph subsetting:
//!
//! ```no_run
//! use oxifont_adapter_pure::FontDatabase;
//! use oxifont_core::FontCatalog as _;
//!
//! let db = FontDatabase::system().unwrap();
//! if let Some(info) = db.faces().first() {
//!     let bytes = db.font_bytes(info).unwrap();
//!     // Pass `bytes` to `oxifont_subset::subset_font(&bytes, &codepoints)`.
//! }
//! ```
//!
//! # Example
//! ```no_run
//! use oxifont_adapter_pure::FontDatabase;
//! use oxifont_core::{FontCatalog as _, FontQuery};
//!
//! let db = FontDatabase::system().unwrap();
//! println!("found {} faces", db.faces().len());
//!
//! if let Some(face) = db.find(&FontQuery::new().family("Helvetica")) {
//!     println!("found: {}", face.family);
//! }
//! ```

use std::collections::HashMap;
use std::path::Path;

use oxifont_core::{FaceInfo, FontCatalog, FontError, FontQuery, FontStyle};
use oxifont_parser::ParsedFace;

// ---------------------------------------------------------------------------
// Generic family alias table (CSS Fonts Level 4 §3.1)
// ---------------------------------------------------------------------------

/// Static map from CSS generic family names to ordered lists of concrete
/// family names to try as fallbacks.
const GENERIC_FAMILIES: &[(&str, &[&str])] = &[
    (
        "sans-serif",
        &[
            "Arial",
            "Helvetica",
            "DejaVu Sans",
            "Liberation Sans",
            "Nimbus Sans",
            "FreeSans",
            "Noto Sans",
        ],
    ),
    (
        "serif",
        &[
            "Times New Roman",
            "Georgia",
            "DejaVu Serif",
            "Liberation Serif",
            "Nimbus Roman",
            "FreeSerif",
            "Noto Serif",
        ],
    ),
    (
        "monospace",
        &[
            "Courier New",
            "Courier",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Nimbus Mono",
            "FreeMono",
            "Noto Sans Mono",
        ],
    ),
    (
        "cursive",
        &["Comic Sans MS", "Zapf Chancery", "URW Chancery L"],
    ),
    ("fantasy", &["Impact", "Copperplate", "Papyrus"]),
];

// ---------------------------------------------------------------------------
// CSS §4.5 weight priority helper
// ---------------------------------------------------------------------------

/// Compute the priority ordering key for weight matching per CSS Fonts Level 4
/// §4.5.5. Returns a `(u32, u32)` sort key where a smaller value means higher
/// preference: `(tier, distance)`.
///
/// Tier 0 = exact match, Tier 1 = first fallback group, Tier 2 = second
/// fallback group. Within a tier the `distance` is the absolute distance
/// between query weight and face weight.
fn weight_priority(query_w: u16, face_w: u16) -> (u32, u32) {
    let distance = (query_w as i32 - face_w as i32).unsigned_abs();
    match query_w {
        // Exact match is always best regardless of query value.
        q if q == face_w => (0, 0),

        // weight == 400: prefer 400, then 500, then 300→200→100, then 600→900
        400 => match face_w {
            500 => (1, 0),
            w if w < 400 => (2, (400 - w) as u32),
            w => (3, (w - 400) as u32),
        },

        // weight == 500: prefer 500, then 400, then 300→200→100, then 600→900
        500 => match face_w {
            400 => (1, 0),
            w if w < 400 => (2, (500 - w) as u32),
            w => (3, (w - 500) as u32),
        },

        // weight < 400: nearest below first, then ascending above
        q if q < 400 => {
            if face_w <= q {
                (1, distance)
            } else {
                (2, distance)
            }
        }

        // weight > 500: nearest above first, then descending below
        q => {
            if face_w >= q {
                (1, distance)
            } else {
                (2, distance)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CSS §4.5.4 style priority helper
// ---------------------------------------------------------------------------

/// Returns a priority value for style matching per CSS Fonts Level 4 §4.5.4.
/// Lower value = higher preference.
fn style_priority(query_style: &FontStyle, face_style: &FontStyle) -> u32 {
    match query_style {
        FontStyle::Italic => match face_style {
            FontStyle::Italic => 0,
            FontStyle::Oblique => 1,
            FontStyle::Normal => 2,
        },
        FontStyle::Oblique => match face_style {
            FontStyle::Oblique => 0,
            FontStyle::Italic => 1,
            FontStyle::Normal => 2,
        },
        FontStyle::Normal => match face_style {
            FontStyle::Normal => 0,
            FontStyle::Oblique => 1,
            FontStyle::Italic => 2,
        },
    }
}

// ---------------------------------------------------------------------------
// CSS §4.5.3 stretch priority helper
// ---------------------------------------------------------------------------

/// Returns a `(tier, distance)` sort key for stretch matching per CSS Fonts
/// Level 4 §4.5.3. Lower is better.
fn stretch_priority(query_s: u8, face_s: u8) -> (u32, u32) {
    let distance = (query_s as i32 - face_s as i32).unsigned_abs();
    match query_s {
        q if q == face_s => (0, 0),
        // S ≤ 5 (normal or narrower): prefer ≤ S (nearest below), then > S
        q if q <= 5 => {
            if face_s <= q {
                (1, distance)
            } else {
                (2, distance)
            }
        }
        // S > 5: prefer ≥ S (nearest above), then < S
        q => {
            if face_s >= q {
                (1, distance)
            } else {
                (2, distance)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FontDatabase
// ---------------------------------------------------------------------------

/// An in-memory catalog of font faces discovered by scanning directories.
///
/// Backed by a `Vec<FaceInfo>` (for ordered storage) and a
/// `HashMap<String, Vec<usize>>` index (for O(1) family lookup by exact
/// lowercase name). A linear substring scan is used as fallback so that the
/// documented case-insensitive substring semantics of [`FontCatalog::find`]
/// are preserved.
///
/// Thread-safe (immutable after construction via `scan`/`system`; mutations
/// are single-threaded builder calls).
#[derive(Debug)]
pub struct FontDatabase {
    faces: Vec<FaceInfo>,
    /// Lowercase family name → indices into `faces`.
    by_family: HashMap<String, Vec<usize>>,
}

impl FontDatabase {
    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Push one face record and update the `by_family` index.
    fn add_face(&mut self, face: FaceInfo) {
        let idx = self.faces.len();
        let key = face.family.to_lowercase();
        self.by_family.entry(key).or_default().push(idx);
        self.faces.push(face);
    }

    /// Rebuild `by_family` from scratch after bulk removals.
    fn rebuild_index(&mut self) {
        self.by_family.clear();
        for (idx, face) in self.faces.iter().enumerate() {
            let key = face.family.to_lowercase();
            self.by_family.entry(key).or_default().push(idx);
        }
    }

    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Builds an empty database with no faces.
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            by_family: HashMap::new(),
        }
    }

    /// Builds a database pre-populated with the given `FaceInfo` records.
    ///
    /// Useful for constructing test catalogs or programmatically assembled
    /// databases without scanning the filesystem.
    pub fn from_faces(faces: Vec<FaceInfo>) -> Self {
        let mut db = Self::new();
        for face in faces {
            db.add_face(face);
        }
        db
    }

    /// Builds a catalog by recursively scanning `paths` for font files.
    ///
    /// # Errors
    /// Returns [`FontError`] only in exceptional cases; individual font-parse
    /// failures are silently skipped.
    pub fn scan(paths: &[impl AsRef<Path>]) -> Result<Self, FontError> {
        let discovered = oxifont_discovery::scan_dirs(paths);
        let mut db = Self::new();
        for face in discovered {
            db.add_face(face);
        }
        Ok(db)
    }

    /// Builds a catalog from the OS system font directories.
    ///
    /// Calls [`oxifont_discovery::system_font_dirs`] to determine the search
    /// paths, then delegates to [`FontDatabase::scan`].
    ///
    /// Returns an empty catalog (not an error) when no system font directories
    /// exist (e.g. on a minimal CI container).
    ///
    /// # Errors
    /// Returns [`FontError`] only in exceptional cases; individual font-parse
    /// failures are silently skipped.
    pub fn system() -> Result<Self, FontError> {
        let dirs = oxifont_discovery::system_font_dirs();
        Self::scan(&dirs)
    }

    /// Builds a catalog from the OS system font directories using metadata-only
    /// scanning (no full font parse).
    ///
    /// Only the `name`, `OS/2`, and `cmap` SFNT tables are read per file.
    /// This is typically 10–50× faster than [`system`](Self::system) on systems
    /// with many large fonts because the `glyf`/`loca`/`hmtx` tables (which
    /// make up 90–99% of most font files) are never loaded.
    ///
    /// Actual glyph-level data can be loaded on demand via [`load_face`](Self::load_face).
    ///
    /// Returns an empty catalog (not an error) when no system font directories
    /// exist.
    ///
    /// # Errors
    /// Returns [`FontError`] only in exceptional cases; individual font-parse
    /// failures are silently skipped.
    pub fn system_lazy() -> Result<Self, FontError> {
        let dirs = oxifont_discovery::system_font_dirs();
        Self::scan_lazy(&dirs)
    }

    /// Builds a catalog from the given directories using metadata-only scanning.
    ///
    /// Equivalent to [`scan`](Self::scan) but reads only `name`, `OS/2`, and
    /// `cmap` tables per font file. All [`FaceInfo`] fields derivable from
    /// those three tables (family, PostScript name, style, weight, stretch) are
    /// populated. Fields requiring other tables (e.g. variation axes from
    /// `fvar`) are left at their zero/default values.
    ///
    /// # Errors
    /// Returns [`FontError`] only in exceptional cases; individual font-parse
    /// failures are silently skipped.
    pub fn scan_lazy(dirs: &[impl AsRef<std::path::Path>]) -> Result<Self, FontError> {
        let paths: Vec<std::path::PathBuf> =
            dirs.iter().map(|p| p.as_ref().to_path_buf()).collect();
        let result = oxifont_discovery::scan_dirs_metadata_only(&paths);
        let mut db = Self::new();
        for face in result.faces {
            db.add_face(face);
        }
        Ok(db)
    }

    // -----------------------------------------------------------------------
    // Mutation methods
    // -----------------------------------------------------------------------

    /// Scans a directory and adds all found font faces to this database.
    ///
    /// Uses [`oxifont_discovery::scan_dirs`] internally; malformed font files
    /// are silently skipped. Returns `&mut Self` for builder-style chaining.
    pub fn add_dir(&mut self, path: &Path) -> &mut Self {
        let found = oxifont_discovery::scan_dirs(&[path.to_path_buf()]);
        for face in found {
            self.add_face(face);
        }
        self
    }

    /// Parses a font from in-memory bytes and adds all contained faces.
    ///
    /// For TTC collections every sub-face is added. For TTF/OTF only face 0 is
    /// added. Returns the number of faces added.
    ///
    /// If `family_hint` is `Some`, the hint is used as the family name for any
    /// face whose parsed family name is empty or `"Unknown"` (e.g. a
    /// hand-crafted test font with no `name` table entries).
    ///
    /// # Errors
    /// Returns [`FontError::ParseError`] when not a single face can be parsed
    /// from the provided bytes. Faces that fail individually are skipped; the
    /// error is only propagated when **all** faces fail.
    pub fn add_bytes(
        &mut self,
        bytes: Vec<u8>,
        family_hint: Option<&str>,
    ) -> Result<usize, FontError> {
        let count = oxifont_parser::face_count(&bytes);
        let arc: std::sync::Arc<[u8]> = bytes.into();
        let mut added = 0usize;
        let mut last_err: Option<FontError> = None;

        for idx in 0..count {
            match ParsedFace::parse(arc.clone(), idx) {
                Ok(parsed) => {
                    let mut info = parsed.as_face_info();
                    // Apply hint when the parsed name is absent.
                    if let Some(hint) = family_hint {
                        if info.family.is_empty() || info.family.as_ref() == "Unknown" {
                            info.family = std::sync::Arc::from(hint);
                        }
                    }
                    self.add_face(info);
                    added += 1;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        if added == 0 {
            Err(last_err.unwrap_or(FontError::UnsupportedFormat))
        } else {
            Ok(added)
        }
    }

    /// Removes all faces whose `path` matches `path`.
    ///
    /// Returns the number of faces removed. The internal index is rebuilt after
    /// removal.
    pub fn remove(&mut self, path: &Path) -> usize {
        let before = self.faces.len();
        self.faces.retain(|f| f.path != path);
        let removed = before - self.faces.len();
        if removed > 0 {
            self.rebuild_index();
        }
        removed
    }

    /// Merges all faces from `other` into this database.
    ///
    /// Returns `&mut Self` for builder-style chaining.
    pub fn merge(&mut self, other: FontDatabase) -> &mut Self {
        for face in other.faces {
            self.add_face(face);
        }
        self
    }

    // -----------------------------------------------------------------------
    // Query methods
    // -----------------------------------------------------------------------

    /// Returns all faces whose family name matches `family` (case-insensitive,
    /// exact match against the full family name).
    ///
    /// For substring matching use [`FontCatalog::find`] with a
    /// [`FontQuery::family`] query instead.
    pub fn find_all(&self, family: &str) -> Vec<&FaceInfo> {
        let key = family.to_lowercase();
        match self.by_family.get(&key) {
            Some(indices) => indices.iter().filter_map(|&i| self.faces.get(i)).collect(),
            None => Vec::new(),
        }
    }

    /// Returns candidate faces for a given family name using the `by_family`
    /// index. Returns an empty slice when the family is not found.
    fn candidates_for_family<'a>(&'a self, family: &str) -> Vec<&'a FaceInfo> {
        let key = family.to_lowercase();
        match self.by_family.get(&key) {
            Some(indices) => indices.iter().filter_map(|&i| self.faces.get(i)).collect(),
            None => Vec::new(),
        }
    }

    /// Returns the best matching face for `query` using CSS Fonts Level 4
    /// §4.5 priority ordering (stretch → style → weight).
    ///
    /// Unlike [`FontCatalog::find`], this method uses **exact** case-insensitive
    /// family matching (not substring matching), and applies the full CSS
    /// font-matching algorithm for stretch, style, and weight.
    ///
    /// If the `query.family` is a CSS generic family keyword (`"sans-serif"`,
    /// `"serif"`, `"monospace"`, `"cursive"`, `"fantasy"`), the generic alias
    /// table is consulted and the first matching concrete family is returned.
    ///
    /// Returns `None` when no face in the database matches the family.
    pub fn find_css(&self, query: &FontQuery) -> Option<&FaceInfo> {
        // Collect the candidate pool from the family field.
        let mut candidates: Vec<&FaceInfo> = match &query.family {
            None => self.faces.iter().collect(),
            Some(family) => {
                let direct = self.candidates_for_family(family);
                if !direct.is_empty() {
                    direct
                } else {
                    // Not a direct match — try generic family resolution.
                    return self.resolve_generic_family(family, query);
                }
            }
        };

        if candidates.is_empty() {
            return None;
        }

        // Stage 1 — Stretch narrowing (CSS §4.5.3).
        if let Some(query_stretch) = &query.stretch {
            let q = query_stretch.to_width_class();
            // Find the best (minimum) stretch key among candidates.
            let best_stretch_key = candidates
                .iter()
                .map(|f| stretch_priority(q, f.stretch.to_width_class()))
                .min();
            if let Some(best) = best_stretch_key {
                candidates.retain(|f| stretch_priority(q, f.stretch.to_width_class()) == best);
            }
        }

        // Stage 2 — Style narrowing (CSS §4.5.4).
        if let Some(query_style) = &query.style {
            let best_style_key = candidates
                .iter()
                .map(|f| style_priority(query_style, &f.style))
                .min();
            if let Some(best) = best_style_key {
                candidates.retain(|f| style_priority(query_style, &f.style) == best);
            }
        }

        // Stage 3 — Weight narrowing (CSS §4.5.5).
        if let Some(query_weight) = query.weight {
            let best_weight_key = candidates
                .iter()
                .map(|f| weight_priority(query_weight, f.weight))
                .min();
            if let Some(best) = best_weight_key {
                candidates.retain(|f| weight_priority(query_weight, f.weight) == best);
            }
        }

        // Stage 4 — PostScript name exact filter (optional refinement).
        if let Some(ps_name) = &query.postscript_name {
            let ps_filtered: Vec<&FaceInfo> = candidates
                .iter()
                .copied()
                .filter(|f| &f.post_script_name == ps_name)
                .collect();
            if !ps_filtered.is_empty() {
                return ps_filtered.into_iter().next();
            }
        }

        candidates.into_iter().next()
    }

    /// Attempts to resolve a CSS generic family name to a concrete face.
    ///
    /// Looks up `name` in the static generic-family table, then tries each
    /// concrete family in order by delegating back to
    /// [`FontDatabase::find_css`] with the same style/weight/stretch
    /// constraints. Returns the first match.
    pub fn resolve_generic_family<'a>(
        &'a self,
        name: &str,
        query: &FontQuery,
    ) -> Option<&'a FaceInfo> {
        let lower = name.to_lowercase();
        let concrete_families = GENERIC_FAMILIES
            .iter()
            .find(|(generic, _)| *generic == lower.as_str())
            .map(|(_, families)| *families)?;

        for &family in concrete_families {
            // Build a new query with the resolved concrete family but the
            // same style/weight/stretch constraints from the original query.
            let resolved_query = FontQuery {
                family: Some(family.to_string()),
                style: query.style.clone(),
                weight: query.weight,
                stretch: query.stretch,
                postscript_name: query.postscript_name.clone(),
            };
            // Collect candidates directly (avoid infinite recursion through
            // find_css by going to candidates_for_family directly).
            let mut candidates = self.candidates_for_family(family);
            if candidates.is_empty() {
                continue;
            }

            // Apply same CSS narrowing stages.
            if let Some(query_stretch) = &resolved_query.stretch {
                let q = query_stretch.to_width_class();
                let best = candidates
                    .iter()
                    .map(|f| stretch_priority(q, f.stretch.to_width_class()))
                    .min();
                if let Some(best) = best {
                    candidates.retain(|f| stretch_priority(q, f.stretch.to_width_class()) == best);
                }
            }
            if let Some(query_style) = &resolved_query.style {
                let best = candidates
                    .iter()
                    .map(|f| style_priority(query_style, &f.style))
                    .min();
                if let Some(best) = best {
                    candidates.retain(|f| style_priority(query_style, &f.style) == best);
                }
            }
            if let Some(query_weight) = resolved_query.weight {
                let best = candidates
                    .iter()
                    .map(|f| weight_priority(query_weight, f.weight))
                    .min();
                if let Some(best) = best {
                    candidates.retain(|f| weight_priority(query_weight, f.weight) == best);
                }
            }

            if let Some(face) = candidates.into_iter().next() {
                return Some(face);
            }
        }
        None
    }

    /// Loads and fully parses the face described by `info`.
    ///
    /// # Errors
    /// Returns [`FontError::IoError`] if the file cannot be read, or
    /// [`FontError::ParseError`] if the bytes are malformed.
    pub fn load_face(&self, info: &FaceInfo) -> Result<ParsedFace, FontError> {
        ParsedFace::from_face_info(info)
    }

    /// Returns the raw font file bytes for the face described by `info`.
    ///
    /// This method provides access to the raw SFNT bytes needed for subsetting
    /// operations (e.g. via [`oxifont_subset::subset_font`]) or WOFF2 encoding.
    /// It simply reads the file from disk — no parsing is performed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_core::FontCatalog as _;
    ///
    /// let db = FontDatabase::system().unwrap();
    /// if let Some(info) = db.faces().first() {
    ///     let bytes = db.font_bytes(info).unwrap();
    ///     println!("loaded {} bytes for {:?}", bytes.len(), info.path);
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns [`FontError::IoError`] if the file at `info.path` cannot be read.
    pub fn font_bytes(&self, info: &FaceInfo) -> Result<Vec<u8>, FontError> {
        std::fs::read(&info.path).map_err(FontError::from)
    }

    // -----------------------------------------------------------------------
    // Fallback chain query
    // -----------------------------------------------------------------------

    /// Try each family name in `families` until one has a matching face in the
    /// database, and return the first match.
    ///
    /// Each family is resolved with the style, weight, and stretch constraints
    /// taken from `base_query`. The `text` parameter is reserved for future
    /// cmap-coverage checking (e.g. verifying that the face can render every
    /// codepoint in the string); in this implementation it is accepted but not
    /// yet used, because `FaceInfo` carries no pre-computed unicode range
    /// bitmask and loading every candidate font from disk would be prohibitively
    /// expensive in a hot path.
    ///
    /// Returns the first face whose family name resolves via [`find_css`], or
    /// `None` when no family in `families` is present in the database.
    ///
    /// # Example
    /// ```
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
    /// use std::path::PathBuf;
    /// use std::sync::Arc;
    ///
    /// let face = FaceInfo {
    ///     family: Arc::from("Arial"),
    ///     post_script_name: String::new(),
    ///     style: FontStyle::Normal,
    ///     weight: 400,
    ///     stretch: FontStretch::Normal,
    ///     path: PathBuf::from("/dev/null"),
    ///     face_index: 0,
    ///     localized_families: Vec::new(),
    /// };
    /// let db = FontDatabase::from_faces(vec![face]);
    /// let base = FontQuery::new().weight(400);
    /// let result = db.find_with_fallback(&["Arial", "Helvetica", "sans-serif"], &base, "Hello");
    /// assert!(result.is_some());
    /// ```
    ///
    /// [`find_css`]: FontDatabase::find_css
    pub fn find_with_fallback<'a>(
        &'a self,
        families: &[&str],
        base_query: &FontQuery,
        _text: &str,
    ) -> Option<&'a FaceInfo> {
        for &family in families {
            // Build a per-family query that preserves all constraints from
            // the caller's base query but pins the family field.
            let query = FontQuery {
                family: Some(family.to_string()),
                style: base_query.style.clone(),
                weight: base_query.weight,
                stretch: base_query.stretch,
                postscript_name: base_query.postscript_name.clone(),
            };
            if let Some(face) = self.find_css(&query) {
                return Some(face);
            }
        }
        None
    }

    /// Find the best face for a [`FontQuery`] and optional text, using the
    /// query's `family` field as the primary family, resolved through
    /// [`find_css`] (which handles generic family keywords such as
    /// `"sans-serif"` and applies full CSS §4.5 narrowing).
    ///
    /// This is a convenience wrapper around [`find_with_fallback`] that accepts
    /// a single `&FontQuery` instead of an explicit `&[&str]` families slice.
    /// It is the idiomatic entry-point when the call-site already has a
    /// `FontQuery` and a text string:
    ///
    /// - If `query.family` is `Some(name)`, the search is driven by `name`
    ///   (possibly a CSS generic keyword) with the remaining query constraints
    ///   forwarded verbatim.
    /// - If `query.family` is `None`, the method delegates directly to
    ///   [`find_css`] over the whole database (equivalent to an unconstrained
    ///   family query).
    ///
    /// The `text` parameter mirrors the same semantics as
    /// [`find_with_fallback`]: it is accepted for future cmap-coverage
    /// checking but is not yet used to filter candidates.
    ///
    /// # Example
    /// ```
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
    /// use std::path::PathBuf;
    /// use std::sync::Arc;
    ///
    /// let face = FaceInfo {
    ///     family: Arc::from("Arial"),
    ///     post_script_name: String::new(),
    ///     style: FontStyle::Normal,
    ///     weight: 400,
    ///     stretch: FontStretch::Normal,
    ///     path: PathBuf::from("/dev/null"),
    ///     face_index: 0,
    ///     localized_families: Vec::new(),
    /// };
    /// let db = FontDatabase::from_faces(vec![face]);
    /// let query = FontQuery::new().family("Arial").weight(400);
    /// let result = db.find_best_for_text(&query, "Hello");
    /// assert!(result.is_some());
    /// ```
    ///
    /// [`find_css`]: FontDatabase::find_css
    /// [`find_with_fallback`]: FontDatabase::find_with_fallback
    pub fn find_best_for_text<'a>(&'a self, query: &FontQuery, text: &str) -> Option<&'a FaceInfo> {
        match &query.family {
            Some(family) => {
                // Build a single-element families slice and delegate to the
                // existing fallback implementation; this reuses generic
                // resolution and CSS narrowing without code duplication.
                self.find_with_fallback(&[family.as_str()], query, text)
            }
            None => {
                // No family constraint — fall through to the full CSS matcher
                // which treats a `None` family as a wildcard.
                self.find_css(query)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Capacity / size
    // -----------------------------------------------------------------------

    /// Returns the total number of font faces in the database.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Returns `true` when the database contains no faces.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IntoIterator for &'a FontDatabase {
    type Item = &'a FaceInfo;
    type IntoIter = std::slice::Iter<'a, FaceInfo>;

    fn into_iter(self) -> Self::IntoIter {
        self.faces.iter()
    }
}

impl FontCatalog for FontDatabase {
    fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    fn find(&self, query: &FontQuery) -> Option<&FaceInfo> {
        // Fast path: if the query is an exact family-name lookup (no wildcards
        // from the other fields) we check the index first.  This path only
        // applies when every supplied field matches exactly one of the index
        // entries; substring queries fall through to the linear scan below.
        if let Some(family_q) = &query.family {
            let key = family_q.to_lowercase();
            if let Some(indices) = self.by_family.get(&key) {
                // Exact index hit — check the remaining fields linearly within
                // the (usually small) set of index matches.
                let candidate = indices.iter().filter_map(|&i| self.faces.get(i)).find(|f| {
                    let style_ok = query.style.as_ref().map(|q| &f.style == q).unwrap_or(true);
                    let weight_ok = query.weight.map(|q| f.weight == q).unwrap_or(true);
                    let stretch_ok = query
                        .stretch
                        .as_ref()
                        .map(|q| &f.stretch == q)
                        .unwrap_or(true);
                    let ps_ok = query
                        .postscript_name
                        .as_ref()
                        .map(|q| &f.post_script_name == q)
                        .unwrap_or(true);
                    style_ok && weight_ok && stretch_ok && ps_ok
                });
                if candidate.is_some() {
                    return candidate;
                }
                // Exact match missed — fall through to substring scan.
            }
        }

        // Fallback: linear substring scan preserves the original documented
        // behavior (case-insensitive substring match on family name).
        self.faces.iter().find(|f| {
            let family_ok = query
                .family
                .as_ref()
                .map(|q| f.family.to_lowercase().contains(&q.to_lowercase()))
                .unwrap_or(true);

            let style_ok = query.style.as_ref().map(|q| &f.style == q).unwrap_or(true);

            let weight_ok = query.weight.map(|q| f.weight == q).unwrap_or(true);

            let stretch_ok = query
                .stretch
                .as_ref()
                .map(|q| &f.stretch == q)
                .unwrap_or(true);

            let ps_ok = query
                .postscript_name
                .as_ref()
                .map(|q| &f.post_script_name == q)
                .unwrap_or(true);

            family_ok && style_ok && weight_ok && stretch_ok && ps_ok
        })
    }
}

// ---------------------------------------------------------------------------
// oxifont-db bridge (feature = "db")
// ---------------------------------------------------------------------------
//
// When the `db` feature is enabled, `FontDatabase` gains a conversion method
// `into_db()` that migrates all `FaceInfo` records into an `oxifont_db::FontDatabase`,
// enabling access to its CSS Level 4 query engine (`oxifont_db::Query`).
//
// The conversion uses the `From<oxifont_core::FaceInfo> for oxifont_db::FaceInfo`
// bridge already provided by `oxifont-db/src/bridge.rs`, so each face record
// round-trips without data loss for all fields that `oxifont-core::FaceInfo`
// carries (family, weight, style, stretch, path, face_index).

#[cfg(feature = "db")]
impl FontDatabase {
    /// Converts this catalog into an [`oxifont_db::FontDatabase`], enabling
    /// full CSS Fonts Level 4 query access via [`oxifont_db::Query`].
    ///
    /// All face records currently stored in this catalog are converted to
    /// [`oxifont_db::FaceInfo`] using the standard `From` bridge (see
    /// `oxifont-db/src/bridge.rs`). The resulting database is independent of
    /// this one — both can be used simultaneously.
    ///
    /// # CSS Level 4 queries after conversion
    ///
    /// ```no_run
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_db::Query;
    ///
    /// let pure_db = FontDatabase::system().unwrap();
    /// let db = pure_db.into_db();
    ///
    /// if let Some(face) = Query::new(&db)
    ///     .family("sans-serif")
    ///     .weight(700)
    ///     .italic(false)
    ///     .match_best()
    /// {
    ///     println!("CSS match: {} weight={}", face.family, face.weight);
    /// }
    /// ```
    pub fn into_db(self) -> oxifont_db::FontDatabase {
        let mut db = oxifont_db::FontDatabase::new();
        for face in self.faces {
            let db_face = oxifont_db::FaceInfo::from(face);
            db.add_face(db_face);
        }
        db
    }

    /// Produces an [`oxifont_db::FontDatabase`] from a reference to this
    /// catalog, cloning each face record during conversion.
    ///
    /// Prefer [`FontDatabase::into_db`] when the pure database is no longer
    /// needed after the conversion.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use oxifont_adapter_pure::FontDatabase;
    ///
    /// let pure_db = FontDatabase::system().unwrap();
    /// let db = pure_db.as_db();
    /// // `pure_db` remains usable.
    /// println!("{} faces in CSS db", db.stats().face_count);
    /// ```
    pub fn as_db(&self) -> oxifont_db::FontDatabase {
        let mut db = oxifont_db::FontDatabase::new();
        for face in &self.faces {
            let db_face = oxifont_db::FaceInfo::from(face);
            db.add_face(db_face);
        }
        db
    }
}

// ---------------------------------------------------------------------------
// oxifont-subset bridge (feature = "subset")
// ---------------------------------------------------------------------------
//
// When the `subset` feature is enabled, `FontDatabase` gains two convenience
// methods that chain `font_bytes()` with the `oxifont_subset` subsetting
// pipeline:
//
//   - `subset_face(info, codepoints)` — subset with default options.
//   - `subset_face_for_web(info, codepoints)` — subset with web-friendly
//     presets (hints stripped, names trimmed).
//
// These methods provide font data access for `oxifont-subset` operations
// without requiring callers to explicitly read file bytes or import the
// `oxifont_subset` crate directly.

#[cfg(feature = "subset")]
impl FontDatabase {
    /// Reads the font file described by `info`, subsets it to the given
    /// `codepoints`, and returns the resulting SFNT bytes.
    ///
    /// This is a convenience wrapper around [`FontDatabase::font_bytes`] +
    /// [`oxifont_subset::subset_font`]. It uses the default [`oxifont_subset::SubsetOptions`]:
    /// hints are retained, layout tables (`GSUB`/`GPOS`/`GDEF`) are kept, and
    /// the full `name` table is preserved.
    ///
    /// Use [`subset_face_for_web`](Self::subset_face_for_web) for a
    /// web-optimised preset (strip hints, trim name table).
    ///
    /// # Errors
    /// - [`FontError::IoError`] if the font file cannot be read.
    /// - [`FontError::ParseError`] if the font bytes are structurally invalid
    ///   or subsetting fails.
    ///
    /// # Example
    /// ```no_run
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_core::FontCatalog as _;
    /// use std::collections::BTreeSet;
    ///
    /// let db = FontDatabase::system().unwrap();
    /// if let Some(info) = db.faces().first() {
    ///     let cps: BTreeSet<char> = "Hello, world!".chars().collect();
    ///     let subset_bytes = db.subset_face(info, &cps).unwrap();
    ///     println!("subset: {} bytes", subset_bytes.len());
    /// }
    /// ```
    pub fn subset_face(
        &self,
        info: &FaceInfo,
        codepoints: &std::collections::BTreeSet<char>,
    ) -> Result<Vec<u8>, FontError> {
        let bytes = self.font_bytes(info)?;
        oxifont_subset::subset_font(&bytes, codepoints)
            .map_err(|e| FontError::ParseError(format!("subset failed: {e}")))
    }

    /// Reads the font file described by `info`, subsets it to the given
    /// `codepoints` using web-optimised presets, and returns the resulting
    /// SFNT bytes.
    ///
    /// Equivalent to [`subset_face`](Self::subset_face) but with
    /// `strip_hints = true` and `retain_names = false` — suitable for web
    /// fonts where hint data is rarely beneficial and name records inflate the
    /// download size.
    ///
    /// # Errors
    /// - [`FontError::IoError`] if the font file cannot be read.
    /// - [`FontError::ParseError`] if the font bytes are structurally invalid
    ///   or subsetting fails.
    ///
    /// # Example
    /// ```no_run
    /// use oxifont_adapter_pure::FontDatabase;
    /// use oxifont_core::FontCatalog as _;
    /// use std::collections::BTreeSet;
    ///
    /// let db = FontDatabase::system().unwrap();
    /// if let Some(info) = db.faces().first() {
    ///     let cps: BTreeSet<char> = "Hello".chars().collect();
    ///     let web_bytes = db.subset_face_for_web(info, &cps).unwrap();
    ///     println!("web-subset: {} bytes", web_bytes.len());
    /// }
    /// ```
    pub fn subset_face_for_web(
        &self,
        info: &FaceInfo,
        codepoints: &std::collections::BTreeSet<char>,
    ) -> Result<Vec<u8>, FontError> {
        let bytes = self.font_bytes(info)?;
        oxifont_subset::subset_font_for_web(&bytes, codepoints)
            .map_err(|e| FontError::ParseError(format!("subset_for_web failed: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Disk cache (feature = "cache")
// ---------------------------------------------------------------------------
//
// The cache is a JSON file that stores a list of `FaceInfo` records together
// with the mtime (seconds since UNIX epoch) of each source font file.  On
// subsequent starts the cache is considered valid for a given font file only
// when its mtime has not changed; otherwise the face is re-parsed from disk.
//
// Layout of a cache entry:
//   { "path": "/path/to/Font.ttf", "mtime": 1716000000, "faces": [...FaceInfo...] }
//
// The cache is stored at `<cache_dir>/oxifont_face_cache.json` where
// `<cache_dir>` is `oxifont_core::platform_dirs::cache_dir()` (e.g. `~/.cache` on Linux,
// `~/Library/Caches` on macOS).

#[cfg(feature = "cache")]
pub(crate) mod cache {
    use super::FontDatabase;
    use oxifont_core::{FaceInfo, FontError};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::time::UNIX_EPOCH;

    // -----------------------------------------------------------------------
    // Cache file types
    // -----------------------------------------------------------------------

    /// A single cache record: all faces parsed from one source font file
    /// together with the file's mtime.
    #[derive(Debug, Serialize, Deserialize)]
    pub(crate) struct CacheRecord {
        /// Modification time of the source font file in seconds since UNIX
        /// epoch. Used to detect staleness.
        pub mtime: u64,
        /// All `FaceInfo` records extracted from this file.
        pub faces: Vec<FaceInfo>,
    }

    /// The complete on-disk cache: a map from source-font file path
    /// (as a UTF-8 string) to its [`CacheRecord`].
    #[derive(Debug, Default, Serialize, Deserialize)]
    pub(crate) struct CacheFile {
        /// Map: canonical UTF-8 path → per-file record.
        pub entries: HashMap<String, CacheRecord>,
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Returns the path to the on-disk cache file, or `None` if the platform
    /// has no accessible cache directory.
    fn cache_file_path() -> Option<PathBuf> {
        // Use the `OXIFONT_CACHE_DIR` env var as an override (useful in tests).
        if let Ok(dir) = std::env::var("OXIFONT_CACHE_DIR") {
            let dir = PathBuf::from(dir);
            if dir.exists() || std::fs::create_dir_all(&dir).is_ok() {
                return Some(dir.join("oxifont_face_cache.json"));
            }
        }
        let dir = oxifont_core::platform_dirs::cache_dir()?.join("oxifont");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join("oxifont_face_cache.json"))
    }

    /// Returns the mtime of `path` in whole seconds since UNIX epoch.
    /// Returns `None` on any I/O error.
    fn mtime_secs(path: &Path) -> Option<u64> {
        std::fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    /// Reads and deserialises the cache file from disk.  Returns an empty
    /// [`CacheFile`] on any read or parse error (treats errors as a cold start).
    pub(crate) fn load_cache(path: &Path) -> CacheFile {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Serialises `cache` and writes it atomically (write to a `.tmp` file,
    /// then rename) to `path`.  Silently ignores write errors so that cache
    /// failures never surface as fatal errors.
    pub(crate) fn save_cache(path: &Path, cache: &CacheFile) {
        let json = match serde_json::to_string(cache) {
            Ok(j) => j,
            Err(_) => return,
        };
        // Write to a sibling temp file then rename for atomicity.
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, &json).is_err() {
            return;
        }
        let _ = std::fs::rename(&tmp, path);
    }

    // -----------------------------------------------------------------------
    // Public API additions on FontDatabase
    // -----------------------------------------------------------------------

    impl FontDatabase {
        /// Builds a catalog by recursively scanning `paths`, using a
        /// JSON disk cache to avoid re-parsing unchanged font files.
        ///
        /// For each font file discovered:
        /// - If a cache entry exists **and** the file's mtime matches the
        ///   stored mtime, the cached [`FaceInfo`] records are loaded
        ///   directly without parsing the file.
        /// - Otherwise the file is parsed via [`oxifont_parser`], and the
        ///   resulting records are written back to the cache.
        ///
        /// The cache is stored at `<platform_cache_dir>/oxifont/oxifont_face_cache.json`.
        /// Set the `OXIFONT_CACHE_DIR` environment variable to override the
        /// cache directory (useful in tests).
        ///
        /// Cache failures (unreadable file, stale permissions, serialisation
        /// errors) are silently treated as a cold start; the database is
        /// always built correctly even without a working cache.
        ///
        /// # Errors
        /// Returns [`FontError`] only in the same exceptional cases as
        /// [`FontDatabase::scan`]; individual font-parse failures are skipped.
        pub fn scan_cached(paths: &[impl AsRef<Path>]) -> Result<Self, FontError> {
            let cache_path = cache_file_path();

            // Load the existing cache (empty if absent or corrupt).
            let mut disk_cache = cache_path.as_deref().map(load_cache).unwrap_or_default();

            // Collect all font file paths from the discovery layer.
            // `scan_dirs` returns fully-hydrated `FaceInfo` records but we
            // only need the file paths here; we discard the records and
            // re-drive caching ourselves.
            let font_paths: Vec<PathBuf> = {
                let discovered = oxifont_discovery::scan_dirs(paths);
                let mut seen = std::collections::HashSet::new();
                discovered
                    .into_iter()
                    .filter_map(|fi| {
                        if seen.insert(fi.path.clone()) {
                            Some(fi.path)
                        } else {
                            None
                        }
                    })
                    .collect()
            };

            let mut db = Self::new();
            let mut cache_dirty = false;

            for font_path in &font_paths {
                let key = font_path.to_string_lossy().into_owned();
                let current_mtime = mtime_secs(font_path);

                // Try to serve from cache.
                if let (Some(record), Some(mtime)) = (disk_cache.entries.get(&key), current_mtime) {
                    if record.mtime == mtime {
                        // Cache hit: add stored faces directly.
                        for face in &record.faces {
                            db.add_face(face.clone());
                        }
                        continue;
                    }
                }

                // Cache miss or stale: parse the file.
                let bytes = match std::fs::read(font_path) {
                    Ok(b) => b,
                    Err(_) => continue, // unreadable — skip silently
                };

                let arc: std::sync::Arc<[u8]> = bytes.into();
                let face_count = oxifont_parser::face_count(&arc);
                let mut new_faces: Vec<FaceInfo> = Vec::with_capacity(face_count as usize);

                for idx in 0..face_count {
                    if let Ok(parsed) = oxifont_parser::ParsedFace::parse(arc.clone(), idx) {
                        new_faces.push(parsed.as_face_info());
                    }
                }

                if new_faces.is_empty() {
                    continue;
                }

                // Add to database.
                for face in &new_faces {
                    db.add_face(face.clone());
                }

                // Update cache entry.
                if let Some(mtime) = current_mtime {
                    disk_cache.entries.insert(
                        key,
                        CacheRecord {
                            mtime,
                            faces: new_faces,
                        },
                    );
                    cache_dirty = true;
                }
            }

            // Prune stale entries (paths no longer discovered).
            let path_set: std::collections::HashSet<String> = font_paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let before = disk_cache.entries.len();
            disk_cache.entries.retain(|k, _| path_set.contains(k));
            if disk_cache.entries.len() != before {
                cache_dirty = true;
            }

            // Persist updated cache.
            if cache_dirty {
                if let Some(ref cp) = cache_path {
                    save_cache(cp, &disk_cache);
                }
            }

            Ok(db)
        }

        /// Builds a cached catalog from the OS system font directories.
        ///
        /// Equivalent to calling [`scan_cached`] with the paths returned by
        /// [`oxifont_discovery::system_font_dirs`].
        ///
        /// [`scan_cached`]: FontDatabase::scan_cached
        ///
        /// # Errors
        /// Returns [`FontError`] only in the same exceptional cases as
        /// [`FontDatabase::system`]; individual font-parse failures are skipped.
        pub fn system_cached() -> Result<Self, FontError> {
            let dirs = oxifont_discovery::system_font_dirs();
            Self::scan_cached(&dirs)
        }
    }
}
