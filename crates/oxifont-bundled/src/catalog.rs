//! [`BundledFont`] and [`BundledCatalog`] — typed access to embedded Noto fonts.
//!
//! `BundledFont` is a zero-copy descriptor that holds all per-font metadata and
//! a `'static` byte slice pointing directly into the compiled binary.
//!
//! `BundledCatalog` implements the [`FontCatalog`] trait over a snapshot of
//! bundled fonts. Its `faces()` slice is built once at construction time so
//! queries are O(n) over a `Vec<FaceInfo>`.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use oxifont_core::{FaceInfo, FontCatalog, FontError, FontQuery, FontStretch, FontStyle};
use oxifont_parser::ParsedFace;

// ── BundledFont ───────────────────────────────────────────────────────────────

/// A statically embedded font identified by family, weight, and style.
///
/// `BundledFont` holds a `'static` byte slice pointing into the compiled
/// binary alongside lightweight metadata. It is `Clone` (all fields are
/// either primitive or `'static` references). Cloning resets the parsed-face
/// cache to empty — the `pub static` constants are the canonical instances
/// that share a long-lived cache.
///
/// # Example
/// ```ignore
/// // Requires --features bundled-noto
/// use oxifont_bundled::SANS_REGULAR;
/// assert_eq!(SANS_REGULAR.family_name(), "Noto Sans");
/// assert_eq!(SANS_REGULAR.weight(), 400);
/// ```
pub struct BundledFont {
    /// Typographic family name (e.g. `"Noto Sans"`).
    pub family: &'static str,
    /// PostScript name (e.g. `"NotoSans-Regular"`).
    pub postscript_name: &'static str,
    /// Raw font bytes embedded in the binary via `include_bytes!`.
    pub data: &'static [u8],
    /// CSS weight (100–900).
    pub weight: u16,
    /// Style classification.
    pub style: FontStyle,
    /// Width classification.
    pub stretch: FontStretch,
    /// Whether all glyphs share the same advance width.
    pub is_monospace: bool,
    /// Lazily-parsed face, initialised once on first call to [`parsed_face`](Self::parsed_face).
    ///
    /// The `OnceLock` is `const`-initialised to empty, so it is compatible with
    /// `pub static` declarations. Cloning a `BundledFont` resets the cache.
    pub parsed: OnceLock<Arc<ParsedFace>>,
}

impl Clone for BundledFont {
    /// Clone the descriptor metadata; the parsed-face cache is **not** copied.
    ///
    /// The cloned value starts with an empty cache and will re-parse on the
    /// first call to [`parsed_face`](Self::parsed_face). This is intentional:
    /// the `pub static` constants are the canonical cached instances.
    fn clone(&self) -> Self {
        Self {
            family: self.family,
            postscript_name: self.postscript_name,
            data: self.data,
            weight: self.weight,
            style: self.style.clone(),
            stretch: self.stretch,
            is_monospace: self.is_monospace,
            parsed: OnceLock::new(),
        }
    }
}

impl std::fmt::Debug for BundledFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundledFont")
            .field("family", &self.family)
            .field("postscript_name", &self.postscript_name)
            .field("weight", &self.weight)
            .field("style", &self.style)
            .field("stretch", &self.stretch)
            .field("is_monospace", &self.is_monospace)
            .field("data_len", &self.data.len())
            .field("parsed_cached", &self.parsed.get().is_some())
            .finish()
    }
}

impl BundledFont {
    /// Return the typographic family name.
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// assert_eq!(SANS_REGULAR.family_name(), "Noto Sans");
    /// ```
    #[inline]
    pub fn family_name(&self) -> &'static str {
        self.family
    }

    /// Return the CSS weight (100–900).
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// assert_eq!(SANS_REGULAR.weight(), 400);
    /// ```
    #[inline]
    pub fn weight(&self) -> u16 {
        self.weight
    }

    /// Return the style classification.
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// use oxifont_core::FontStyle;
    /// assert_eq!(SANS_REGULAR.style(), FontStyle::Normal);
    /// ```
    #[inline]
    pub fn style(&self) -> FontStyle {
        self.style.clone()
    }

    /// Return the raw font bytes.
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// assert!(!SANS_REGULAR.data().is_empty());
    /// ```
    #[inline]
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// Decompress and return the raw font bytes as an owned `Vec<u8>`.
    ///
    /// When the `compressed` feature is enabled and the bundled data is stored
    /// as zlib/DEFLATE-compressed bytes, this decompresses on the fly.
    /// When the `compressed` feature is disabled (or the build script has not
    /// yet been implemented), this is equivalent to `self.data.to_vec()`.
    ///
    /// # Errors
    /// Returns [`FontError::ParseError`] if decompression fails (only relevant
    /// when the `compressed` feature is enabled and data is actually compressed).
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// let bytes = SANS_REGULAR.decompressed_data().unwrap();
    /// assert!(!bytes.is_empty());
    /// ```
    pub fn decompressed_data(&self) -> Result<Vec<u8>, FontError> {
        crate::compressed::decompress_font(self.data)
    }

    /// Parse this font's bytes into a [`ParsedFace`].
    ///
    /// # Errors
    /// Returns [`FontError::ParseError`] if the embedded bytes are not a valid
    /// TTF/OTF font. This should not happen for the constants shipped by this
    /// crate but can occur with synthetic test entries that carry placeholder
    /// bytes.
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use oxifont_bundled::SANS_REGULAR;
    /// use oxifont_core::FontFace as _;
    /// let face = SANS_REGULAR.parse().expect("Noto Sans Regular must parse");
    /// assert!(!face.family_name().is_empty());
    /// ```
    pub fn parse(&self) -> Result<ParsedFace, FontError> {
        ParsedFace::parse(self.data, 0).map_err(|e| FontError::ParseError(e.to_string()))
    }

    /// Return a lazily-parsed [`ParsedFace`] wrapped in an [`Arc`], cached for
    /// the lifetime of this static instance.
    ///
    /// On the first call the font bytes are parsed into a `ParsedFace` and the
    /// result is stored inside this descriptor's [`OnceLock`]. Subsequent calls
    /// clone the cached `Arc` without re-parsing.
    ///
    /// Because the bundled font data is always valid (it is compiled into the
    /// binary via `include_bytes!`), a parse failure is treated as a panic
    /// rather than propagated as a `Result`. If you need graceful error handling
    /// for untrusted data, use [`parse`](Self::parse) instead.
    ///
    /// # Errors
    /// Returns [`FontError::ParseError`] only when this `BundledFont` was
    /// constructed with deliberately invalid bytes (e.g. in unit tests).
    ///
    /// # Example
    /// ```ignore
    /// // Requires --features bundled-noto
    /// use std::sync::Arc;
    /// use oxifont_bundled::catalog::SANS_REGULAR;
    ///
    /// let a = SANS_REGULAR.parsed_face().expect("must parse");
    /// let b = SANS_REGULAR.parsed_face().expect("must parse");
    /// assert!(Arc::ptr_eq(&a, &b), "same Arc returned on repeated call");
    /// ```
    pub fn parsed_face(&self) -> Result<Arc<ParsedFace>, FontError> {
        let arc = self.parsed.get_or_init(|| {
            let face = ParsedFace::parse(self.data, 0)
                .unwrap_or_else(|e| panic!("bundled font parse failed: {e}"));
            Arc::new(face)
        });
        Ok(arc.clone())
    }

    /// Convert this descriptor into a [`FaceInfo`] record.
    ///
    /// The `path` field is filled with a synthetic sentinel that identifies the
    /// font as a bundled resource. The path does **not** point to a real file on
    /// disk; it is provided solely so downstream code that relies on `FaceInfo`
    /// (e.g. catalog serialisation) has a stable, human-readable identifier.
    pub(crate) fn to_face_info(&self) -> FaceInfo {
        FaceInfo {
            family: Arc::from(self.family),
            post_script_name: self.postscript_name.to_owned(),
            style: self.style.clone(),
            weight: self.weight,
            stretch: self.stretch,
            path: PathBuf::from(format!("<bundled>/{}.ttf", self.postscript_name)),
            face_index: 0,
            localized_families: vec![],
        }
    }
}

// ── BundledCatalog ────────────────────────────────────────────────────────────

/// A [`FontCatalog`] backed entirely by statically embedded Noto fonts.
///
/// `BundledCatalog` pre-builds a `Vec<FaceInfo>` at construction time so that
/// [`FontCatalog::faces`] can return a proper slice and [`FontCatalog::find`]
/// can do meaningful matches.
///
/// Internally the catalog holds a `&'static [&'static BundledFont]` to avoid
/// copying the font structs — each entry is a reference to a static descriptor.
///
/// # Example
/// ```
/// use oxifont_bundled::BundledCatalog;
/// use oxifont_core::FontCatalog as _;
///
/// let catalog = BundledCatalog::default();
/// // Every bundled font shows up in faces().
/// for face in catalog.faces() {
///     println!("{} w{}", face.family, face.weight);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BundledCatalog {
    /// Pre-built `FaceInfo` records derived from the bundled font descriptors.
    faces: Vec<FaceInfo>,
    /// Static references to the bundled font descriptors.
    fonts: &'static [&'static BundledFont],
}

impl BundledCatalog {
    /// Build a catalog from a static slice of [`BundledFont`] references.
    ///
    /// All entries in `fonts` become available through [`FontCatalog::faces`]
    /// and [`FontCatalog::find`].
    ///
    /// # Example
    /// ```
    /// use oxifont_bundled::{BundledCatalog, ALL_FONT_REFS};
    ///
    /// let catalog = BundledCatalog::new(ALL_FONT_REFS);
    /// assert_eq!(catalog.fonts().len(), ALL_FONT_REFS.len());
    /// ```
    pub fn new(fonts: &'static [&'static BundledFont]) -> Self {
        let faces = fonts.iter().map(|f| f.to_face_info()).collect();
        Self { faces, fonts }
    }

    /// Return the underlying static references to [`BundledFont`] descriptors.
    ///
    /// # Example
    /// ```
    /// use oxifont_bundled::{BundledCatalog, ALL_FONT_REFS};
    ///
    /// let catalog = BundledCatalog::default();
    /// assert_eq!(catalog.fonts().len(), ALL_FONT_REFS.len());
    /// ```
    pub fn fonts(&self) -> &'static [&'static BundledFont] {
        self.fonts
    }

    /// Find the first bundled font whose family name matches `family`
    /// (case-insensitive).
    ///
    /// # Example
    /// ```
    /// use oxifont_bundled::BundledCatalog;
    ///
    /// let catalog = BundledCatalog::default();
    /// // Works regardless of whether bundled-noto feature is active.
    /// let _ = catalog.find_by_family("Noto Sans");
    /// let _ = catalog.find_by_family("noto sans");
    /// ```
    pub fn find_by_family(&self, family: &str) -> Option<&'static BundledFont> {
        let needle = family.to_lowercase();
        self.fonts
            .iter()
            .copied()
            .find(|f| f.family.to_lowercase() == needle)
    }

    /// Iterate over all bundled fonts whose family name matches `family`
    /// (case-insensitive).
    ///
    /// # Example
    /// ```
    /// use oxifont_bundled::BundledCatalog;
    ///
    /// let catalog = BundledCatalog::default();
    /// let count = catalog.fonts_by_family("Noto Sans").count();
    /// println!("Noto Sans variants: {}", count);
    /// ```
    pub fn fonts_by_family(&self, family: &str) -> impl Iterator<Item = &'static BundledFont> + '_ {
        let needle = family.to_lowercase();
        self.fonts
            .iter()
            .copied()
            .filter(move |f| f.family.to_lowercase() == needle)
    }
}

impl Default for BundledCatalog {
    /// Create a catalog from all compiled-in bundled fonts (those enabled by
    /// feature flags).
    fn default() -> Self {
        Self::new(ALL_FONT_REFS)
    }
}

impl FontCatalog for BundledCatalog {
    /// Returns the pre-built slice of [`FaceInfo`] records for all bundled fonts.
    fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// Find the first face whose family, weight, style, and stretch match `query`.
    ///
    /// All specified fields in `query` must match (logical AND). Unset fields
    /// are wildcards. Family matching is case-insensitive.
    fn find(&self, query: &FontQuery) -> Option<&FaceInfo> {
        self.faces.iter().find(|info| {
            query
                .family
                .as_deref()
                .is_none_or(|f| info.family.to_lowercase() == f.to_lowercase())
                && query.weight.is_none_or(|w| info.weight == w)
                && query.style.as_ref().is_none_or(|s| &info.style == s)
                && query.stretch.as_ref().is_none_or(|st| &info.stretch == st)
                && query
                    .postscript_name
                    .as_deref()
                    .is_none_or(|ps| info.post_script_name.to_lowercase() == ps.to_lowercase())
        })
    }
}

// ── Bundled font constants ────────────────────────────────────────────────────

/// Noto Sans Regular — proportional sans-serif, weight 400, normal style.
///
/// Licensed under the SIL Open Font License 1.1.
/// Bytes embedded from `../fonts/NotoSans-Regular.ttf`.
#[cfg(feature = "bundled-noto")]
pub static SANS_REGULAR: BundledFont = BundledFont {
    family: "Noto Sans",
    postscript_name: "NotoSans-Regular",
    data: include_bytes!("../fonts/NotoSans-Regular.ttf"),
    weight: 400,
    style: FontStyle::Normal,
    stretch: FontStretch::Normal,
    is_monospace: false,
    parsed: OnceLock::new(),
};

/// Noto Sans Bold — proportional sans-serif, weight 700, normal style.
///
/// Licensed under the SIL Open Font License 1.1.
/// Bytes embedded from `../fonts/NotoSans-Bold.ttf`.
#[cfg(feature = "bundled-noto")]
pub static SANS_BOLD: BundledFont = BundledFont {
    family: "Noto Sans",
    postscript_name: "NotoSans-Bold",
    data: include_bytes!("../fonts/NotoSans-Bold.ttf"),
    weight: 700,
    style: FontStyle::Normal,
    stretch: FontStretch::Normal,
    is_monospace: false,
    parsed: OnceLock::new(),
};

/// Noto Serif Regular — proportional serif, weight 400, normal style.
///
/// Licensed under the SIL Open Font License 1.1.
/// Bytes embedded from `../fonts/NotoSerif-Regular.ttf`.
#[cfg(feature = "bundled-noto")]
pub static SERIF_REGULAR: BundledFont = BundledFont {
    family: "Noto Serif",
    postscript_name: "NotoSerif-Regular",
    data: include_bytes!("../fonts/NotoSerif-Regular.ttf"),
    weight: 400,
    style: FontStyle::Normal,
    stretch: FontStretch::Normal,
    is_monospace: false,
    parsed: OnceLock::new(),
};

/// Noto Sans Italic — proportional sans-serif italic, weight 400, italic style.
///
/// Variable-font TTF (weight and width axes) sourced from the Google Fonts
/// repository. At face index 0 it resolves to weight 400, italic style.
///
/// Licensed under the SIL Open Font License 1.1.
/// Bytes embedded from `../fonts/NotoSans-Italic.ttf`.
#[cfg(feature = "bundled-noto")]
pub static SANS_ITALIC: BundledFont = BundledFont {
    family: "Noto Sans",
    postscript_name: "NotoSans-Italic",
    data: include_bytes!("../fonts/NotoSans-Italic.ttf"),
    weight: 400,
    style: FontStyle::Italic,
    stretch: FontStretch::Normal,
    is_monospace: false,
    parsed: OnceLock::new(),
};

/// Noto Sans Mono Regular — monospace sans-serif, weight 400, normal style.
///
/// Variable-font TTF (weight and width axes) sourced from the Google Fonts
/// repository. At face index 0 it resolves to weight 400, normal style, monospace.
///
/// Note: ttf_parser's `is_monospaced()` flag may return `false` for this
/// variable font despite it being a true monospace face; use the PostScript
/// name (`NotoSansMono-Regular`) or family name (`Noto Sans Mono`) to
/// identify this face programmatically.
///
/// Licensed under the SIL Open Font License 1.1.
/// Bytes embedded from `../fonts/NotoSansMono-Regular.ttf`.
#[cfg(feature = "bundled-noto")]
pub static MONO_REGULAR: BundledFont = BundledFont {
    family: "Noto Sans Mono",
    postscript_name: "NotoSansMono-Regular",
    data: include_bytes!("../fonts/NotoSansMono-Regular.ttf"),
    weight: 400,
    style: FontStyle::Normal,
    stretch: FontStretch::Normal,
    is_monospace: true,
    parsed: OnceLock::new(),
};

/// Static references to all bundled font descriptors enabled at compile time.
///
/// This slice is empty when no font feature flags are active. Use [`all`] as
/// a convenience wrapper that returns this slice.
pub static ALL_FONT_REFS: &[&BundledFont] = &[
    #[cfg(feature = "bundled-noto")]
    &SANS_REGULAR,
    #[cfg(feature = "bundled-noto")]
    &SANS_BOLD,
    #[cfg(feature = "bundled-noto")]
    &SERIF_REGULAR,
    #[cfg(feature = "bundled-noto")]
    &SANS_ITALIC,
    #[cfg(feature = "bundled-noto")]
    &MONO_REGULAR,
];

/// Return all bundled fonts compiled into this binary as a slice of references.
///
/// Equivalent to `ALL_FONT_REFS`.
///
/// # Example
/// ```
/// use oxifont_bundled::all;
///
/// for font in all() {
///     println!("{} w{}", font.family_name(), font.weight());
/// }
/// ```
pub fn all() -> &'static [&'static BundledFont] {
    ALL_FONT_REFS
}
