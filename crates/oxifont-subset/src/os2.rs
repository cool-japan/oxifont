/// OS/2 table rewriting utilities.
///
/// Recomputes unicode range bits and first/last char indices from the
/// surviving codepoint set after subsetting.
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Unicode range table
// ---------------------------------------------------------------------------

/// Maps (range_start, range_end, bit_index) to OpenType ulUnicodeRange bits.
///
/// Bit numbers 0-31 correspond to ulUnicodeRange1,
/// 32-63 to ulUnicodeRange2, 64-95 to ulUnicodeRange3,
/// 96-127 to ulUnicodeRange4.
const UNICODE_RANGES: &[(u32, u32, u8)] = &[
    (0x0000, 0x007F, 0),  // Basic Latin
    (0x0080, 0x00FF, 1),  // Latin-1 Supplement
    (0x0100, 0x024F, 2),  // Latin Extended-A/B
    (0x0250, 0x02AF, 3),  // IPA Extensions
    (0x02B0, 0x02FF, 4),  // Spacing Modifier Letters
    (0x0300, 0x036F, 5),  // Combining Diacritical Marks
    (0x0370, 0x03FF, 6),  // Greek and Coptic
    (0x0400, 0x04FF, 9),  // Cyrillic
    (0x0500, 0x052F, 9),  // Cyrillic Supplement (same bit)
    (0x0530, 0x058F, 10), // Armenian
    (0x0590, 0x05FF, 11), // Hebrew
    (0x0600, 0x06FF, 13), // Arabic
    (0x0700, 0x074F, 71), // Syriac
    (0x0750, 0x077F, 13), // Arabic Supplement (same bit as Arabic)
    (0x0780, 0x07BF, 72), // Thaana
    (0x07C0, 0x07FF, 14), // NKo
    (0x0900, 0x097F, 15), // Devanagari
    (0x0980, 0x09FF, 16), // Bengali
    (0x0A00, 0x0A7F, 17), // Gurmukhi
    (0x0A80, 0x0AFF, 18), // Gujarati
    (0x0B00, 0x0B7F, 19), // Oriya
    (0x0B80, 0x0BFF, 20), // Tamil
    (0x0C00, 0x0C7F, 21), // Telugu
    (0x0C80, 0x0CFF, 22), // Kannada
    (0x0D00, 0x0D7F, 23), // Malayalam
    (0x0D80, 0x0DFF, 73), // Sinhala
    (0x0E00, 0x0E7F, 24), // Thai
    (0x0E80, 0x0EFF, 25), // Lao
    (0x0F00, 0x0FFF, 70), // Tibetan
    (0x1000, 0x109F, 74), // Myanmar
    (0x10A0, 0x10FF, 26), // Georgian
    (0x1100, 0x11FF, 28), // Hangul Jamo
    (0x1200, 0x137F, 75), // Ethiopic
    (0x13A0, 0x13FF, 76), // Cherokee
    (0x1400, 0x167F, 77), // Unified Canadian Aboriginal Syllabics
    (0x1680, 0x169F, 78), // Ogham
    (0x16A0, 0x16FF, 79), // Runic
    (0x1700, 0x177F, 84), // Tagalog + Hanunoo + Buhid + Tagbanwa (approx)
    (0x1780, 0x17FF, 80), // Khmer
    (0x1800, 0x18AF, 81), // Mongolian
    (0x1D00, 0x1DBF, 4),  // Phonetic extensions (same bit as Spacing Modifiers)
    (0x1E00, 0x1EFF, 29), // Latin Extended Additional
    (0x1F00, 0x1FFF, 7),  // Greek Extended
    (0x2000, 0x206F, 31), // General Punctuation
    (0x2070, 0x209F, 32), // Superscripts and Subscripts
    (0x20A0, 0x20CF, 33), // Currency Symbols
    (0x20D0, 0x20FF, 34), // Combining Diacritical Marks for Symbols
    (0x2100, 0x214F, 35), // Letterlike Symbols
    (0x2150, 0x218F, 36), // Number Forms
    (0x2190, 0x21FF, 37), // Arrows
    (0x2200, 0x22FF, 38), // Mathematical Operators
    (0x2300, 0x23FF, 39), // Miscellaneous Technical
    (0x2400, 0x243F, 40), // Control Pictures
    (0x2440, 0x245F, 41), // Optical Character Recognition
    (0x2460, 0x24FF, 42), // Enclosed Alphanumerics
    (0x2500, 0x257F, 43), // Box Drawing
    (0x2580, 0x259F, 44), // Block Elements
    (0x25A0, 0x25FF, 45), // Geometric Shapes
    (0x2600, 0x26FF, 46), // Miscellaneous Symbols
    (0x2700, 0x27BF, 47), // Dingbats
    (0x27C0, 0x27EF, 38), // Miscellaneous Mathematical Symbols-A (same as Math Ops)
    (0x2800, 0x28FF, 82), // Braille Patterns
    (0x2900, 0x297F, 37), // Supplemental Arrows-B (same bit as Arrows)
    (0x2C00, 0x2C5F, 83), // Glagolitic
    (0x2C60, 0x2C7F, 29), // Latin Extended-C (same bit as Latin Extended Add.)
    (0x2C80, 0x2CFF, 8),  // Coptic
    (0x2D00, 0x2D2F, 26), // Georgian Supplement (same bit as Georgian)
    (0x3000, 0x303F, 48), // CJK Symbols and Punctuation
    (0x3040, 0x309F, 49), // Hiragana
    (0x30A0, 0x30FF, 50), // Katakana
    (0x3100, 0x312F, 51), // Bopomofo
    (0x3130, 0x318F, 52), // Hangul Compatibility Jamo
    (0x3190, 0x319F, 59), // Kanbun
    (0x31A0, 0x31BF, 51), // Bopomofo Extended (same bit)
    (0x31F0, 0x31FF, 50), // Katakana Phonetic Extensions (same bit)
    (0x3200, 0x32FF, 54), // Enclosed CJK Letters and Months
    (0x3300, 0x33FF, 55), // CJK Compatibility
    (0x3400, 0x4DBF, 59), // CJK Unified Ideographs Extension A (same bit as Kanbun)
    (0x4E00, 0x9FFF, 59), // CJK Unified Ideographs
    (0xA000, 0xA48F, 83), // Yi Syllables
    (0xA490, 0xA4CF, 83), // Yi Radicals
    (0xAC00, 0xD7AF, 56), // Hangul Syllables
    (0xD800, 0xDFFF, 57), // Non-Plane 0 (surrogate pairs)
    (0xE000, 0xF8FF, 60), // Private Use Area
    (0xF900, 0xFAFF, 61), // CJK Compatibility Ideographs
    (0xFB00, 0xFB4F, 62), // Alphabetic Presentation Forms
    (0xFB50, 0xFDFF, 63), // Arabic Presentation Forms-A
    (0xFE00, 0xFE0F, 91), // Variation Selectors
    (0xFE20, 0xFE2F, 64), // Combining Half Marks
    (0xFE30, 0xFE4F, 65), // CJK Compatibility Forms
    (0xFE50, 0xFE6F, 66), // Small Form Variants
    (0xFE70, 0xFEFF, 67), // Arabic Presentation Forms-B
    (0xFF00, 0xFFEF, 68), // Halfwidth and Fullwidth Forms
    (0xFFF0, 0xFFFF, 69), // Specials
    // Supplementary ranges (>= 0x10000) map to high bits.
    (0x10000, 0x1007F, 85), // Linear B Syllabary
    (0x10080, 0x100FF, 85), // Linear B Ideograms (same)
    (0x10140, 0x1018F, 86), // Ancient Greek Numbers
    (0x10300, 0x1032F, 87), // Old Italic
    (0x10330, 0x1034F, 88), // Gothic
    (0x10400, 0x1044F, 89), // Deseret
    (0x1D000, 0x1D0FF, 90), // Byzantine Musical Symbols
    (0x1D100, 0x1D1FF, 90), // Musical Symbols (same bit)
    (0x1D300, 0x1D35F, 92), // Tai Xuan Jing Symbols
    (0x1D400, 0x1D7FF, 89), // Mathematical Alphanumeric Symbols
    (0x1F000, 0x1F02F, 93), // Mahjong Tiles
    (0x20000, 0x2A6DF, 59), // CJK Unified Ideographs Extension B (same bit 59)
    (0x2F800, 0x2FA1F, 61), // CJK Compatibility Ideographs Supplement (same bit 61)
    (0xE0000, 0xE007F, 94), // Tags
    (0xE0100, 0xE01EF, 91), // Variation Selectors Supplement (same bit 91)
    (0xF0000, 0xFFFFF, 60), // Supplementary Private Use Area-A (same bit 60)
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rewrite an OS/2 table, recomputing unicode range bits and first/last char
/// indices from the surviving codepoint set.
///
/// If `codepoints` is empty or the table is too short (< 68 bytes, pre-v1),
/// returns the original bytes verbatim (with zeroed range bits and first/last=0
/// when codepoints is empty but table is long enough).
///
/// # Field byte offsets (from table start)
/// - Bytes 0-1:   version (u16)
/// - Bytes 42-57: ulUnicodeRange1-4 (4 × u32 LE)
/// - Bytes 64-65: usFirstCharIndex (u16 LE), clamped to 0xFFFF
/// - Bytes 66-67: usLastCharIndex (u16 LE), clamped to 0xFFFF
pub fn rewrite_os2(table: &[u8], codepoints: &BTreeSet<char>) -> Vec<u8> {
    // Pre-v1 tables (< 68 bytes) cannot hold the fields we rewrite.
    if table.len() < 68 {
        return table.to_vec();
    }

    let mut out = table.to_vec();

    // Zero out the four unicode range u32 fields (bytes 42-57).
    out[42..58].fill(0);

    if codepoints.is_empty() {
        // Zero unicode ranges (already done) and set first/last to 0.
        out[64] = 0;
        out[65] = 0;
        out[66] = 0;
        out[67] = 0;
        return out;
    }

    // Build the 128-bit unicode range bitmap from surviving codepoints.
    let mut ranges: [u32; 4] = [0u32; 4];

    for &cp in codepoints {
        let cp_u32 = cp as u32;
        for &(start, end, bit) in UNICODE_RANGES {
            if cp_u32 >= start && cp_u32 <= end {
                let word = (bit / 32) as usize;
                let shift = bit % 32;
                if word < 4 {
                    ranges[word] |= 1u32 << shift;
                }
            }
        }
    }

    // Write the four u32 fields in little-endian byte order.
    out[42..46].copy_from_slice(&ranges[0].to_le_bytes());
    out[46..50].copy_from_slice(&ranges[1].to_le_bytes());
    out[50..54].copy_from_slice(&ranges[2].to_le_bytes());
    out[54..58].copy_from_slice(&ranges[3].to_le_bytes());

    // Compute first and last char (clamped to 0xFFFF).
    // BTreeSet is ordered, so first/last are O(1).
    let min_cp =
        (*codepoints.iter().next().expect("non-empty checked above") as u32).min(0xFFFF) as u16;
    let max_cp = (*codepoints
        .iter()
        .next_back()
        .expect("non-empty checked above") as u32)
        .min(0xFFFF) as u16;

    out[64..66].copy_from_slice(&min_cp.to_le_bytes());
    out[66..68].copy_from_slice(&max_cp.to_le_bytes());

    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the four unicode range words from an OS/2 table (bytes 42-57, little-endian).
///
/// Returns `None` if the table is shorter than 58 bytes.
pub fn read_unicode_ranges(table: &[u8]) -> Option<[u32; 4]> {
    if table.len() < 58 {
        return None;
    }
    Some([
        u32::from_le_bytes([table[42], table[43], table[44], table[45]]),
        u32::from_le_bytes([table[46], table[47], table[48], table[49]]),
        u32::from_le_bytes([table[50], table[51], table[52], table[53]]),
        u32::from_le_bytes([table[54], table[55], table[56], table[57]]),
    ])
}
