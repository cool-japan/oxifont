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
/// Coordinates are in font design units (positive Y is up, following font
/// convention). Consumers that render to screen space (Y down) must negate Y
/// before drawing.
///
/// # Coordinate system
///
/// All coordinates use the OpenType/TrueType font coordinate system:
/// - X increases left-to-right.
/// - Y increases bottom-to-top (Y=0 is the baseline).
/// - Values are in design units; divide by `units_per_em` and multiply by the
///   target pixel size to convert to pixels.
///
/// # Field-name convention
///
/// Control-point fields use the prefix `cx`/`cy` (curve control) rather than
/// `x1`/`y1` to avoid ambiguity with endpoint coordinates. When passing
/// commands to renderers that use a different convention (e.g. `x1`/`y1`),
/// map `cx → x1`, `cy → y1`, `cx1 → x1`, `cy1 → y1`, `cx2 → x2`, `cy2 → y2`.
/// The [`GlyphOutline::transform`] helper applies a linear transform to all
/// coordinates and is suitable for scaling + Y-flip when targeting screen space.
///
/// # Compatibility with `oxitext-raster`
///
/// `oxitext-raster`'s `PathCommand` type uses `x1`/`y1` for `QuadTo` control
/// points and `x1`/`y1`/`x2`/`y2` for `CubicTo`. The `OxifontRaster` backend
/// in `oxitext-raster` (feature `oxifont-backend`) consumes `GlyphOutline`
/// directly via the field destructuring pattern, so these types are compatible
/// at the Rust pattern-match level. No intermediate conversion type is needed.
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
///
/// // Scale to pixels (units_per_em=1000, target=16px) with Y-flip:
/// let scale = 16.0_f32 / 1000.0;
/// let pixel_cmds: Vec<GlyphOutline> = cmds
///     .iter()
///     .map(|c| c.transform(scale, -scale, 0.0, 16.0))
///     .collect();
/// // After transform, Y values are negated and scaled.
/// match &pixel_cmds[0] {
///     GlyphOutline::MoveTo { x, y } => {
///         assert!((*x - 0.0).abs() < 1e-5);
///         assert!((*y - 16.0).abs() < 1e-5); // 0.0 * -scale + 16.0 = 16.0
///     }
///     _ => unreachable!(),
/// }
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
    ///
    /// The control point `(cx, cy)` corresponds to `(x1, y1)` in renderers
    /// that use numbered control-point names.
    QuadTo {
        /// Control point X (maps to `x1` in many renderer APIs).
        cx: f32,
        /// Control point Y (maps to `y1` in many renderer APIs).
        cy: f32,
        /// End point X.
        x: f32,
        /// End point Y.
        y: f32,
    },
    /// Draw a cubic Bezier curve (two control points).
    ///
    /// Control points `(cx1, cy1)` and `(cx2, cy2)` correspond to `(x1, y1)`
    /// and `(x2, y2)` in renderers that use numbered control-point names.
    CubicTo {
        /// First control point X (maps to `x1` in many renderer APIs).
        cx1: f32,
        /// First control point Y (maps to `y1` in many renderer APIs).
        cy1: f32,
        /// Second control point X (maps to `x2` in many renderer APIs).
        cx2: f32,
        /// Second control point Y (maps to `y2` in many renderer APIs).
        cy2: f32,
        /// End point X.
        x: f32,
        /// End point Y.
        y: f32,
    },
    /// Close the current contour (draw a line back to the last MoveTo).
    Close,
}

impl GlyphOutline {
    /// Apply a linear coordinate transform to every coordinate in this command.
    ///
    /// Each coordinate `v` is transformed as `v * scale + offset` where
    /// `x_scale`/`x_offset` apply to X coordinates and `y_scale`/`y_offset`
    /// apply to Y coordinates.
    ///
    /// This is the canonical way to convert from font design space (Y-up,
    /// design units) to screen space (Y-down, pixels):
    ///
    /// ```text
    /// x_scale = px_size / units_per_em
    /// y_scale = -(px_size / units_per_em)   // negate to flip Y axis
    /// x_offset = 0.0
    /// y_offset = ascender_px                // shift baseline down
    /// ```
    ///
    /// # Example
    /// ```
    /// use oxifont_core::GlyphOutline;
    ///
    /// // A 1000-unit-per-em font rendered at 16px: scale = 0.016, y-flip.
    /// let cmd = GlyphOutline::LineTo { x: 500.0, y: 700.0 };
    /// let scale = 16.0_f32 / 1000.0;
    /// let transformed = cmd.transform(scale, -scale, 0.0, 16.0);
    /// match transformed {
    ///     GlyphOutline::LineTo { x, y } => {
    ///         assert!((x - 8.0).abs() < 1e-5, "x={x}");
    ///         // y = 700.0 * -0.016 + 16.0 = -11.2 + 16.0 = 4.8
    ///         assert!((y - 4.8).abs() < 1e-4, "y={y}");
    ///     }
    ///     _ => unreachable!(),
    /// }
    /// ```
    #[must_use]
    pub fn transform(
        &self,
        x_scale: f32,
        y_scale: f32,
        x_offset: f32,
        y_offset: f32,
    ) -> GlyphOutline {
        let tx = |v: f32| v * x_scale + x_offset;
        let ty = |v: f32| v * y_scale + y_offset;
        match *self {
            GlyphOutline::MoveTo { x, y } => GlyphOutline::MoveTo { x: tx(x), y: ty(y) },
            GlyphOutline::LineTo { x, y } => GlyphOutline::LineTo { x: tx(x), y: ty(y) },
            GlyphOutline::QuadTo { cx, cy, x, y } => GlyphOutline::QuadTo {
                cx: tx(cx),
                cy: ty(cy),
                x: tx(x),
                y: ty(y),
            },
            GlyphOutline::CubicTo {
                cx1,
                cy1,
                cx2,
                cy2,
                x,
                y,
            } => GlyphOutline::CubicTo {
                cx1: tx(cx1),
                cy1: ty(cy1),
                cx2: tx(cx2),
                cy2: ty(cy2),
                x: tx(x),
                y: ty(y),
            },
            GlyphOutline::Close => GlyphOutline::Close,
        }
    }

    /// Return an iterator over all (x, y) coordinate pairs embedded in this
    /// command, including control points.
    ///
    /// Useful for computing bounding boxes without pattern-matching on each
    /// variant.  `Close` yields no coordinates.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::GlyphOutline;
    ///
    /// let cmd = GlyphOutline::CubicTo {
    ///     cx1: 10.0, cy1: 20.0,
    ///     cx2: 30.0, cy2: 40.0,
    ///     x: 50.0, y: 60.0,
    /// };
    /// let coords: Vec<(f32, f32)> = cmd.coords().collect();
    /// assert_eq!(coords.len(), 3);
    /// assert_eq!(coords[0], (10.0, 20.0));
    /// assert_eq!(coords[2], (50.0, 60.0));
    /// ```
    pub fn coords(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        // Build a fixed-size stack array; use a counter to track how many are filled.
        let mut buf = [(0.0f32, 0.0f32); 3];
        let n = match *self {
            GlyphOutline::MoveTo { x, y } => {
                buf[0] = (x, y);
                1
            }
            GlyphOutline::LineTo { x, y } => {
                buf[0] = (x, y);
                1
            }
            GlyphOutline::QuadTo { cx, cy, x, y } => {
                buf[0] = (cx, cy);
                buf[1] = (x, y);
                2
            }
            GlyphOutline::CubicTo {
                cx1,
                cy1,
                cx2,
                cy2,
                x,
                y,
            } => {
                buf[0] = (cx1, cy1);
                buf[1] = (cx2, cy2);
                buf[2] = (x, y);
                3
            }
            GlyphOutline::Close => 0,
        };
        buf.into_iter().take(n)
    }

    /// Compute the axis-aligned bounding box of a slice of outline commands.
    ///
    /// Returns `None` if no coordinate data is present (e.g. only `Close`
    /// commands).  The returned tuple is `(x_min, y_min, x_max, y_max)`.
    ///
    /// Control points of quadratic and cubic curves are included in the bounding
    /// box calculation.  This is a conservative estimate — the true curve bounds
    /// may be tighter — but is sufficient for bitmap allocation.
    ///
    /// # Example
    /// ```
    /// use oxifont_core::GlyphOutline;
    ///
    /// let cmds = [
    ///     GlyphOutline::MoveTo { x: 100.0, y: 0.0 },
    ///     GlyphOutline::LineTo { x: 200.0, y: 700.0 },
    ///     GlyphOutline::Close,
    /// ];
    /// let bbox = GlyphOutline::bounding_box(&cmds);
    /// assert!(bbox.is_some());
    /// let (x0, y0, x1, y1) = bbox.unwrap();
    /// assert!((x0 - 100.0).abs() < 1e-5);
    /// assert!((y0 - 0.0).abs() < 1e-5);
    /// assert!((x1 - 200.0).abs() < 1e-5);
    /// assert!((y1 - 700.0).abs() < 1e-5);
    /// ```
    pub fn bounding_box(cmds: &[GlyphOutline]) -> Option<(f32, f32, f32, f32)> {
        let mut x_min = f32::INFINITY;
        let mut y_min = f32::INFINITY;
        let mut x_max = f32::NEG_INFINITY;
        let mut y_max = f32::NEG_INFINITY;

        for cmd in cmds {
            for (x, y) in cmd.coords() {
                if x < x_min {
                    x_min = x;
                }
                if x > x_max {
                    x_max = x;
                }
                if y < y_min {
                    y_min = y;
                }
                if y > y_max {
                    y_max = y;
                }
            }
        }

        if x_min.is_infinite() {
            None
        } else {
            Some((x_min, y_min, x_max, y_max))
        }
    }
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
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new OpenType color table formats can be added in
/// minor versions without a semver break.
///
/// # Example
/// ```
/// use oxifont_core::ColorGlyphFormat;
/// let fmt = ColorGlyphFormat::ColrV1;
/// assert_eq!(fmt, ColorGlyphFormat::ColrV1);
/// assert_ne!(fmt, ColorGlyphFormat::Svg);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
