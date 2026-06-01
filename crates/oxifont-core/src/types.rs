//! Primitive font data types: weights, styles, metrics, outlines, kerning, and color formats.

extern crate alloc;

/// The typographic style of a font face.
///
/// Ordering follows CSS specificity: `Normal < Italic < Oblique`.
///
/// # Example
/// ```
/// use oxifont_core::FontStyle;
/// assert_eq!(FontStyle::default(), FontStyle::Normal);
/// assert!(FontStyle::Normal < FontStyle::Italic);
/// assert!(FontStyle::Italic < FontStyle::Oblique);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FontStyle {
    /// A regular, upright style.
    #[default]
    Normal,
    /// An italic style.
    Italic,
    /// An oblique (mechanically slanted) style.
    Oblique,
}

impl FontStyle {
    /// Returns a CSS-style preference score for `available` when `requested`
    /// is the desired style.
    ///
    /// Higher scores indicate a better match.  This follows the CSS Fonts
    /// Level 4 §4.5 style-matching algorithm:
    ///
    /// * When italic is requested: `Italic` > `Oblique` > `Normal`.
    /// * When oblique is requested: `Oblique` > `Italic` > `Normal`.
    /// * When normal is requested: `Normal` > `Oblique` > `Italic`.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontStyle;
    ///
    /// let score_italic = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Italic);
    /// let score_oblique = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Oblique);
    /// let score_normal = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Normal);
    /// assert!(score_italic > score_oblique);
    /// assert!(score_oblique > score_normal);
    /// ```
    pub fn css_preference_score(requested: FontStyle, available: FontStyle) -> i32 {
        match (requested, available) {
            // When italic is requested: Italic=2, Oblique=1, Normal=0
            (FontStyle::Italic, FontStyle::Italic) => 2,
            (FontStyle::Italic, FontStyle::Oblique) => 1,
            (FontStyle::Italic, FontStyle::Normal) => 0,
            // When oblique is requested: Oblique=2, Italic=1, Normal=0
            (FontStyle::Oblique, FontStyle::Oblique) => 2,
            (FontStyle::Oblique, FontStyle::Italic) => 1,
            (FontStyle::Oblique, FontStyle::Normal) => 0,
            // When normal is requested: Normal=2, Oblique=1, Italic=0
            (FontStyle::Normal, FontStyle::Normal) => 2,
            (FontStyle::Normal, FontStyle::Oblique) => 1,
            (FontStyle::Normal, FontStyle::Italic) => 0,
        }
    }
}

/// CSS font-stretch / width values (CSS Fonts Level 4).
///
/// Maps to CSS `font-stretch` keyword values. The numeric value (1--9)
/// corresponds to the OpenType `usWidthClass` in the OS/2 table.
///
/// # Example
/// ```
/// use oxifont_core::FontStretch;
/// assert_eq!(FontStretch::default(), FontStretch::Normal);
/// assert!(FontStretch::Condensed < FontStretch::Normal);
/// assert!(FontStretch::Normal < FontStretch::Expanded);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum FontStretch {
    /// 50% width (usWidthClass = 1).
    UltraCondensed = 1,
    /// 62.5% width (usWidthClass = 2).
    ExtraCondensed = 2,
    /// 75% width (usWidthClass = 3).
    Condensed = 3,
    /// 87.5% width (usWidthClass = 4).
    SemiCondensed = 4,
    /// 100% width (usWidthClass = 5). Default.
    #[default]
    Normal = 5,
    /// 112.5% width (usWidthClass = 6).
    SemiExpanded = 6,
    /// 125% width (usWidthClass = 7).
    Expanded = 7,
    /// 150% width (usWidthClass = 8).
    ExtraExpanded = 8,
    /// 200% width (usWidthClass = 9).
    UltraExpanded = 9,
}

impl FontStretch {
    /// Convert a numeric width class (1--9) to a `FontStretch` value.
    ///
    /// Values outside the 1--9 range are clamped.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontStretch;
    /// assert_eq!(FontStretch::from_width_class(3), FontStretch::Condensed);
    /// assert_eq!(FontStretch::from_width_class(5), FontStretch::Normal);
    /// assert_eq!(FontStretch::from_width_class(0), FontStretch::UltraCondensed);
    /// assert_eq!(FontStretch::from_width_class(255), FontStretch::UltraExpanded);
    /// ```
    pub fn from_width_class(value: u8) -> Self {
        match value {
            0 | 1 => Self::UltraCondensed,
            2 => Self::ExtraCondensed,
            3 => Self::Condensed,
            4 => Self::SemiCondensed,
            5 => Self::Normal,
            6 => Self::SemiExpanded,
            7 => Self::Expanded,
            8 => Self::ExtraExpanded,
            _ => Self::UltraExpanded,
        }
    }

    /// Returns the numeric width class (1--9).
    ///
    /// # Example
    /// ```
    /// use oxifont_core::FontStretch;
    /// assert_eq!(FontStretch::Normal.to_width_class(), 5);
    /// assert_eq!(FontStretch::Condensed.to_width_class(), 3);
    /// assert_eq!(FontStretch::UltraExpanded.to_width_class(), 9);
    /// ```
    pub fn to_width_class(self) -> u8 {
        self as u8
    }
}

impl core::fmt::Display for FontStretch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::UltraCondensed => "ultra-condensed",
            Self::ExtraCondensed => "extra-condensed",
            Self::Condensed => "condensed",
            Self::SemiCondensed => "semi-condensed",
            Self::Normal => "normal",
            Self::SemiExpanded => "semi-expanded",
            Self::Expanded => "expanded",
            Self::ExtraExpanded => "extra-expanded",
            Self::UltraExpanded => "ultra-expanded",
        };
        write!(f, "{name}")
    }
}

/// Embedding license derived from OS/2 `fsType` bits.
///
/// The `fsType` field in the OS/2 table controls how a font may be embedded
/// and used in documents.  Bit meanings follow the OpenType specification §
/// OS/2 `fsType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EmbeddingLicense {
    /// Installable embedding: `fsType == 0`.
    ///
    /// The font may be embedded and permanently installed on a remote system.
    Installable,
    /// Restricted license embedding: bit 1 set (`fsType & 0x0002 != 0`).
    ///
    /// The font may be embedded but must only be used temporarily on the
    /// target system (preview/print).
    Restricted,
    /// Print and preview embedding: bit 2 set (`fsType & 0x0004 != 0`).
    ///
    /// The font may be embedded for read-only viewing; users may not edit
    /// documents in the embedded font.
    PrintAndPreview,
    /// Editable embedding: bit 3 set (`fsType & 0x0008 != 0`).
    ///
    /// The font may be embedded and temporarily installed; users may edit
    /// documents using the embedded font.
    Editable,
    /// No subsetting allowed: bit 8 set (`fsType & 0x0100 != 0`), combined
    /// with another embedding class.
    ///
    /// The font must be embedded in its entirety; subsetting is prohibited.
    NoSubsetting,
    /// Bitmap embedding only: bit 9 set (`fsType & 0x0200 != 0`), combined
    /// with another embedding class.
    ///
    /// Only bitmap strikes may be embedded; outline data must not be used.
    BitmapOnly,
}

impl EmbeddingLicense {
    /// Parse from an OS/2 `fsType` u16 field.
    ///
    /// Modifier bits (NoSubsetting = bit 8, BitmapOnly = bit 9) take
    /// precedence over the embedding class bits (1–3) when set, because they
    /// further restrict how the font may be used.
    ///
    /// # Examples
    /// ```
    /// use oxifont_core::EmbeddingLicense;
    /// assert_eq!(EmbeddingLicense::from_fs_type(0), EmbeddingLicense::Installable);
    /// assert_eq!(EmbeddingLicense::from_fs_type(0x0002), EmbeddingLicense::Restricted);
    /// assert_eq!(EmbeddingLicense::from_fs_type(0x0200), EmbeddingLicense::BitmapOnly);
    /// ```
    pub fn from_fs_type(fs_type: u16) -> Self {
        // Modifier bits take precedence — check them first.
        if fs_type & 0x0200 != 0 {
            return Self::BitmapOnly;
        }
        if fs_type & 0x0100 != 0 {
            return Self::NoSubsetting;
        }
        // Embedding class bits (only one should be set; lowest wins per spec).
        if fs_type & 0x0002 != 0 {
            Self::Restricted
        } else if fs_type & 0x0004 != 0 {
            Self::PrintAndPreview
        } else if fs_type & 0x0008 != 0 {
            Self::Editable
        } else {
            Self::Installable
        }
    }
}

/// Font-wide metrics extracted from the `OS/2`, `hhea`, `head`, and `post`
/// tables.
///
/// All values are in font design units unless otherwise noted. To convert to
/// pixels, multiply by `(point_size / 72.0) * dpi / units_per_em`.
///
/// # Example
/// ```
/// use oxifont_core::FontMetrics;
/// let m = FontMetrics {
///     units_per_em: 2048,
///     ascender: 1638,
///     descender: -410,
///     line_gap: 0,
///     cap_height: Some(1462),
///     x_height: Some(1126),
///     underline_position: -205,
///     underline_thickness: 102,
///     strikeout_position: 530,
///     strikeout_thickness: 102,
/// };
/// assert_eq!(m.units_per_em, 2048);
/// assert!(m.ascender > 0);
/// assert!(m.descender < 0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FontMetrics {
    /// Design units per em square (typically 1000 for CFF, 2048 for TrueType).
    pub units_per_em: u16,
    /// Typographic ascender (positive, above baseline).
    pub ascender: i16,
    /// Typographic descender (negative, below baseline).
    pub descender: i16,
    /// Typographic line gap (extra leading between lines).
    pub line_gap: i16,
    /// Cap height — height of uppercase flat glyphs (e.g. `H`). `None` if the
    /// font does not declare it (pre-OpenType 1.2 OS/2 tables).
    pub cap_height: Option<i16>,
    /// x-height — height of lowercase flat glyphs (e.g. `x`). `None` if the
    /// font does not declare it.
    pub x_height: Option<i16>,
    /// Underline position (negative = below baseline), from the `post` table.
    pub underline_position: i16,
    /// Underline thickness, from the `post` table.
    pub underline_thickness: i16,
    /// Strikeout position (positive = above baseline), from the `OS/2` table.
    pub strikeout_position: i16,
    /// Strikeout thickness (stroke width), from the `OS/2` table.
    pub strikeout_thickness: i16,
}

/// A single path command produced by glyph outline extraction.
///
/// Coordinates are in font design units.
///
/// # Example
/// ```
/// use oxifont_core::GlyphOutline;
/// let cmds = vec![
///     GlyphOutline::MoveTo { x: 0.0, y: 0.0 },
///     GlyphOutline::LineTo { x: 100.0, y: 0.0 },
///     GlyphOutline::QuadTo { cx: 100.0, cy: 50.0, x: 50.0, y: 100.0 },
///     GlyphOutline::Close,
/// ];
/// assert_eq!(cmds.len(), 4);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphOutline {
    /// Move to an absolute position (start of a new contour).
    MoveTo {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },
    /// Draw a straight line to the given point.
    LineTo {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },
    /// Draw a quadratic Bezier curve (one control point).
    QuadTo {
        /// Control point X.
        cx: f32,
        /// Control point Y.
        cy: f32,
        /// End point X.
        x: f32,
        /// End point Y.
        y: f32,
    },
    /// Draw a cubic Bezier curve (two control points).
    CubicTo {
        /// First control point X.
        cx1: f32,
        /// First control point Y.
        cy1: f32,
        /// Second control point X.
        cx2: f32,
        /// Second control point Y.
        cy2: f32,
        /// End point X.
        x: f32,
        /// End point Y.
        y: f32,
    },
    /// Close the current contour (draw a line back to the last MoveTo).
    Close,
}

/// A kerning adjustment between two glyphs.
///
/// # Example
/// ```
/// use oxifont_core::KerningPair;
/// let pair = KerningPair { left_gid: 36, right_gid: 55, value: -20 };
/// assert_eq!(pair.value, -20);
/// assert!(pair.value < 0, "negative value means glyphs are drawn closer together");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KerningPair {
    /// Left glyph ID.
    pub left_gid: u16,
    /// Right glyph ID.
    pub right_gid: u16,
    /// Horizontal kerning value in design units (negative = tighter).
    pub value: i16,
}

/// The type of color glyph data present in a font.
///
/// # Example
/// ```
/// use oxifont_core::ColorGlyphFormat;
/// let fmt = ColorGlyphFormat::ColrV1;
/// assert_eq!(fmt, ColorGlyphFormat::ColrV1);
/// assert_ne!(fmt, ColorGlyphFormat::Svg);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorGlyphFormat {
    /// COLR version 0 (layered color glyphs with CPAL palette).
    ColrV0,
    /// COLR version 1 (paint-based color glyphs, gradients, compositing).
    ColrV1,
    /// CBDT/CBLC — embedded color bitmap glyphs.
    Cbdt,
    /// `sbix` — Apple bitmap-in-SFNT color glyphs.
    Sbix,
    /// `SVG ` — SVG documents embedded per glyph.
    Svg,
}
