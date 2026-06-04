#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # OxiFont
//!
//! Pure-Rust font discovery, parsing, subsetting, and web-font encoding.
//!
//! `oxifont` is a facade crate that re-exports the most commonly needed items
//! from the `oxifont-*` subcrates. Each subcrate can also be used independently.
//!
//! ## Feature Flags
//!
//! | Feature | Default | Enables |
//! |---------|---------|---------|
//! | `pure` | yes | [`FontDatabase`] from filesystem scan via `oxifont-adapter-pure` |
//! | `discovery` | yes | [`discovery`] module: [`discovery::system_font_dirs`], [`discovery::scan_dirs`] |
//! | `native` | no | [`native`] module: [`native::NativeCatalog`] via CoreText (macOS) or DirectWrite (Windows) |
//! | `db` | no | [`db`] module: [`db::FontDatabase`] with CSS Level 4 query engine |
//! | `woff1` | no | [`webfont`] module: WOFF1 encode and decode functions |
//! | `woff2` | no | [`webfont`] module: WOFF2 encode and decode functions |
//! | `subset` | no | [`subset`] module: [`subset_and_encode_woff2`] and glyph subsetting |
//! | `bundled-noto` | no | [`bundled`] module: embedded Noto Sans font bytes; enables [`system_with_bundled`] |
//! | `bundled-noto-cjk-jp` | no | [`bundled`] module: embedded Noto Sans CJK JP (Japanese) font bytes |
//! | `bundled-noto-cjk-kr` | no | [`bundled`] module: embedded Noto Sans CJK KR (Korean) font bytes |
//! | `bundled-noto-cjk-sc` | no | [`bundled`] module: embedded Noto Sans CJK SC (Simplified Chinese) font bytes |
//! | `bundled-noto-cjk-tc` | no | [`bundled`] module: embedded Noto Sans CJK TC (Traditional Chinese) font bytes |
//!
//! ## Quick Start
//!
//! ```no_run
//! use oxifont::{FontDatabase, FontCatalog as _, FontQuery};
//!
//! let db = FontDatabase::system().unwrap();
//! if let Some(face) = db.find(&FontQuery::new().family("Arial")) {
//!     println!("found: {} (weight {})", face.family, face.weight);
//! }
//! ```
//!
//! ## CSS Level 4 Query (feature `db`)
//!
//! ```no_run
//! use oxifont::db::{FontDatabase as Db, Query};
//!
//! let mut database = Db::new();
//! database.load_dir(std::path::Path::new("/usr/share/fonts")).ok();
//! if let Some(face) = Query::new(&database).family("sans-serif").weight(700).match_best() {
//!     println!("css match: {} weight={}", face.family, face.weight);
//! }
//! ```
//!
//! ## Architecture
//!
//! Each subcrate in the OxiFont ecosystem can be used independently. The facade
//! crate re-exports the most commonly needed items under a single dependency.
//!
//! - `oxifont-core` — core traits (`FontFace`, `FontCatalog`) and shared types (`FaceInfo`, `FontError`, `FontQuery`)
//! - `oxifont-parser` — TTF/OTF/TTC parsing (re-exported as [`parser`] module and top-level [`ParsedFace`])
//! - `oxifont-discovery` — filesystem font directory scanning (re-exported as [`discovery`] module, feature `discovery`)
//! - `oxifont-adapter-pure` — pure Rust font catalog from filesystem (re-exported as [`FontDatabase`], feature `pure`)
//! - `oxifont-adapter-native` — CoreText/DirectWrite system enumeration (re-exported as [`native`] module, feature `native`)
//! - `oxifont-db` — in-memory indexed font database with CSS matching (re-exported as [`db`] module, feature `db`)
//! - `oxifont-subset` — TrueType/CFF glyph subsetting (re-exported as [`subset`] module, feature `subset`)
//! - `oxifont-webfont` — WOFF1/WOFF2 encode and decode (re-exported as [`webfont`] module, features `woff1`/`woff2`)
//! - `oxifont-bundled` — compile-time embedded Noto fonts (re-exported as [`bundled`] module, feature `bundled-noto`)
//!
//! ## Core Traits vs Database Types
//!
//! [`FontFace`] is the core trait representing an individual font face with
//! outline data, metrics, and glyph access. It is implemented by [`ParsedFace`]
//! from `oxifont-parser`. Use [`load_font`] or [`load_font_bytes`] to obtain a
//! [`ParsedFace`] from a file path or raw bytes respectively.
//!
//! [`FontCatalog`] is the trait for a searchable collection of fonts. It is
//! implemented by [`FontDatabase`] (feature `pure`) for pure-Rust filesystem
//! scanning, and by [`db::FontDatabase`] (feature `db`) for the CSS-aware
//! in-memory index. The two databases are independent types; the `db` database
//! supports richer CSS Level 4 queries via [`db::Query`].
//!
//! [`FaceInfo`] is a lightweight descriptor holding the on-disk path, family
//! name, weight, and style of a font face. It does not hold the font bytes.
//! Use [`load_font`] or the catalog's `load_face` method to load the full
//! [`ParsedFace`] from a [`FaceInfo`]. Note that [`db::FaceInfo`] is a
//! parallel type defined by `oxifont-db` with an extended field set suited to
//! CSS matching; the two `FaceInfo` types are distinct.

// ---------------------------------------------------------------------------
// Core re-exports (unconditional)
// ---------------------------------------------------------------------------

pub use oxifont_core::{
    ColorGlyphFormat, FaceInfo, FontCatalog, FontError, FontFace, FontMetrics, FontQuery,
    FontStretch, FontStyle, GlyphOutline, KerningPair, VariationAxis,
};

pub use oxifont_parser::{face_count, ParsedFace};

// ---------------------------------------------------------------------------
// Re-export modules
// ---------------------------------------------------------------------------

/// Re-exports from `oxifont-parser`.
///
/// Provides the [`ParsedFace`] struct and its `ParsedFaceBuilder` for
/// constructing parsed font faces from raw bytes with optional face-index and
/// variation-axis configuration.
pub mod parser {
    pub use oxifont_parser::{ParsedFace, ParsedFaceBuilder};
}

/// Re-exports from `oxifont-discovery`.
///
/// Provides the OS font-directory scanner: `scan_dirs`, `scan_file`,
/// `system_font_dirs`, `user_font_dirs`, `ScanOptions`, and `ScanResult`.
///
/// Requires the `discovery` Cargo feature (enabled by default).
#[cfg(feature = "discovery")]
pub mod discovery {
    pub use oxifont_discovery::{
        scan_dirs, scan_file, system_font_dirs, user_font_dirs, ScanOptions, ScanResult,
    };
}

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "pure")]
pub use oxifont_adapter_pure::FontDatabase;

/// In-memory indexed font database and CSS Level 4 query engine.
///
/// Requires the `db` Cargo feature.
#[cfg(feature = "db")]
pub mod db {
    pub use oxifont_db::{DbError, FaceInfo, FontDatabase, Query, Source, VariationAxis};
    /// Deprecated: use [`VariationAxis`] instead.
    #[deprecated(note = "Use `VariationAxis` instead")]
    pub type VariableAxis = VariationAxis;
}

/// WOFF1 and WOFF2 webfont decoding.
///
/// Requires the `woff1` or `woff2` Cargo feature.
#[cfg(any(feature = "woff1", feature = "woff2"))]
pub mod webfont {
    pub use oxifont_webfont::*;
}

/// Native OS font catalog (CoreText on macOS; DirectWrite on Windows).
///
/// Requires the `native` Cargo feature.
#[cfg(feature = "native")]
pub mod native {
    pub use oxifont_adapter_native::NativeCatalog;
}

/// Build a native catalog from the OS font APIs.
///
/// On macOS this calls `CTFontCollectionCreateFromAvailableFonts`.
/// On Windows this calls `DWriteCreateFactory` → `IDWriteFontCollection`
/// enumeration via the `windows` 0.62 crate.
/// On other platforms this is a type alias for `FontDatabase::system()`.
///
/// Requires the `native` Cargo feature.
#[cfg(feature = "native")]
pub fn system_with_native() -> Result<native::NativeCatalog, FontError> {
    // `system()` is the uniform cross-platform constructor: the native
    // CoreText/DirectWrite backends mirror `oxifont_adapter_pure::FontDatabase::system()`
    // (the non-macOS/Windows alias for `NativeCatalog`) so this compiles on every
    // platform without `#[cfg]` guards.
    Ok(native::NativeCatalog::system()?)
}

/// Font subsetting (reduce a font to a glyph subset).
///
/// Requires the `subset` Cargo feature.
#[cfg(feature = "subset")]
pub mod subset {
    pub use oxifont_subset::*;
}

// ---------------------------------------------------------------------------
// system_fonts(): unified system font discovery returning FontDatabase
// ---------------------------------------------------------------------------

/// Return a [`db::FontDatabase`] populated from the best available system font
/// source.
///
/// On macOS when the `native` feature is enabled, the CoreText font collection
/// is enumerated and each discovered font file is loaded into the database.  If
/// the native adapter fails (e.g. on a minimal system), the function falls back
/// to the pure filesystem scan provided by the `pure` or default
/// `oxifont_db::FontDatabase::system` path.
///
/// On all other platforms (including Windows with DirectWrite disabled and
/// Linux), the function uses `oxifont_db::FontDatabase::system()` which scans
/// the OS-default font directories with `walkdir`.
///
/// # Errors
/// Returns [`FontError::ParseError`] if the underlying database scan returns an
/// error.  Individual font files that cannot be parsed are silently skipped by
/// both the native and pure backends; an error here indicates a systemic
/// failure (e.g. unable to determine the cache directory).
///
/// Requires the `db` Cargo feature and at least one of `native` or `pure`.
#[cfg(all(feature = "db", feature = "native", target_os = "macos"))]
pub fn system_fonts() -> Result<db::FontDatabase, FontError> {
    // Attempt to enumerate via CoreText first.
    match native::NativeCatalog::load() {
        Ok(catalog) => {
            use oxifont_core::FontCatalog as _;
            let mut database = db::FontDatabase::new();
            for face_info in catalog.faces() {
                // Each FaceInfo holds the on-disk path; load the file into the
                // indexed database.  Per-file errors are silently skipped so
                // that a single unreadable font does not abort the scan.
                let _ = database.load_file(&face_info.path);
            }
            Ok(database)
        }
        Err(_) => {
            // Native adapter failed — fall back to oxifont_db's own scan.
            db::FontDatabase::system().map_err(|e| FontError::ParseError(e.to_string()))
        }
    }
}

/// Return a [`db::FontDatabase`] populated from the pure filesystem scan.
///
/// This variant is used on non-macOS platforms (or when the `native` feature is
/// disabled) and simply delegates to `db::FontDatabase::system()`, which
/// scans the OS-default font directories with `walkdir`.
///
/// # Errors
/// Returns [`FontError::ParseError`] if the database scan fails.
///
/// Requires the `db` Cargo feature and at least one of `native` or `pure`.
#[cfg(all(
    feature = "db",
    any(feature = "native", feature = "pure"),
    not(all(feature = "native", target_os = "macos"))
))]
pub fn system_fonts() -> Result<db::FontDatabase, FontError> {
    db::FontDatabase::system().map_err(|e| FontError::ParseError(e.to_string()))
}

/// Bundled SIL-OFL-1.1 Noto font data.
///
/// Provides static byte slices for Noto fonts embedded at compile time.
/// Useful for environments without system fonts (embedded targets, CI, etc.).
///
/// Requires the `bundled-noto` (or any `bundled-noto-cjk-*`) Cargo feature.
#[cfg(feature = "bundled-noto")]
pub mod bundled {
    pub use oxifont_bundled::*;
}

/// Returns a `BundledFontProvider` pre-loaded with the SIL-OFL-1.1 Noto fonts
/// embedded at compile time.
///
/// This is the recommended entry point for environments without system font access
/// (e.g. WASM, sandboxed containers, CI pipelines).
///
/// # Example
/// ```no_run
/// use oxifont::FontError;
///
/// # fn main() -> Result<(), FontError> {
/// let provider = oxifont::system_with_bundled();
/// for (name, bytes) in provider.font_data() {
///     println!("{}: {} bytes", name, bytes.len());
/// }
/// # Ok(())
/// # }
/// ```
///
/// Requires the `bundled-noto` Cargo feature.
#[cfg(feature = "bundled-noto")]
pub fn system_with_bundled() -> bundled::provider::BundledFontProvider {
    bundled::provider::BundledFontProvider::new()
}

/// Returns the built-in bundled font catalog.
///
/// Useful as a fallback when system font discovery finds zero fonts (e.g. in
/// containerised environments without installed fonts) or whenever an
/// application needs a guaranteed minimal set of fonts regardless of system
/// state.
///
/// # Example
/// ```no_run
/// use oxifont_core::FontCatalog as _;
///
/// let catalog = oxifont::bundled_fonts();
/// for face in catalog.faces() {
///     println!("{} w{}", face.family, face.weight);
/// }
/// ```
///
/// Requires the `bundled-noto` Cargo feature.
#[cfg(feature = "bundled-noto")]
pub fn bundled_fonts() -> bundled::BundledCatalog {
    bundled::BundledCatalog::default()
}

/// Return a [`db::FontDatabase`] populated from system fonts, falling back to
/// the bundled Noto fonts when system discovery returns zero faces.
///
/// This is a convenience wrapper around [`system_fonts`] that ensures the
/// returned database is never empty in environments without installed fonts
/// (e.g. CI containers, WASM targets).
///
/// The bundled fonts are injected via [`db::FontDatabase::load_bytes`] so they
/// are queryable through the full CSS Level 4 matching API.
///
/// # Errors
/// Returns [`FontError::ParseError`] if the underlying system scan fails.
///
/// Requires the `db` and `bundled-noto` Cargo features.
#[cfg(all(feature = "db", feature = "bundled-noto"))]
pub fn system_fonts_with_bundled_fallback() -> Result<db::FontDatabase, FontError> {
    // system_fonts() is only defined when db + (native or pure) is active.
    // We replicate the same logic here rather than calling system_fonts()
    // to avoid a circular cfg dependency.
    #[cfg(all(feature = "native", target_os = "macos"))]
    let mut database = {
        match native::NativeCatalog::load() {
            Ok(catalog) => {
                use oxifont_core::FontCatalog as _;
                let mut db = db::FontDatabase::new();
                for face_info in catalog.faces() {
                    let _ = db.load_file(&face_info.path);
                }
                db
            }
            Err(_) => {
                db::FontDatabase::system().map_err(|e| FontError::ParseError(e.to_string()))?
            }
        }
    };
    #[cfg(not(all(feature = "native", target_os = "macos")))]
    let mut database =
        db::FontDatabase::system().map_err(|e| FontError::ParseError(e.to_string()))?;

    // If the database is empty, inject the bundled fonts.
    if database.stats().face_count == 0 {
        for font in bundled::all() {
            // decompressed_data() is always Ok when not using compressed storage.
            if let Ok(bytes) = font.decompressed_data() {
                database.load_bytes(bytes);
            }
        }
    }
    Ok(database)
}

// ---------------------------------------------------------------------------
// Convenience top-level functions
// ---------------------------------------------------------------------------

/// Load and parse a font from a file path.
///
/// For TTC collections, loads the first face (index 0). Use
/// [`ParsedFace::parse`] with a specific `face_index` for other sub-faces.
///
/// # Errors
/// Returns [`FontError::IoError`] if the file cannot be read, or
/// [`FontError::ParseError`] if the bytes are malformed.
///
/// # Example
/// ```no_run
/// let face = oxifont::load_font("/System/Library/Fonts/Helvetica.ttc").unwrap();
/// println!("{} weight={}", oxifont::FontFace::family_name(&face), oxifont::FontFace::weight(&face));
/// ```
pub fn load_font(path: impl AsRef<std::path::Path>) -> Result<ParsedFace, FontError> {
    let bytes = std::fs::read(path.as_ref())?;
    ParsedFace::parse(bytes, 0)
}

/// Load and parse a font from raw bytes.
///
/// # Errors
/// Returns [`FontError::ParseError`] if the bytes are malformed, or
/// [`FontError::IndexOutOfBounds`] if `face_index` exceeds the number of
/// faces in a TTC collection.
///
/// # Example
/// ```no_run
/// let data = std::fs::read("font.ttf").unwrap();
/// let face = oxifont::load_font_bytes(data, 0).unwrap();
/// ```
pub fn load_font_bytes(
    data: impl Into<std::sync::Arc<[u8]>>,
    face_index: u32,
) -> Result<ParsedFace, FontError> {
    ParsedFace::parse(data, face_index)
}

/// Detected font container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFormat {
    /// TrueType font (`.ttf`, magic `0x00010000`).
    TrueType,
    /// OpenType font with CFF outlines (`.otf`, magic `OTTO`).
    OpenType,
    /// TrueType collection (`.ttc`, magic `ttcf`).
    TrueTypeCollection,
    /// WOFF version 1 (magic `wOFF`).
    Woff1,
    /// WOFF version 2 (magic `wOF2`).
    Woff2,
    /// Unknown format.
    Unknown,
}

impl std::fmt::Display for FontFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::TrueType => "TrueType",
            Self::OpenType => "OpenType (CFF)",
            Self::TrueTypeCollection => "TrueType Collection",
            Self::Woff1 => "WOFF1",
            Self::Woff2 => "WOFF2",
            Self::Unknown => "Unknown",
        };
        write!(f, "{name}")
    }
}

/// Detect the font format from the first 4 bytes of data.
///
/// # Example
/// ```
/// let data = b"\x00\x01\x00\x00rest of font...";
/// assert_eq!(oxifont::detect_format(data), oxifont::FontFormat::TrueType);
/// ```
pub fn detect_format(data: &[u8]) -> FontFormat {
    if data.len() < 4 {
        return FontFormat::Unknown;
    }
    let magic = &data[..4];
    match magic {
        [0x00, 0x01, 0x00, 0x00] => FontFormat::TrueType,
        b"OTTO" => FontFormat::OpenType,
        b"ttcf" => FontFormat::TrueTypeCollection,
        b"wOFF" => FontFormat::Woff1,
        b"wOF2" => FontFormat::Woff2,
        _ => FontFormat::Unknown,
    }
}

/// Detect the format, decode if necessary (WOFF1/2), then parse into a
/// [`ParsedFace`].
///
/// Supports TTF, OTF, TTC (face index 0), WOFF1 (requires `woff1` feature),
/// and WOFF2 (requires `woff2` feature).
///
/// # Errors
/// Returns [`FontError`] on unsupported format, decode failure, or parse
/// error.
///
/// # Example
/// ```no_run
/// let data = std::fs::read("font.woff2").unwrap();
/// let face = oxifont::decode_and_parse(&data).unwrap();
/// ```
pub fn decode_and_parse(data: &[u8]) -> Result<ParsedFace, FontError> {
    match detect_format(data) {
        FontFormat::TrueType | FontFormat::OpenType | FontFormat::TrueTypeCollection => {
            ParsedFace::parse(data.to_vec(), 0)
        }
        #[cfg(feature = "woff1")]
        FontFormat::Woff1 => {
            let sfnt = oxifont_webfont::decode_woff1(data)
                .map_err(|e| FontError::ParseError(e.to_string()))?;
            ParsedFace::parse(sfnt, 0)
        }
        #[cfg(not(feature = "woff1"))]
        FontFormat::Woff1 => Err(FontError::UnsupportedFormat),
        #[cfg(feature = "woff2")]
        FontFormat::Woff2 => {
            let sfnt = oxifont_webfont::decode_woff2(data)
                .map_err(|e| FontError::ParseError(e.to_string()))?;
            ParsedFace::parse(sfnt, 0)
        }
        #[cfg(not(feature = "woff2"))]
        FontFormat::Woff2 => Err(FontError::UnsupportedFormat),
        FontFormat::Unknown => Err(FontError::UnsupportedFormat),
    }
}

// ---------------------------------------------------------------------------
// Subset + WOFF2 encode pipeline
// ---------------------------------------------------------------------------

/// Error returned by [`subset_and_encode_woff2`].
///
/// Wraps either a subsetting failure or a WOFF2 encoding failure.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new variants can be added in minor versions.
///
/// Requires the `subset` and `woff2` Cargo features.
#[cfg(all(feature = "subset", feature = "woff2"))]
#[derive(Debug)]
#[non_exhaustive]
pub enum SubsetEncodeError {
    /// The subsetting step failed.
    Subset(oxifont_subset::SubsetError),
    /// The WOFF2 encoding step failed.
    Encode(oxifont_webfont::WebFontError),
}

#[cfg(all(feature = "subset", feature = "woff2"))]
impl std::fmt::Display for SubsetEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subset(e) => write!(f, "subset error: {e}"),
            Self::Encode(e) => write!(f, "woff2 encode error: {e}"),
        }
    }
}

#[cfg(all(feature = "subset", feature = "woff2"))]
impl std::error::Error for SubsetEncodeError {}

#[cfg(all(feature = "subset", feature = "woff2"))]
impl From<oxifont_subset::SubsetError> for SubsetEncodeError {
    fn from(e: oxifont_subset::SubsetError) -> Self {
        Self::Subset(e)
    }
}

#[cfg(all(feature = "subset", feature = "woff2"))]
impl From<oxifont_webfont::WebFontError> for SubsetEncodeError {
    fn from(e: oxifont_webfont::WebFontError) -> Self {
        Self::Encode(e)
    }
}

/// Subset a font to the given codepoints, then encode the result as WOFF2.
///
/// This is a thin composition of:
/// 1. [`oxifont_subset::subset_font`] — reduces the SFNT to only the requested
///    glyphs (plus `.notdef`).
/// 2. [`oxifont_webfont::encode_woff2`] — encodes the resulting SFNT as a
///    standards-compliant WOFF2 file.
///
/// # Errors
/// Returns [`SubsetEncodeError::Subset`] if the font data is structurally
/// invalid or a required table is absent, or [`SubsetEncodeError::Encode`] if
/// WOFF2 encoding fails.
///
/// # Example
/// ```no_run
/// use std::collections::BTreeSet;
///
/// let font_data = std::fs::read("NotoSans-Regular.ttf").unwrap();
/// let codepoints: BTreeSet<char> = "Hello".chars().collect();
/// let woff2 = oxifont::subset_and_encode_woff2(&font_data, &codepoints)
///     .expect("subset + encode failed");
/// assert_eq!(&woff2[0..4], b"wOF2");
/// ```
///
/// Requires the `subset` and `woff2` Cargo features.
#[cfg(all(feature = "subset", feature = "woff2"))]
pub fn subset_and_encode_woff2(
    font_data: &[u8],
    codepoints: &std::collections::BTreeSet<char>,
) -> Result<Vec<u8>, SubsetEncodeError> {
    let sfnt = oxifont_subset::subset_font(font_data, codepoints)?;
    let woff2 = oxifont_webfont::encode_woff2(&sfnt)?;
    Ok(woff2)
}

/// The prelude module re-exports the most commonly used types and traits.
///
/// # Usage
/// ```no_run
/// use oxifont::prelude::*;
/// ```
pub mod prelude {
    pub use oxifont_core::{
        ColorGlyphFormat, FaceInfo, FontCatalog, FontError, FontFace, FontMetrics, FontQuery,
        FontStretch, FontStyle, GlyphOutline, VariationAxis,
    };
    pub use oxifont_parser::ParsedFace;
}

/// Returns the version string of the `oxifont` crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
