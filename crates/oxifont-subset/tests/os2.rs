use oxifont_subset::os2::rewrite_os2;
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

/// Build a zeroed 96-byte OS/2 table buffer.
fn make_os2_buf() -> Vec<u8> {
    vec![0u8; 96]
}

/// Read a u32 little-endian from buf at the given offset.
fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Read a u16 little-endian from buf at the given offset.
fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

/// Return a singleton BTreeSet<char> for a single codepoint.
fn cps(chars: &[char]) -> BTreeSet<char> {
    chars.iter().copied().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Basic Latin 'A' (U+0041) → bit 0 set in ulUnicodeRange1.
#[test]
fn test_basic_latin_bit0() {
    let buf = make_os2_buf();
    let result = rewrite_os2(&buf, &cps(&['A']));

    let ur1 = read_u32_le(&result, 42);
    assert!(
        ur1 & (1 << 0) != 0,
        "bit 0 (Basic Latin) should be set, got ur1={ur1:#010x}"
    );
    // Sanity: other range words should be 0.
    assert_eq!(read_u32_le(&result, 46), 0, "ur2 should be zero");
    assert_eq!(read_u32_le(&result, 50), 0, "ur3 should be zero");
    assert_eq!(read_u32_le(&result, 54), 0, "ur4 should be zero");
}

/// Greek capital letter Alpha U+0391 → bit 6 set in ulUnicodeRange1.
#[test]
fn test_greek_coptic_bit6() {
    let buf = make_os2_buf();
    let alpha = char::from_u32(0x0391).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&[alpha]));

    let ur1 = read_u32_le(&result, 42);
    assert!(
        ur1 & (1 << 6) != 0,
        "bit 6 (Greek and Coptic) should be set, got ur1={ur1:#010x}"
    );
}

/// Cyrillic U+0410 → bit 9 set in ulUnicodeRange1.
#[test]
fn test_cyrillic_bit9() {
    let buf = make_os2_buf();
    let cyr = char::from_u32(0x0410).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&[cyr]));

    let ur1 = read_u32_le(&result, 42);
    assert!(
        ur1 & (1 << 9) != 0,
        "bit 9 (Cyrillic) should be set, got ur1={ur1:#010x}"
    );
}

/// Superscripts and Subscripts U+2070 → bit 32 which is bit 0 of ulUnicodeRange2.
#[test]
fn test_superscripts_bit32() {
    let buf = make_os2_buf();
    let cp = char::from_u32(0x2070).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&[cp]));

    let ur2 = read_u32_le(&result, 46);
    // Bit 32 → word index 1 (ur2), bit position 0 within that word.
    assert!(
        ur2 & (1 << 0) != 0,
        "bit 32 (Superscripts) should be set in ur2={ur2:#010x}"
    );
}

/// Syriac U+0700 → bit 71, which is bit 7 of ulUnicodeRange3 (word index 2).
#[test]
fn test_syriac_bit71() {
    let buf = make_os2_buf();
    let cp = char::from_u32(0x0700).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&[cp]));

    let ur3 = read_u32_le(&result, 50);
    // Bit 71 → word index 2 (ur3), bit position 71 % 32 = 7.
    assert!(
        ur3 & (1 << 7) != 0,
        "bit 71 (Syriac) should be set in ur3={ur3:#010x}"
    );
}

/// Empty codepoints → all unicode range bits zero, first=0, last=0.
#[test]
fn test_empty_codepoints_all_zero() {
    let mut buf = make_os2_buf();
    // Pre-populate the range fields with non-zero values to ensure they get cleared.
    buf[42] = 0xFF;
    buf[43] = 0xFF;
    buf[64] = 0x41;
    buf[66] = 0x7E;

    let result = rewrite_os2(&buf, &BTreeSet::new());

    assert_eq!(
        read_u32_le(&result, 42),
        0,
        "ur1 should be zero for empty codepoints"
    );
    assert_eq!(
        read_u32_le(&result, 46),
        0,
        "ur2 should be zero for empty codepoints"
    );
    assert_eq!(
        read_u32_le(&result, 50),
        0,
        "ur3 should be zero for empty codepoints"
    );
    assert_eq!(
        read_u32_le(&result, 54),
        0,
        "ur4 should be zero for empty codepoints"
    );
    assert_eq!(
        read_u16_le(&result, 64),
        0,
        "usFirstCharIndex should be 0 for empty codepoints"
    );
    assert_eq!(
        read_u16_le(&result, 66),
        0,
        "usLastCharIndex should be 0 for empty codepoints"
    );
}

/// Table shorter than 68 bytes → returned verbatim.
#[test]
fn test_short_table_verbatim() {
    let short_buf: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
    let result = rewrite_os2(&short_buf, &cps(&['A']));
    assert_eq!(result, short_buf, "short table should be returned verbatim");
}

/// Table exactly 67 bytes → returned verbatim (boundary condition).
#[test]
fn test_67_bytes_verbatim() {
    let buf: Vec<u8> = (0u8..67).collect();
    let result = rewrite_os2(&buf, &cps(&['A']));
    assert_eq!(result, buf, "67-byte table should be returned verbatim");
}

/// Table exactly 68 bytes → processed (not returned verbatim).
#[test]
fn test_68_bytes_processed() {
    let buf = vec![0u8; 68];
    let result = rewrite_os2(&buf, &cps(&['A']));
    // bit 0 should be set (Basic Latin 'A')
    let ur1 = read_u32_le(&result, 42);
    assert!(
        ur1 & 1 != 0,
        "68-byte table should be processed; ur1={ur1:#010x}"
    );
}

/// First/last char with codepoints > 0xFFFF: clamped to 0xFFFF.
#[test]
fn test_first_last_clamped_to_ffff() {
    let buf = make_os2_buf();
    // U+1F600 GRINNING FACE (emoji, > 0xFFFF)
    let emoji = char::from_u32(0x1F600).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&[emoji]));

    assert_eq!(
        read_u16_le(&result, 64),
        0xFFFF,
        "usFirstCharIndex should clamp to 0xFFFF for SMP codepoints"
    );
    assert_eq!(
        read_u16_le(&result, 66),
        0xFFFF,
        "usLastCharIndex should clamp to 0xFFFF for SMP codepoints"
    );
}

/// First/last char with mixed BMP and non-BMP codepoints.
#[test]
fn test_first_last_mixed() {
    let buf = make_os2_buf();
    // 'A' = 0x0041, '~' = 0x007E — both BMP, last should be 0x007E.
    let result = rewrite_os2(&buf, &cps(&['A', '~']));

    assert_eq!(
        read_u16_le(&result, 64),
        0x0041,
        "usFirstCharIndex should be 'A'"
    );
    assert_eq!(
        read_u16_le(&result, 66),
        0x007E,
        "usLastCharIndex should be '~'"
    );
}

/// Non-range bytes (version, panose, etc.) are preserved verbatim.
#[test]
fn test_non_range_bytes_preserved() {
    let mut buf = make_os2_buf();
    // version at bytes 0-1
    buf[0] = 0x00;
    buf[1] = 0x04;
    // panose bytes at 32-41 (10 bytes)
    for (idx, byte) in buf[32..42].iter_mut().enumerate() {
        *byte = ((idx + 32) as u8).wrapping_add(0xA0);
    }
    // Some byte well past the range fields (e.g., byte 80)
    buf[80] = 0xBB;

    let result = rewrite_os2(&buf, &cps(&['A']));

    // Version preserved.
    assert_eq!(result[0], 0x00, "version high byte preserved");
    assert_eq!(result[1], 0x04, "version low byte preserved");
    // panose preserved.
    for (idx, &byte) in result[32..42].iter().enumerate() {
        let expected = ((idx + 32) as u8).wrapping_add(0xA0);
        assert_eq!(
            byte,
            expected,
            "panose byte {} should be preserved",
            idx + 32
        );
    }
    // Byte 80 preserved.
    assert_eq!(result[80], 0xBB, "byte 80 should be preserved verbatim");
}

/// Codepoints spanning multiple unicode blocks set multiple bits.
#[test]
fn test_multiple_blocks() {
    let buf = make_os2_buf();
    // 'A' = Basic Latin (bit 0), alpha = Greek (bit 6)
    let alpha = char::from_u32(0x0391).expect("valid codepoint");
    let result = rewrite_os2(&buf, &cps(&['A', alpha]));

    let ur1 = read_u32_le(&result, 42);
    assert!(ur1 & (1 << 0) != 0, "bit 0 (Basic Latin) should be set");
    assert!(ur1 & (1 << 6) != 0, "bit 6 (Greek) should be set");
}
