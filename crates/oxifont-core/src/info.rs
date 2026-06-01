//! Font face metadata and query types: `FaceInfo` and `FontQuery`.

extern crate alloc;

#[cfg(feature = "std")]
use std::path::PathBuf;

use crate::types::{FontStretch, FontStyle};

/// Lightweight metadata about a single font face stored on disk.
///
/// Does **not** hold parsed glyph data — use [`crate::FontFace`] implementations
/// for that. `FaceInfo` is cheap to clone and suitable for building indices.
///
/// Note: the `path` field requires `std` feature (uses `std::path::PathBuf`).
/// Full no_std compliance for `FaceInfo` is deferred.
///
/// # Example
/// ```
/// use oxifont_core::{FaceInfo, FontStyle, FontStretch};
/// use std::path::PathBuf;
/// let info = FaceInfo {
///     family: std::sync::Arc::from("Roboto"),
///     post_script_name: "Roboto-Regular".to_string(),
///     style: FontStyle::Normal,
///     weight: 400,
///     stretch: FontStretch::Normal,
///     path: PathBuf::from("/usr/share/fonts/Roboto-Regular.ttf"),
///     face_index: 0,
///     localized_families: vec![],
/// };
/// assert_eq!(&*info.family, "Roboto");
/// assert_eq!(info.weight, 400);
/// ```
#[derive(Debug, Clone, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FaceInfo {
    /// The typographic family name (e.g. `"Helvetica Neue"`).
    pub family: alloc::sync::Arc<str>,
    /// PostScript name (e.g. `"HelveticaNeue-Bold"`). Empty string when
    /// unavailable.
    pub post_script_name: alloc::string::String,
    /// Italic / oblique / normal classification.
    pub style: FontStyle,
    /// CSS-style weight number (100–900, 400 = Regular).
    pub weight: u16,
    /// CSS font-stretch classification.
    pub stretch: FontStretch,
    /// Absolute path to the font file on disk.
    #[cfg(feature = "std")]
    pub path: PathBuf,
    /// Zero-based index within a TTC collection; always 0 for TTF/OTF.
    pub face_index: u32,
    /// All localized family name strings for this face (from the OS name
    /// table or native font API). May include names in multiple locales.
    /// Empty when the native adapter does not populate it (e.g. pure adapter).
    pub localized_families: alloc::vec::Vec<alloc::string::String>,
}

/// A builder-style query for matching a [`FaceInfo`] inside a [`crate::FontCatalog`].
///
/// All fields are optional; unset fields are treated as wildcards.
///
/// # Example
/// ```
/// use oxifont_core::{FontQuery, FontStyle, FontStretch};
/// let q = FontQuery::new()
///     .family("Arial")
///     .style(FontStyle::Italic)
///     .weight(700)
///     .stretch(FontStretch::Normal);
/// assert_eq!(q.family.as_deref(), Some("Arial"));
/// assert_eq!(q.style, Some(FontStyle::Italic));
/// assert_eq!(q.weight, Some(700));
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FontQuery {
    /// Desired family name (case-insensitive substring match).
    pub family: Option<alloc::string::String>,
    /// Desired style.
    pub style: Option<FontStyle>,
    /// Desired CSS weight (exact match).
    pub weight: Option<u16>,
    /// Desired CSS font-stretch.
    pub stretch: Option<FontStretch>,
    /// Desired PostScript name (name ID 6, exact match).
    pub postscript_name: Option<alloc::string::String>,
}

impl FontQuery {
    /// Creates an empty query (matches everything).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontQuery;
    /// let q = FontQuery::new();
    /// assert!(q.family.is_none());
    /// assert!(q.style.is_none());
    /// assert!(q.weight.is_none());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Constrains the query to a specific family name.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontQuery;
    /// let q = FontQuery::new().family("Arial");
    /// assert_eq!(q.family.as_deref(), Some("Arial"));
    /// ```
    pub fn family(mut self, f: impl Into<alloc::string::String>) -> Self {
        self.family = Some(f.into());
        self
    }

    /// Constrains the query to a specific style.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontQuery, FontStyle};
    /// let q = FontQuery::new().style(FontStyle::Italic);
    /// assert_eq!(q.style, Some(FontStyle::Italic));
    /// ```
    pub fn style(mut self, s: FontStyle) -> Self {
        self.style = Some(s);
        self
    }

    /// Constrains the query to a specific CSS weight.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontQuery;
    /// let q = FontQuery::new().weight(700);
    /// assert_eq!(q.weight, Some(700));
    /// ```
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = Some(w);
        self
    }

    /// Constrains the query to a specific CSS font-stretch.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontQuery, FontStretch};
    /// let q = FontQuery::new().stretch(FontStretch::Condensed);
    /// assert_eq!(q.stretch, Some(FontStretch::Condensed));
    /// ```
    pub fn stretch(mut self, s: FontStretch) -> Self {
        self.stretch = Some(s);
        self
    }

    /// Constrains the query to a specific PostScript name (name ID 6).
    ///
    /// This is an exact-match filter. PostScript names are ASCII and typically
    /// look like `"Arial-BoldMT"` or `"Helvetica-Oblique"`.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontQuery;
    /// let q = FontQuery::new().postscript_name("Arial-BoldMT");
    /// assert_eq!(q.postscript_name.as_deref(), Some("Arial-BoldMT"));
    /// ```
    pub fn postscript_name(mut self, name: impl Into<alloc::string::String>) -> Self {
        self.postscript_name = Some(name.into());
        self
    }
}
