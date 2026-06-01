//! [`FaceInfo`] — lightweight font face metadata stored in the database.

use std::path::PathBuf;

pub use oxifont_core::VariationAxis;

// ---------------------------------------------------------------------------
// Unicode range → bit mapping table (mirrors OS/2 ulUnicodeRange fields).
//
// Each entry is `(range_start, range_end, bit_index)`.
// Bit N corresponds to bit N of the combined 128-bit unicode_ranges value,
// where bits 0-31 are ulUnicodeRange1, 32-63 are ulUnicodeRange2, etc.
// ---------------------------------------------------------------------------

const UNICODE_RANGES: &[(u32, u32, u8)] = &[
    (0x0000, 0x007F, 0),    // Basic Latin
    (0x0080, 0x00FF, 1),    // Latin-1 Supplement
    (0x0100, 0x024F, 2),    // Latin Extended-A/B
    (0x0250, 0x02AF, 3),    // IPA Extensions
    (0x02B0, 0x02FF, 4),    // Spacing Modifier Letters
    (0x0300, 0x036F, 5),    // Combining Diacritical Marks
    (0x0370, 0x03FF, 6),    // Greek and Coptic
    (0x0400, 0x04FF, 9),    // Cyrillic
    (0x0500, 0x052F, 9),    // Cyrillic Supplement
    (0x0530, 0x058F, 10),   // Armenian
    (0x0590, 0x05FF, 11),   // Hebrew
    (0x0600, 0x06FF, 13),   // Arabic
    (0x0700, 0x074F, 71),   // Syriac
    (0x0750, 0x077F, 13),   // Arabic Supplement
    (0x0780, 0x07BF, 72),   // Thaana
    (0x07C0, 0x07FF, 14),   // NKo
    (0x0900, 0x097F, 15),   // Devanagari
    (0x0980, 0x09FF, 16),   // Bengali
    (0x0A00, 0x0A7F, 17),   // Gurmukhi
    (0x0A80, 0x0AFF, 18),   // Gujarati
    (0x0B00, 0x0B7F, 19),   // Oriya
    (0x0B80, 0x0BFF, 20),   // Tamil
    (0x0C00, 0x0C7F, 21),   // Telugu
    (0x0C80, 0x0CFF, 22),   // Kannada
    (0x0D00, 0x0D7F, 23),   // Malayalam
    (0x0D80, 0x0DFF, 73),   // Sinhala
    (0x0E00, 0x0E7F, 24),   // Thai
    (0x0E80, 0x0EFF, 25),   // Lao
    (0x0F00, 0x0FFF, 70),   // Tibetan
    (0x1000, 0x109F, 74),   // Myanmar
    (0x10A0, 0x10FF, 26),   // Georgian
    (0x1100, 0x11FF, 28),   // Hangul Jamo
    (0x1200, 0x137F, 75),   // Ethiopic
    (0x13A0, 0x13FF, 76),   // Cherokee
    (0x1400, 0x167F, 77),   // Unified Canadian Aboriginal Syllabics
    (0x1680, 0x169F, 78),   // Ogham
    (0x16A0, 0x16FF, 79),   // Runic
    (0x1700, 0x177F, 84),   // Tagalog/Hanunoo/Buhid/Tagbanwa
    (0x1780, 0x17FF, 80),   // Khmer
    (0x1800, 0x18AF, 81),   // Mongolian
    (0x1E00, 0x1EFF, 29),   // Latin Extended Additional
    (0x1F00, 0x1FFF, 7),    // Greek Extended
    (0x2000, 0x206F, 31),   // General Punctuation
    (0x2070, 0x209F, 32),   // Superscripts and Subscripts
    (0x20A0, 0x20CF, 33),   // Currency Symbols
    (0x20D0, 0x20FF, 34),   // Combining Diacritical Marks for Symbols
    (0x2100, 0x214F, 35),   // Letterlike Symbols
    (0x2150, 0x218F, 36),   // Number Forms
    (0x2190, 0x21FF, 37),   // Arrows
    (0x2200, 0x22FF, 38),   // Mathematical Operators
    (0x2300, 0x23FF, 39),   // Miscellaneous Technical
    (0x2400, 0x243F, 40),   // Control Pictures
    (0x2440, 0x245F, 41),   // Optical Character Recognition
    (0x2460, 0x24FF, 42),   // Enclosed Alphanumerics
    (0x2500, 0x257F, 43),   // Box Drawing
    (0x2580, 0x259F, 44),   // Block Elements
    (0x25A0, 0x25FF, 45),   // Geometric Shapes
    (0x2600, 0x26FF, 46),   // Miscellaneous Symbols
    (0x2700, 0x27BF, 47),   // Dingbats
    (0x2800, 0x28FF, 82),   // Braille Patterns
    (0x2C00, 0x2C5F, 83),   // Glagolitic
    (0x2C60, 0x2C7F, 29),   // Latin Extended-C
    (0x2C80, 0x2CFF, 8),    // Coptic
    (0x3000, 0x303F, 48),   // CJK Symbols and Punctuation
    (0x3040, 0x309F, 49),   // Hiragana
    (0x30A0, 0x30FF, 50),   // Katakana
    (0x3100, 0x312F, 51),   // Bopomofo
    (0x3130, 0x318F, 52),   // Hangul Compatibility Jamo
    (0x3190, 0x319F, 59),   // Kanbun
    (0x31A0, 0x31BF, 51),   // Bopomofo Extended
    (0x31F0, 0x31FF, 50),   // Katakana Phonetic Extensions
    (0x3200, 0x32FF, 54),   // Enclosed CJK Letters and Months
    (0x3300, 0x33FF, 55),   // CJK Compatibility
    (0x3400, 0x4DBF, 59),   // CJK Unified Ideographs Extension A
    (0x4E00, 0x9FFF, 59),   // CJK Unified Ideographs
    (0xA000, 0xA48F, 83),   // Yi Syllables
    (0xA490, 0xA4CF, 83),   // Yi Radicals
    (0xAC00, 0xD7AF, 56),   // Hangul Syllables
    (0xF900, 0xFAFF, 61),   // CJK Compatibility Ideographs
    (0xFB00, 0xFB4F, 62),   // Alphabetic Presentation Forms
    (0xFB50, 0xFDFF, 63),   // Arabic Presentation Forms-A
    (0xFE00, 0xFE0F, 91),   // Variation Selectors
    (0xFE20, 0xFE2F, 64),   // Combining Half Marks
    (0xFE30, 0xFE4F, 65),   // CJK Compatibility Forms
    (0xFE50, 0xFE6F, 66),   // Small Form Variants
    (0xFE70, 0xFEFF, 67),   // Arabic Presentation Forms-B
    (0xFF00, 0xFFEF, 68),   // Halfwidth and Fullwidth Forms
    (0xFFF0, 0xFFFF, 69),   // Specials
    (0x10000, 0x1007F, 85), // Linear B Syllabary
    (0x10080, 0x100FF, 85), // Linear B Ideograms
    (0x10140, 0x1018F, 86), // Ancient Greek Numbers
    (0x10300, 0x1032F, 87), // Old Italic
    (0x10330, 0x1034F, 88), // Gothic
    (0x10400, 0x1044F, 89), // Deseret
    (0x1D000, 0x1D0FF, 90), // Byzantine Musical Symbols
    (0x1D100, 0x1D1FF, 90), // Musical Symbols
    (0x1D300, 0x1D35F, 92), // Tai Xuan Jing Symbols
    (0x1D400, 0x1D7FF, 89), // Mathematical Alphanumeric Symbols
    (0x20000, 0x2A6DF, 59), // CJK Unified Ideographs Extension B
    (0x2F800, 0x2FA1F, 61), // CJK Compatibility Ideographs Supplement
];

// ---------------------------------------------------------------------------
// Script-tag lookup: bit index → OpenType script tags
// ---------------------------------------------------------------------------

/// Maps a single unicode range bit to one or more OpenType script tags.
///
/// Each entry is `(bit_index, &[tag_bytes_4])`.
const BIT_TO_SCRIPTS: &[(u8, &[[u8; 4]])] = &[
    (0, &[*b"DFLT", *b"latn"]),  // Basic Latin
    (1, &[*b"latn"]),            // Latin-1 Supplement
    (2, &[*b"latn"]),            // Latin Extended-A/B
    (3, &[*b"latn"]),            // IPA Extensions
    (6, &[*b"grek"]),            // Greek and Coptic
    (7, &[*b"grek"]),            // Greek Extended
    (8, &[*b"copt"]),            // Coptic
    (9, &[*b"cyrl"]),            // Cyrillic
    (10, &[*b"armn"]),           // Armenian
    (11, &[*b"hebr"]),           // Hebrew
    (13, &[*b"arab"]),           // Arabic
    (14, &[*b"nko "]),           // NKo
    (15, &[*b"dev2", *b"deva"]), // Devanagari
    (16, &[*b"bng2", *b"beng"]), // Bengali
    (17, &[*b"gjr2", *b"gujr"]), // Gurmukhi (bit 17 is actually Gurmukhi)
    (18, &[*b"gjr2", *b"gujr"]), // Gujarati
    (19, &[*b"ory2", *b"orya"]), // Oriya
    (20, &[*b"tml2", *b"taml"]), // Tamil
    (21, &[*b"tel2", *b"telu"]), // Telugu
    (22, &[*b"knd2", *b"knda"]), // Kannada
    (23, &[*b"mlm2", *b"mlym"]), // Malayalam
    (24, &[*b"thai"]),           // Thai
    (25, &[*b"lao "]),           // Lao
    (26, &[*b"geor"]),           // Georgian
    (28, &[*b"jamo", *b"hang"]), // Hangul Jamo
    (48, &[*b"hani"]),           // CJK Symbols and Punctuation
    (49, &[*b"hira"]),           // Hiragana
    (50, &[*b"kana"]),           // Katakana
    (51, &[*b"bopo"]),           // Bopomofo
    (52, &[*b"hang"]),           // Hangul Compatibility Jamo
    (56, &[*b"hang"]),           // Hangul Syllables
    (59, &[*b"hani"]),           // CJK Unified Ideographs (and extensions)
    (61, &[*b"hani"]),           // CJK Compatibility Ideographs
    (62, &[*b"latn", *b"hebr"]), // Alphabetic Presentation Forms
    (63, &[*b"arab"]),           // Arabic Presentation Forms-A
    (67, &[*b"arab"]),           // Arabic Presentation Forms-B
    (70, &[*b"tibt"]),           // Tibetan
    (71, &[*b"syrc"]),           // Syriac
    (72, &[*b"thaa"]),           // Thaana
    (73, &[*b"sinh"]),           // Sinhala
    (74, &[*b"mymr"]),           // Myanmar
    (75, &[*b"ethi"]),           // Ethiopic
    (76, &[*b"cher"]),           // Cherokee
    (80, &[*b"khmr"]),           // Khmer
    (81, &[*b"mong"]),           // Mongolian
];

/// The physical source of font data.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Source {
    /// Font data lives in a file on disk.  This is the normal case.
    File(PathBuf),
    /// Font data is held inline (for small test fixtures).  Avoid for large
    /// production fonts.
    Memory(Vec<u8>),
}

/// Lightweight metadata about a single font face.
///
/// This is the primary record type stored in [`crate::FontDatabase`].  It is
/// cheap to clone and designed to be serialised for the optional disk cache.
///
/// All metric fields are derived from the font's binary tables at load time
/// and never re-read from disk unless the database is rebuilt.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FaceInfo {
    /// Unique identifier assigned by the database.
    pub id: u32,
    /// Typographic family name (e.g. `"Helvetica Neue"`).
    pub family: String,
    /// PostScript name (e.g. `"HelveticaNeue-Bold"`).
    pub post_script_name: String,
    /// CSS weight number in the range 100..=900.
    pub weight: u16,
    /// `true` when the face is italic or oblique.
    pub italic: bool,
    /// CSS stretch value: 1 (ultra-condensed) … 9 (ultra-expanded).
    pub stretch: u8,
    /// `true` when all glyphs share the same advance width.
    pub monospaced: bool,
    /// Where the raw font bytes live.
    pub source: Source,
    /// Zero-based index into a TTC collection; `0` for TTF/OTF.
    pub face_index: u32,
    /// Variable-font axes (`fvar` table); empty for static fonts.
    pub variable_axes: Vec<VariationAxis>,
    /// Per-locale family names keyed by Windows LCID.
    ///
    /// Built during `parse_face_info` by scanning all Name
    /// table records.  Entries with `name_id == 1` (FAMILY) or `name_id == 16`
    /// (TYPOGRAPHIC_FAMILY) are stored here; the LCID is taken directly from
    /// the name record's `language_id` field.
    pub locale_families: Vec<(u16, String)>,
    /// Combined OS/2 `ulUnicodeRange1..4` as a single 128-bit value.
    ///
    /// Bit layout: bits 0-31 = ulUnicodeRange1, bits 32-63 = ulUnicodeRange2,
    /// bits 64-95 = ulUnicodeRange3, bits 96-127 = ulUnicodeRange4.
    ///
    /// A value of `0` means the OS/2 table was absent or provided no range
    /// bits; callers should treat `0` as "unknown / assume all".
    ///
    /// `#[serde(default)]` ensures old JSON caches that lack this field
    /// deserialise successfully (defaulting to 0 = unknown).
    #[serde(default)]
    pub unicode_ranges: u128,
}

/// Derive a human-readable style name from weight and italic flag.
///
/// Weight names follow the CSS Fonts Level 4 / OpenType naming convention.
/// " Italic" is appended when `italic` is `true`.
fn derive_style_name(weight: u16, italic: bool) -> String {
    let base = match weight {
        ..=149 => "Thin",
        150..=249 => "ExtraLight",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "SemiBold",
        650..=749 => "Bold",
        750..=849 => "ExtraBold",
        _ => "Black",
    };
    if italic {
        format!("{base} Italic")
    } else {
        base.to_string()
    }
}

impl std::fmt::Display for FaceInfo {
    /// Formats the face as a human-readable one-liner:
    ///
    /// ```text
    /// "<family> <style> (weight: <weight>, path: <path>[<index>])"
    /// ```
    ///
    /// The optional `[<index>]` suffix is appended only when `face_index > 0`
    /// (i.e. the face lives inside a TTC collection at a non-zero position).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let style = derive_style_name(self.weight, self.italic);
        let path_str = match &self.source {
            Source::File(p) => p.display().to_string(),
            Source::Memory(_) => "<memory>".to_string(),
        };
        if self.face_index > 0 {
            write!(
                f,
                "{} {} (weight: {}, path: {}[{}])",
                self.family, style, self.weight, path_str, self.face_index
            )
        } else {
            write!(
                f,
                "{} {} (weight: {}, path: {})",
                self.family, style, self.weight, path_str
            )
        }
    }
}

impl FaceInfo {
    /// Returns the family name for the given BCP-47 locale tag, falling back
    /// to the canonical [`Self::family`] when no locale-specific record exists.
    pub fn family_for_locale(&self, bcp47: &str) -> &str {
        if let Some(lcid) = crate::locale::bcp47_to_lcid(bcp47) {
            if let Some((_, name)) = self.locale_families.iter().find(|(id, _)| *id == lcid) {
                return name.as_str();
            }
        }
        &self.family
    }

    /// Returns `true` when this face is a variable font with a `wght` axis
    /// whose range covers `weight`.
    pub fn covers_weight(&self, weight: u16) -> bool {
        let wght_tag: [u8; 4] = [b'w', b'g', b'h', b't'];
        self.variable_axes.iter().any(|ax| {
            ax.tag == wght_tag
                && f32::from(weight) >= ax.min_value
                && f32::from(weight) <= ax.max_value
        })
    }

    /// Approximate check: returns `true` when the OS/2 unicode range bits suggest
    /// this face covers the given character.
    ///
    /// When `unicode_ranges` is `0` (unknown), this returns `true` to avoid
    /// incorrectly excluding faces.  This may yield false positives: a range bit
    /// being set does not guarantee every specific codepoint in that range is
    /// present, only that the face claims coverage for the range.
    pub fn covers_char_approx(&self, c: char) -> bool {
        if self.unicode_ranges == 0 {
            return true;
        }
        covers_codepoint(self.unicode_ranges, c as u32)
    }

    /// Returns an approximate list of OpenType script tags supported by this face,
    /// derived from the OS/2 unicode range bits.
    ///
    /// The returned list is sorted and deduplicated.  When `unicode_ranges` is `0`,
    /// an empty `Vec` is returned (no claims made about scripts).
    pub fn supported_scripts_approx(&self) -> Vec<[u8; 4]> {
        let mut tags: Vec<[u8; 4]> = Vec::new();
        for (bit, script_tags) in BIT_TO_SCRIPTS {
            if self.unicode_ranges & (1u128 << bit) != 0 {
                for &tag in *script_tags {
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        tags.sort_unstable();
        tags
    }
}

// ---------------------------------------------------------------------------
// Internal: codepoint → unicode range bit coverage check
// ---------------------------------------------------------------------------

/// Returns `true` when `cp` falls within any unicode range whose bit is set
/// in `ranges`.
///
/// Iterates the static `UNICODE_RANGES` table and checks whether the relevant
/// bit is set for any range that contains `cp`.
pub(crate) fn covers_codepoint(ranges: u128, cp: u32) -> bool {
    for &(start, end, bit) in UNICODE_RANGES {
        if cp >= start && cp <= end && ranges & (1u128 << bit) != 0 {
            return true;
        }
    }
    false
}
