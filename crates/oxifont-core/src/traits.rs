//! Core font traits: `FontFace`, `FontCatalog`, `NameTable`, `FontCapabilities`,
//! and `FontCollection`.

extern crate alloc;

use crate::axis::VariationAxis;
use crate::error::FontError;
use crate::info::{FaceInfo, FontQuery};
use crate::types::{ColorGlyphFormat, FontMetrics, FontStretch, FontStyle, GlyphOutline};

/// Parsed font face — provides metric and glyph queries.
///
/// Implementors hold parsed font data in memory and answer metric queries
/// without returning lifetimed references, making them easy to use across
/// thread and async boundaries.
///
/// # Example
/// ```
/// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis, GlyphOutline, FontMetrics};
///
/// struct DummyFace;
///
/// impl FontFace for DummyFace {
///     fn family_name(&self) -> &str { "DummyFont" }
///     fn style(&self) -> FontStyle { FontStyle::Normal }
///     fn weight(&self) -> u16 { 400 }
///     fn is_monospace(&self) -> bool { false }
///     fn units_per_em(&self) -> u16 { 1000 }
///     fn glyph_for_char(&self, _c: char) -> Option<u16> { Some(1) }
///     fn advance_width(&self, _gid: u16) -> Option<u16> { Some(600) }
///     fn axes(&self) -> &[VariationAxis] { &[] }
/// }
///
/// let face = DummyFace;
/// assert_eq!(face.family_name(), "DummyFont");
/// assert_eq!(face.weight(), 400);
/// assert_eq!(face.stretch(), FontStretch::Normal);
/// assert!(!face.has_color_glyphs());
/// assert_eq!(face.glyph_count(), 0);
/// ```
pub trait FontFace {
    /// Returns the typographic family name.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "TestFamily" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.family_name(), "TestFamily");
    /// ```
    fn family_name(&self) -> &str;
    /// Returns the style classification.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Italic }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.style(), FontStyle::Italic);
    /// ```
    fn style(&self) -> FontStyle;
    /// Returns the CSS weight (100–900).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 700 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.weight(), 700);
    /// ```
    fn weight(&self) -> u16;
    /// Returns the CSS font-stretch classification.
    ///
    /// The default implementation returns [`FontStretch::Normal`].
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default is Normal (not overridden here)
    /// assert_eq!(F.stretch(), FontStretch::Normal);
    /// ```
    fn stretch(&self) -> FontStretch {
        FontStretch::Normal
    }
    /// Returns `true` when all glyphs have identical advance widths.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct MonoF;
    /// impl FontFace for MonoF {
    ///     fn family_name(&self) -> &str { "Mono" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { true }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { Some(600) }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert!(MonoF.is_monospace());
    /// ```
    fn is_monospace(&self) -> bool;
    /// Returns the design units per EM (typically 1000 or 2048).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 2048 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.units_per_em(), 2048);
    /// ```
    fn units_per_em(&self) -> u16;
    /// Resolves a Unicode code point to a glyph ID, or `None` if unmapped.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, c: char) -> Option<u16> {
    ///         if c == 'A' { Some(36) } else { None }
    ///     }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.glyph_for_char('A'), Some(36));
    /// assert_eq!(F.glyph_for_char('☃'), None);
    /// ```
    fn glyph_for_char(&self, c: char) -> Option<u16>;
    /// Returns the horizontal advance width for a glyph ID, in design units.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, gid: u16) -> Option<u16> {
    ///         if gid == 36 { Some(611) } else { None }
    ///     }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert_eq!(F.advance_width(36), Some(611));
    /// assert_eq!(F.advance_width(9999), None);
    /// ```
    fn advance_width(&self, gid: u16) -> Option<u16>;
    /// Returns the variable-font axes from the `fvar` table.
    ///
    /// Returns an empty slice for non-variable fonts.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// assert!(F.axes().is_empty());
    /// ```
    fn axes(&self) -> &[VariationAxis];

    /// Returns font-wide metrics (ascender, descender, line gap, etc.).
    ///
    /// Default implementation returns `None`. Implementors should override
    /// this to extract metrics from the OS/2, hhea, and post tables.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns None
    /// assert!(F.metrics().is_none());
    /// ```
    fn metrics(&self) -> Option<FontMetrics> {
        None
    }

    /// Extracts the outline of the glyph at `gid` as a series of path commands.
    ///
    /// Returns `None` if the glyph has no outline (e.g. a space character)
    /// or if outline extraction is not supported.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns None
    /// assert!(F.outline(0).is_none());
    /// ```
    fn outline(&self, _gid: u16) -> Option<alloc::vec::Vec<GlyphOutline>> {
        None
    }

    /// Returns the horizontal kerning adjustment between two glyphs, in
    /// design units (negative = tighter).
    ///
    /// Checks the `kern` table and/or GPOS PairPos lookups.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns None
    /// assert!(F.kern(0, 1).is_none());
    /// ```
    fn kern(&self, _left_gid: u16, _right_gid: u16) -> Option<i16> {
        None
    }

    /// Returns the total number of glyphs in the font (from the `maxp` table).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns 0
    /// assert_eq!(F.glyph_count(), 0);
    /// ```
    fn glyph_count(&self) -> u16 {
        0
    }

    /// Returns the type of color glyph data present, if any.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns None
    /// assert!(F.color_glyph_format().is_none());
    /// ```
    fn color_glyph_format(&self) -> Option<ColorGlyphFormat> {
        None
    }

    /// Returns `true` when the font has color glyph data (COLR, CBDT, sbix,
    /// or SVG).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis, ColorGlyphFormat};
    /// struct ColorF;
    /// impl FontFace for ColorF {
    ///     fn family_name(&self) -> &str { "EmojiFont" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 2048 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { Some(1) }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { Some(2048) }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    ///     fn color_glyph_format(&self) -> Option<ColorGlyphFormat> { Some(ColorGlyphFormat::ColrV1) }
    /// }
    /// assert!(ColorF.has_color_glyphs());
    /// ```
    fn has_color_glyphs(&self) -> bool {
        self.color_glyph_format().is_some()
    }

    /// Returns the PostScript name (name ID 6) if available.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    ///     fn postscript_name(&self) -> Option<&str> { Some("Test-Regular") }
    /// }
    /// assert_eq!(F.postscript_name(), Some("Test-Regular"));
    /// ```
    fn postscript_name(&self) -> Option<&str> {
        None
    }

    /// Returns `true` if the font contains a table with the given 4-byte tag.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation always returns false
    /// assert!(!F.has_table(*b"GSUB"));
    /// ```
    fn has_table(&self, _tag: [u8; 4]) -> bool {
        false
    }

    /// Returns the vertical advance height for a glyph ID, in design units.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontFace, FontStyle, FontStretch, VariationAxis};
    /// struct F;
    /// impl FontFace for F {
    ///     fn family_name(&self) -> &str { "Test" }
    ///     fn style(&self) -> FontStyle { FontStyle::Normal }
    ///     fn weight(&self) -> u16 { 400 }
    ///     fn is_monospace(&self) -> bool { false }
    ///     fn units_per_em(&self) -> u16 { 1000 }
    ///     fn glyph_for_char(&self, _c: char) -> Option<u16> { None }
    ///     fn advance_width(&self, _gid: u16) -> Option<u16> { None }
    ///     fn axes(&self) -> &[VariationAxis] { &[] }
    /// }
    /// // Default implementation returns None
    /// assert!(F.vertical_advance(0).is_none());
    /// ```
    fn vertical_advance(&self, _gid: u16) -> Option<u16> {
        None
    }
}

/// A collection of [`FaceInfo`] records that can be queried.
///
/// # Example
/// ```
/// use oxifont_core::{FaceInfo, FontCatalog, FontQuery, FontStyle, FontStretch};
/// use std::path::PathBuf;
///
/// struct SimpleCatalog { faces: Vec<FaceInfo> }
///
/// impl FontCatalog for SimpleCatalog {
///     fn faces(&self) -> &[FaceInfo] { &self.faces }
///     fn find(&self, query: &FontQuery) -> Option<&FaceInfo> {
///         self.faces.iter().find(|f| {
///             query.family.as_deref().map_or(true, |fam| f.family.as_ref() == fam)
///         })
///     }
/// }
///
/// let catalog = SimpleCatalog { faces: vec![] };
/// assert!(catalog.faces().is_empty());
/// assert!(catalog.find(&FontQuery::new()).is_none());
/// ```
pub trait FontCatalog {
    /// Returns all face records in the catalog.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FaceInfo, FontCatalog, FontQuery, FontStyle, FontStretch};
    /// use std::path::PathBuf;
    ///
    /// struct C { faces: Vec<FaceInfo> }
    /// impl FontCatalog for C {
    ///     fn faces(&self) -> &[FaceInfo] { &self.faces }
    ///     fn find(&self, _q: &FontQuery) -> Option<&FaceInfo> { None }
    /// }
    /// let c = C { faces: vec![] };
    /// assert_eq!(c.faces().len(), 0);
    /// ```
    fn faces(&self) -> &[FaceInfo];

    /// Finds the first face matching `query`, or `None`.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FaceInfo, FontCatalog, FontQuery, FontStyle, FontStretch};
    /// use std::path::PathBuf;
    ///
    /// struct C;
    /// impl FontCatalog for C {
    ///     fn faces(&self) -> &[FaceInfo] { &[] }
    ///     fn find(&self, _q: &FontQuery) -> Option<&FaceInfo> { None }
    /// }
    /// assert!(C.find(&FontQuery::new().family("Arial")).is_none());
    /// ```
    fn find(&self, query: &FontQuery) -> Option<&FaceInfo>;
}

// ────────────────────────────────────────────────────────────────────────────
// NameTable trait
// ────────────────────────────────────────────────────────────────────────────

/// Trait for reading localized name records from a font.
///
/// Provides access to the OpenType `name` table, which stores copyright
/// notices, font family names, PostScript names, license descriptions, and
/// other human-readable metadata.
///
/// # Example
/// ```
/// use oxifont_core::NameTable;
///
/// struct DummyNameTable;
/// impl NameTable for DummyNameTable {
///     fn name_record(&self, name_id: u16, _language: &str) -> Option<String> {
///         if name_id == 1 { Some("DummyFont".to_string()) } else { None }
///     }
///     fn all_name_records(&self) -> Vec<(u16, String, String)> {
///         vec![(1, "en".to_string(), "DummyFont".to_string())]
///     }
/// }
///
/// let t = DummyNameTable;
/// assert_eq!(t.name_record(1, "en").as_deref(), Some("DummyFont"));
/// assert_eq!(t.all_name_records().len(), 1);
/// ```
pub trait NameTable {
    /// Return the string for a given name ID (OpenType name table).
    ///
    /// Returns the best match for the given BCP-47 language tag (e.g., `"en"`,
    /// `"ja"`), falling back to English if the requested language is not
    /// available.
    ///
    /// Common name IDs:
    /// - 0: Copyright
    /// - 1: Font Family
    /// - 2: Font Subfamily (style)
    /// - 4: Full Font Name
    /// - 5: Version string
    /// - 6: PostScript Name
    /// - 9: Designer
    /// - 13: License description
    ///
    /// # Example
    /// ```
    /// use oxifont_core::NameTable;
    /// struct T;
    /// impl NameTable for T {
    ///     fn name_record(&self, name_id: u16, _lang: &str) -> Option<String> {
    ///         if name_id == 6 { Some("Helvetica-Regular".to_string()) } else { None }
    ///     }
    ///     fn all_name_records(&self) -> Vec<(u16, String, String)> { vec![] }
    /// }
    /// // name ID 6 is the PostScript name
    /// assert_eq!(T.name_record(6, "en").as_deref(), Some("Helvetica-Regular"));
    /// assert!(T.name_record(0, "en").is_none());
    /// ```
    fn name_record(&self, name_id: u16, language: &str) -> Option<alloc::string::String>;

    /// Return all available name records as `(name_id, language_tag, value)`.
    ///
    /// The returned triples cover every name record stored in the font's
    /// `name` table, across all platforms and encodings, decoded to UTF-8.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::NameTable;
    /// struct T;
    /// impl NameTable for T {
    ///     fn name_record(&self, _id: u16, _lang: &str) -> Option<String> { None }
    ///     fn all_name_records(&self) -> Vec<(u16, String, String)> {
    ///         vec![(1, "en".to_string(), "MyFont".to_string())]
    ///     }
    /// }
    /// let records = T.all_name_records();
    /// assert_eq!(records[0].0, 1);
    /// assert_eq!(records[0].2, "MyFont");
    /// ```
    fn all_name_records(
        &self,
    ) -> alloc::vec::Vec<(u16, alloc::string::String, alloc::string::String)>;
}

// ────────────────────────────────────────────────────────────────────────────
// FontCapabilities trait
// ────────────────────────────────────────────────────────────────────────────

/// Trait for querying which OpenType features, scripts, and languages a font
/// supports.
///
/// Provides introspection into the GSUB (glyph substitution) and GPOS (glyph
/// positioning) tables, letting callers discover available OpenType layout
/// features, scripts, and language systems without fully parsing the tables.
///
/// # Example
/// ```
/// use oxifont_core::FontCapabilities;
///
/// struct DummyCaps;
/// impl FontCapabilities for DummyCaps {
///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![*b"liga", *b"calt"] }
///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![*b"kern"] }
///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![*b"latn"] }
///     fn supported_languages(&self, _script: [u8; 4]) -> Vec<[u8; 4]> { vec![] }
/// }
///
/// let caps = DummyCaps;
/// assert!(caps.has_feature(*b"liga"));
/// assert!(caps.has_feature(*b"kern"));
/// assert!(!caps.has_feature(*b"smcp"));
/// ```
pub trait FontCapabilities {
    /// Return the 4-byte OpenType feature tags available in GSUB.
    ///
    /// Each `[u8; 4]` corresponds to a registered or private OpenType feature
    /// tag (e.g., `b"liga"`, `b"kern"`, `b"calt"`).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontCapabilities;
    /// struct C;
    /// impl FontCapabilities for C {
    ///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![*b"liga"] }
    ///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_languages(&self, _s: [u8; 4]) -> Vec<[u8; 4]> { vec![] }
    /// }
    /// assert_eq!(C.gsub_features(), vec![*b"liga"]);
    /// ```
    fn gsub_features(&self) -> alloc::vec::Vec<[u8; 4]>;

    /// Return the 4-byte OpenType feature tags available in GPOS.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontCapabilities;
    /// struct C;
    /// impl FontCapabilities for C {
    ///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![*b"kern"] }
    ///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_languages(&self, _s: [u8; 4]) -> Vec<[u8; 4]> { vec![] }
    /// }
    /// assert_eq!(C.gpos_features(), vec![*b"kern"]);
    /// ```
    fn gpos_features(&self) -> alloc::vec::Vec<[u8; 4]>;

    /// Return the script tags present in GSUB and/or GPOS.
    ///
    /// Each `[u8; 4]` corresponds to an OpenType script tag (e.g., `b"latn"`,
    /// `b"kana"`, `b"arab"`).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontCapabilities;
    /// struct C;
    /// impl FontCapabilities for C {
    ///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![*b"latn", *b"kana"] }
    ///     fn supported_languages(&self, _s: [u8; 4]) -> Vec<[u8; 4]> { vec![] }
    /// }
    /// let scripts = C.supported_scripts();
    /// assert!(scripts.contains(b"latn"));
    /// ```
    fn supported_scripts(&self) -> alloc::vec::Vec<[u8; 4]>;

    /// Return the language system tags for a given script.
    ///
    /// Each `[u8; 4]` corresponds to an OpenType language system tag within
    /// the specified script (e.g., `b"TRK "` for Turkish within Latin).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontCapabilities;
    /// struct C;
    /// impl FontCapabilities for C {
    ///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![*b"latn"] }
    ///     fn supported_languages(&self, script: [u8; 4]) -> Vec<[u8; 4]> {
    ///         if &script == b"latn" { vec![*b"TRK "] } else { vec![] }
    ///     }
    /// }
    /// assert_eq!(C.supported_languages(*b"latn"), vec![*b"TRK "]);
    /// assert!(C.supported_languages(*b"arab").is_empty());
    /// ```
    fn supported_languages(&self, script: [u8; 4]) -> alloc::vec::Vec<[u8; 4]>;

    /// Return `true` if the font has the given feature in GSUB or GPOS.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontCapabilities;
    /// struct C;
    /// impl FontCapabilities for C {
    ///     fn gsub_features(&self) -> Vec<[u8; 4]> { vec![*b"liga"] }
    ///     fn gpos_features(&self) -> Vec<[u8; 4]> { vec![*b"kern"] }
    ///     fn supported_scripts(&self) -> Vec<[u8; 4]> { vec![] }
    ///     fn supported_languages(&self, _s: [u8; 4]) -> Vec<[u8; 4]> { vec![] }
    /// }
    /// assert!(C.has_feature(*b"liga"));
    /// assert!(C.has_feature(*b"kern"));
    /// assert!(!C.has_feature(*b"smcp"));
    /// ```
    fn has_feature(&self, tag: [u8; 4]) -> bool {
        self.gsub_features().contains(&tag) || self.gpos_features().contains(&tag)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// FontCollection trait
// ────────────────────────────────────────────────────────────────────────────

/// Trait for font collections (TTC/OTC files with multiple faces).
///
/// A font collection file bundles multiple font faces into a single binary,
/// sharing glyph data where possible. This trait abstracts over TrueType
/// Collections (`.ttc`) and OpenType Collections (`.otc`).
///
/// # Example
/// ```
/// use oxifont_core::{FontCollection, FontError};
///
/// struct SingleFaceCollection;
///
/// impl FontCollection for SingleFaceCollection {
///     type Face = &'static str;
///     fn face_count(&self) -> u32 { 1 }
///     fn face_at(&self, index: u32) -> Result<Self::Face, FontError> {
///         if index == 0 { Ok("FaceZero") } else {
///             Err(FontError::IndexOutOfBounds { index, count: 1 })
///         }
///     }
/// }
///
/// let col = SingleFaceCollection;
/// assert_eq!(col.face_count(), 1);
/// assert_eq!(col.face_at(0).expect("face 0 exists"), "FaceZero");
/// assert!(col.face_at(1).is_err());
/// let collected: Vec<_> = col.faces().collect();
/// assert_eq!(collected.len(), 1);
/// ```
pub trait FontCollection {
    /// The type of face this collection yields.
    type Face;

    /// Number of faces in the collection.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontCollection, FontError};
    /// struct C;
    /// impl FontCollection for C {
    ///     type Face = ();
    ///     fn face_count(&self) -> u32 { 3 }
    ///     fn face_at(&self, index: u32) -> Result<(), FontError> {
    ///         if index < 3 { Ok(()) } else { Err(FontError::NotFound) }
    ///     }
    /// }
    /// assert_eq!(C.face_count(), 3);
    /// ```
    fn face_count(&self) -> u32;

    /// Access a face by zero-based index.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] if the index is out of range or the face cannot
    /// be parsed.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontCollection, FontError};
    /// struct C;
    /// impl FontCollection for C {
    ///     type Face = u32;
    ///     fn face_count(&self) -> u32 { 2 }
    ///     fn face_at(&self, index: u32) -> Result<u32, FontError> {
    ///         if index < 2 { Ok(index) }
    ///         else { Err(FontError::IndexOutOfBounds { index, count: 2 }) }
    ///     }
    /// }
    /// assert_eq!(C.face_at(0).expect("index 0 valid"), 0);
    /// assert!(C.face_at(5).is_err());
    /// ```
    fn face_at(&self, index: u32) -> Result<Self::Face, FontError>;

    /// Iterate over all faces in the collection.
    ///
    /// The default implementation calls [`face_at`] for each index from `0`
    /// to `face_count() - 1`.
    ///
    /// [`face_at`]: FontCollection::face_at
    ///
    /// # Example
    /// ```
    /// use oxifont_core::{FontCollection, FontError};
    /// struct C;
    /// impl FontCollection for C {
    ///     type Face = u32;
    ///     fn face_count(&self) -> u32 { 3 }
    ///     fn face_at(&self, index: u32) -> Result<u32, FontError> { Ok(index) }
    /// }
    /// let indices: Vec<u32> = C.faces().map(|r| r.expect("valid")).collect();
    /// assert_eq!(indices, vec![0, 1, 2]);
    /// ```
    fn faces(&self) -> impl Iterator<Item = Result<Self::Face, FontError>> + '_ {
        (0..self.face_count()).map(move |i| self.face_at(i))
    }
}
