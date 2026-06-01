//! WOFF2 triplet coordinate decode accuracy tests (WOFF2 spec §5.2 / §6.1).
//!
//! Tests all 128 possible flag byte encodings in the WOFF2 triplet decoder
//! `decode_triplet` and verifies `glyph_stream_bytes_for_flag` partitions.
//!
//! Strategy: characterisation testing — expected values are derived by tracing
//! the actual implementation in `woff2/glyf.rs`.  The decoder is already `pub`
//! so no test helpers need to be added to production code.

#[cfg(feature = "woff2")]
mod triplet_accuracy {
    use oxifont_webfont::woff2::glyf::{decode_triplet, glyph_stream_bytes_for_flag};

    // ---------------------------------------------------------------- helpers

    /// Call decode_triplet with `flag` and an 8-byte glyph slice filled with the
    /// provided bytes (enough headroom for any flag range).
    fn decode(flag: u8, extra: &[u8]) -> (i32, i32, bool) {
        decode_triplet(flag, extra).expect("decode_triplet must not error")
    }

    // ============================================================
    // 1.  glyph_stream_bytes_for_flag: exhaustive partition check
    // ============================================================

    /// Flags 0–39 consume 0 glyph-stream bytes.
    #[test]
    fn glyph_bytes_flags_0_to_39_is_zero() {
        for f in 0u8..=39 {
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                0,
                "flag {f}: expected 0 glyph bytes"
            );
        }
    }

    /// Flags 40–87 consume 1 glyph-stream byte.
    #[test]
    fn glyph_bytes_flags_40_to_87_is_one() {
        for f in 40u8..=87 {
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                1,
                "flag {f}: expected 1 glyph byte"
            );
        }
    }

    /// Flags 88–119 consume 2 glyph-stream bytes.
    #[test]
    fn glyph_bytes_flags_88_to_119_is_two() {
        for f in 88u8..=119 {
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                2,
                "flag {f}: expected 2 glyph bytes"
            );
        }
    }

    /// Flags 120–123 consume 4 glyph-stream bytes (2-byte x, 2-byte y).
    #[test]
    fn glyph_bytes_flags_120_to_123_is_four() {
        for f in 120u8..=123 {
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                4,
                "flag {f}: expected 4 glyph bytes"
            );
        }
    }

    /// Flags 124–127 consume 8 glyph-stream bytes (4-byte x, 4-byte y).
    #[test]
    fn glyph_bytes_flags_124_to_127_is_eight() {
        for f in 124u8..=127 {
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                8,
                "flag {f}: expected 8 glyph bytes"
            );
        }
    }

    /// Bit 7 of the flag byte is masked out — `f` and `f | 0x80` return the
    /// same byte count (the high bit is not part of the 0–127 table index).
    #[test]
    fn glyph_bytes_high_bit_masked() {
        for f in 0u8..=127 {
            let with_bit = f | 0x80;
            assert_eq!(
                glyph_stream_bytes_for_flag(f),
                glyph_stream_bytes_for_flag(with_bit),
                "flag {f}: bit-7 must be masked; 0x{:02X} vs 0x{:02X}",
                f,
                with_bit
            );
        }
    }

    // ============================================================
    // 2.  Flags 0–9: on-curve, x-only (no glyph bytes)
    //     x_delta = flag_idx % 10  (== flag itself here), y_delta = 0
    // ============================================================

    /// Flags 0–9 give on-curve deltas x=flag, y=0.
    #[test]
    fn flags_0_to_9_x_only_on_curve() {
        for flag in 0u8..=9 {
            let (x, y, on_curve) = decode(flag, &[]);
            assert_eq!(
                x, flag as i32,
                "flag {flag}: x_delta should equal flag value"
            );
            assert_eq!(y, 0, "flag {flag}: y_delta should be 0");
            assert!(on_curve, "flag {flag}: must be on-curve");
        }
    }

    // ============================================================
    // 3.  Flags 10–19: off-curve, x-only (no glyph bytes)
    //     x_delta = flag_idx % 10, y_delta = 0
    // ============================================================

    /// Flags 10–19 give off-curve deltas x = (flag % 10), y = 0.
    #[test]
    fn flags_10_to_19_x_only_off_curve() {
        for flag in 10u8..=19 {
            let (x, y, on_curve) = decode(flag, &[]);
            let expected_x = (flag as usize % 10) as i32;
            assert_eq!(
                x, expected_x,
                "flag {flag}: x_delta should be {} (flag%10)",
                expected_x
            );
            assert_eq!(y, 0, "flag {flag}: y_delta should be 0");
            assert!(!on_curve, "flag {flag}: must be off-curve");
        }
    }

    // ============================================================
    // 4.  Flags 20–29: on-curve, y-only (no glyph bytes)
    //     y_delta = flag_idx % 10 = (flag - 20), x_delta = 0
    // ============================================================

    /// Flags 20–29 give on-curve deltas x=0, y = (flag % 10).
    #[test]
    fn flags_20_to_29_y_only_on_curve() {
        for flag in 20u8..=29 {
            let (x, y, on_curve) = decode(flag, &[]);
            let expected_y = (flag as usize % 10) as i32;
            assert_eq!(x, 0, "flag {flag}: x_delta should be 0");
            assert_eq!(
                y, expected_y,
                "flag {flag}: y_delta should be {} (flag%10)",
                expected_y
            );
            assert!(on_curve, "flag {flag}: must be on-curve");
        }
    }

    // ============================================================
    // 5.  Flags 30–39: off-curve, y-only (no glyph bytes)
    //     y_delta = flag_idx % 10, x_delta = 0
    // ============================================================

    /// Flags 30–39 give off-curve deltas x=0, y = (flag % 10).
    #[test]
    fn flags_30_to_39_y_only_off_curve() {
        for flag in 30u8..=39 {
            let (x, y, on_curve) = decode(flag, &[]);
            let expected_y = (flag as usize % 10) as i32;
            assert_eq!(x, 0, "flag {flag}: x_delta should be 0");
            assert_eq!(
                y, expected_y,
                "flag {flag}: y_delta should be {} (flag%10)",
                expected_y
            );
            assert!(!on_curve, "flag {flag}: must be off-curve");
        }
    }

    // ============================================================
    // 6.  Flags 40–47: on-curve, 1 nibble-packed byte (x high nibble, y low)
    // ============================================================

    /// Flag 40 (on-curve): byte 0xAB → x=10 (0xA), y=11 (0xB).
    #[test]
    fn flag_40_nibble_packed_on_curve() {
        let (x, y, on_curve) = decode(40, &[0xAB]);
        assert_eq!(x, 10, "flag 40: high nibble 0xA → x=10");
        assert_eq!(y, 11, "flag 40: low nibble 0xB → y=11");
        assert!(on_curve, "flag 40: must be on-curve");
    }

    /// Flag 40 (on-curve): byte 0x00 → x=0, y=0.
    #[test]
    fn flag_40_nibble_packed_zeros() {
        let (x, y, on_curve) = decode(40, &[0x00]);
        assert_eq!(x, 0, "flag 40: x should be 0");
        assert_eq!(y, 0, "flag 40: y should be 0");
        assert!(on_curve, "flag 40: must be on-curve");
    }

    /// Flag 47 (on-curve): byte 0xFF → x=15, y=15.
    #[test]
    fn flag_47_nibble_packed_max() {
        let (x, y, on_curve) = decode(47, &[0xFF]);
        assert_eq!(x, 15, "flag 47: high nibble 0xF → x=15");
        assert_eq!(y, 15, "flag 47: low nibble 0xF → y=15");
        assert!(on_curve, "flag 47: must be on-curve");
    }

    // ============================================================
    // 7.  Flags 48–55: off-curve, nibble-packed byte
    // ============================================================

    /// Flag 48 (off-curve): byte 0x12 → x=1, y=2, off-curve.
    #[test]
    fn flag_48_nibble_packed_off_curve() {
        let (x, y, on_curve) = decode(48, &[0x12]);
        assert_eq!(x, 1, "flag 48: high nibble 0x1 → x=1");
        assert_eq!(y, 2, "flag 48: low nibble 0x2 → y=2");
        assert!(!on_curve, "flag 48: must be off-curve");
    }

    /// Flag 55 (off-curve): byte 0xEF → x=14, y=15, off-curve.
    #[test]
    fn flag_55_nibble_packed_off_curve_max() {
        let (x, y, on_curve) = decode(55, &[0xEF]);
        assert_eq!(x, 14, "flag 55: high nibble 0xE → x=14");
        assert_eq!(y, 15, "flag 55: low nibble 0xF → y=15");
        assert!(!on_curve, "flag 55: must be off-curve");
    }

    // ============================================================
    // 8.  Flags 56–63: on-curve, 2-byte x, no y
    //     even idx → positive: x = ((idx & 7) << 8) | b0
    //     odd idx  → negative: x = -(x_raw + 1)
    // ============================================================

    /// Flag 56 (idx=56, even → positive):
    ///   glyph byte = 0x00 → x_raw = (0 << 8) | 0 = 0 → x=0, y=0
    #[test]
    fn flag_56_2byte_x_positive_zero() {
        let (x, y, on_curve) = decode(56, &[0x00]);
        // idx=56, idx & 7 = 0, high_bits = 0, x_raw = 0, even → x=0
        assert_eq!(x, 0, "flag 56 b0=0: x should be 0");
        assert_eq!(y, 0, "flag 56: y should be 0");
        assert!(on_curve, "flag 56: must be on-curve");
    }

    /// Flag 56 (even): b0=0xFF → x_raw = (0<<8)|255 = 255 → x=255
    #[test]
    fn flag_56_2byte_x_positive_max_low() {
        let (x, y, on_curve) = decode(56, &[0xFF]);
        // idx=56, (56 & 7) = 0, high_bits=0, x_raw=255, even → x=255
        assert_eq!(x, 255, "flag 56 b0=0xFF: x should be 255");
        assert_eq!(y, 0);
        assert!(on_curve);
    }

    /// Flag 57 (idx=57, odd → negative): b0=0x00 → x_raw=0 → x=-(0+1)=-1
    #[test]
    fn flag_57_2byte_x_negative_one() {
        let (x, y, on_curve) = decode(57, &[0x00]);
        // idx=57, odd → x = -(x_raw+1) where x_raw = ((57&7)<<8)|0 = (1<<8) = 256
        // x = -(256 + 1) = -257
        let idx = 57usize;
        let high_bits = ((idx & 0x07) as i32) << 8;
        let x_raw = high_bits; // b0=0x00, so | 0x00 is identity
        let expected_x = -(x_raw + 1);
        assert_eq!(x, expected_x, "flag 57 b0=0x00: x should be {expected_x}");
        assert_eq!(y, 0);
        assert!(on_curve);
    }

    /// Flag 62 (idx=62, even): b0=0x10 → x_raw = ((62&7)<<8)|16 = (6<<8)|16 = 1536+16=1552
    #[test]
    fn flag_62_2byte_x_large_positive() {
        let (x, y, on_curve) = decode(62, &[0x10]);
        let idx = 62usize;
        let high_bits = ((idx & 0x07) as i32) << 8;
        let expected_x = high_bits | 0x10; // 0x610 = 1552
        assert_eq!(x, expected_x, "flag 62 b0=0x10: x should be {expected_x}");
        assert_eq!(y, 0);
        assert!(on_curve);
    }

    // ============================================================
    // 9.  Flags 64–71: off-curve, 2-byte x, no y
    // ============================================================

    /// Flag 64 (idx=64, even, off-curve): b0=5 → x_raw = ((0)<<8)|5 = 5 → x=5
    #[test]
    fn flag_64_2byte_x_off_curve_positive() {
        let (x, y, on_curve) = decode(64, &[0x05]);
        // idx=64, even, has_x only, high_bits = (64&7)<<8 = 0, x_raw=5 → x=5
        assert_eq!(x, 5, "flag 64 b0=5: x should be 5");
        assert_eq!(y, 0);
        assert!(!on_curve, "flag 64: must be off-curve");
    }

    /// Flag 65 (idx=65, odd, off-curve): b0=0 → x_raw = (1<<8)|0 = 256 → x=-(256+1)=-257
    #[test]
    fn flag_65_2byte_x_off_curve_negative() {
        let (x, y, on_curve) = decode(65, &[0x00]);
        let idx = 65usize;
        let high_bits = ((idx & 0x07) as i32) << 8;
        let x_raw = high_bits; // b0=0x00, so | 0x00 is identity
        let expected_x = -(x_raw + 1);
        assert_eq!(x, expected_x, "flag 65 b0=0: x should be {expected_x}");
        assert_eq!(y, 0);
        assert!(!on_curve);
    }

    // ============================================================
    // 10. Flags 72–79: on-curve, 2-byte y, no x
    // ============================================================

    /// Flag 72 (idx=72, even, on-curve): b0=3 → y_raw = ((0)<<8)|3 = 3 → y=3
    #[test]
    fn flag_72_2byte_y_on_curve_positive() {
        let (x, y, on_curve) = decode(72, &[0x03]);
        // idx=72, even, has_y only, high_bits=(72&7)<<8 = 0, y_raw=3 → y=3
        assert_eq!(x, 0, "flag 72: x should be 0");
        assert_eq!(y, 3, "flag 72 b0=3: y should be 3");
        assert!(on_curve, "flag 72: must be on-curve");
    }

    /// Flag 73 (idx=73, odd, on-curve): b0=0 → y_raw=(1<<8)|0=256 → y=-(256+1)=-257
    #[test]
    fn flag_73_2byte_y_on_curve_negative() {
        let (x, y, on_curve) = decode(73, &[0x00]);
        let idx = 73usize;
        let high_bits = ((idx & 0x07) as i32) << 8;
        let y_raw = high_bits; // b0=0x00, so | 0x00 is identity
        let expected_y = -(y_raw + 1);
        assert_eq!(x, 0);
        assert_eq!(y, expected_y, "flag 73 b0=0: y should be {expected_y}");
        assert!(on_curve);
    }

    // ============================================================
    // 11. Flags 80–87: off-curve, 2-byte y, no x
    // ============================================================

    /// Flag 80 (idx=80, even, off-curve): b0=7 → y_raw=((0)<<8)|7=7 → y=7
    #[test]
    fn flag_80_2byte_y_off_curve_positive() {
        let (x, y, on_curve) = decode(80, &[0x07]);
        // idx=80, even, (80&7)=0, high_bits=0, y_raw=7 → y=7
        assert_eq!(x, 0);
        assert_eq!(y, 7, "flag 80 b0=7: y should be 7");
        assert!(!on_curve, "flag 80: must be off-curve");
    }

    /// Flag 81 (idx=81, odd, off-curve): b0=0xFF → y_raw=(1<<8)|255=511 → y=-(511+1)=-512
    #[test]
    fn flag_81_2byte_y_off_curve_negative() {
        let (x, y, on_curve) = decode(81, &[0xFF]);
        let idx = 81usize;
        let high_bits = ((idx & 0x07) as i32) << 8;
        let y_raw = high_bits | 0xFF;
        let expected_y = -(y_raw + 1);
        assert_eq!(x, 0);
        assert_eq!(y, expected_y, "flag 81 b0=0xFF: y should be {expected_y}");
        assert!(!on_curve);
    }

    // ============================================================
    // 12. Flags 88–95: on-curve, 1-byte x + 2-byte y
    //     b0 = x byte, b1 = y byte
    //     even idx → x=b0, y = y_high | b1  where y_high = ((idx&6)<<7)
    //     odd idx  → x=-(b0+1), y = -(y_high | b1 + 1)
    // ============================================================

    /// Flag 88 (idx=88, even, on-curve): b0=5, b1=0 → x=5, y_high=((88&6)<<7)=(0<<7)=0 → y=0
    #[test]
    fn flag_88_1byte_x_2byte_y_positive() {
        let (x, y, on_curve) = decode(88, &[0x05, 0x00]);
        // idx=88, even
        // x = b0 = 5
        // y_high = ((88 & 6) << 7) = (0 << 7) = 0
        // y_raw = 0 | 0 = 0 → y = 0
        assert_eq!(x, 5, "flag 88: x should be 5");
        assert_eq!(y, 0, "flag 88: y should be 0");
        assert!(on_curve, "flag 88: must be on-curve");
    }

    /// Flag 88 (idx=88, even): b0=10, b1=20 → x=10, y_high=0 → y=20
    #[test]
    fn flag_88_1byte_x_2byte_y_nonzero() {
        let (x, y, on_curve) = decode(88, &[0x0A, 0x14]);
        // idx=88, even, y_high=(88&6)<<7=(0<<7)=0
        // x=10, y=0|0x14=20
        assert_eq!(x, 10, "flag 88: x should be 10");
        assert_eq!(y, 20, "flag 88: y should be 20");
        assert!(on_curve);
    }

    /// Flag 89 (idx=89, odd, on-curve): b0=0, b1=0 → x=-(0+1)=-1, y_high=(1<<7)=128 → y=-(128+1)=-129
    #[test]
    fn flag_89_1byte_x_2byte_y_negative() {
        let (x, y, on_curve) = decode(89, &[0x00, 0x00]);
        let idx = 89usize;
        // x: odd → -(b0+1) = -(0+1) = -1
        // y_high = ((idx & 6) << 7) = ((89 & 6) << 7) = (0 << 7) = 0
        // Wait: 89 & 6 = 89 & 0b110 = 0b1011001 & 0b110 = 0b000 = 0
        // Actually 89 in binary = 0b1011001
        // 89 & 6 = 89 & 0b00000110 = 0b00000000 = 0
        // y_raw = 0 | b1 = 0, odd → y = -(0+1) = -1
        let y_high = ((idx & 6) as i32) << 7;
        let y_raw = y_high; // b1=0x00, so | 0x00 is identity
        let expected_x = -1i32; // -(0 + 1) = -1
        let expected_y = -(y_raw + 1);
        assert_eq!(x, expected_x, "flag 89: x should be {expected_x}");
        assert_eq!(y, expected_y, "flag 89: y should be {expected_y}");
        assert!(on_curve);
    }

    // ============================================================
    // 13. Flags 96–103: off-curve, 1-byte x + 2-byte y
    // ============================================================

    /// Flag 96 (idx=96, even, off-curve): b0=3, b1=7 → x=3, y_high=((96&6)<<7)=(0<<7)=0 → y=7
    #[test]
    fn flag_96_1byte_x_2byte_y_off_curve() {
        let (x, y, on_curve) = decode(96, &[0x03, 0x07]);
        // idx=96, even: 96&6 = 0 → y_high=0
        assert_eq!(x, 3, "flag 96: x should be 3");
        assert_eq!(y, 7, "flag 96: y should be 7");
        assert!(!on_curve, "flag 96: must be off-curve");
    }

    // ============================================================
    // 14. Flags 104–111: on-curve, 2-byte x + 1-byte y
    //     b0 = high x byte, b1 = y byte
    //     even → x = x_high | b0, y = b1
    //     odd  → x = -(x_high | b0 + 1), y = -(b1 + 1)
    // ============================================================

    /// Flag 104 (idx=104, even, on-curve): b0=2, b1=5 →
    ///   x_high = ((104&6)<<7) = (0<<7) = 0 → x=2, y=5
    #[test]
    fn flag_104_2byte_x_1byte_y_positive() {
        let (x, y, on_curve) = decode(104, &[0x02, 0x05]);
        // idx=104, even, (104&6)=0 → x_high=0, x_raw=0|2=2 → x=2
        // y = b1 = 5 (even → y=5)
        assert_eq!(x, 2, "flag 104: x should be 2");
        assert_eq!(y, 5, "flag 104: y should be 5");
        assert!(on_curve, "flag 104: must be on-curve");
    }

    /// Flag 105 (idx=105, odd, on-curve): b0=0, b1=0 →
    ///   x_high=((105&6)<<7)=(0<<7)=0, x_raw=0, x=-(0+1)=-1
    ///   y=-(0+1)=-1
    #[test]
    fn flag_105_2byte_x_1byte_y_negative() {
        let (x, y, on_curve) = decode(105, &[0x00, 0x00]);
        // idx=105, 105&6=0 → x_high=0, x_raw=0, odd → x=-(0+1)=-1
        // y=b1=0, odd → y=-(0+1)=-1
        assert_eq!(x, -1, "flag 105: x should be -1");
        assert_eq!(y, -1, "flag 105: y should be -1");
        assert!(on_curve);
    }

    // ============================================================
    // 15. Flags 112–119: off-curve, 2-byte x + 1-byte y
    // ============================================================

    /// Flag 112 (idx=112, even, off-curve): b0=0x10, b1=0x08 →
    ///   x_high=((112&6)<<7)=(0<<7)=0, x_raw=0x10=16 → x=16, y=8
    #[test]
    fn flag_112_2byte_x_1byte_y_off_curve() {
        let (x, y, on_curve) = decode(112, &[0x10, 0x08]);
        // idx=112, even, (112&6)=0 → x_high=0, x_raw=0x10=16 → x=16
        // y=b1=8, even → y=8
        assert_eq!(x, 16, "flag 112: x should be 16");
        assert_eq!(y, 8, "flag 112: y should be 8");
        assert!(!on_curve, "flag 112: must be off-curve");
    }

    // ============================================================
    // 16. Flags 120–123: 2-byte x + 2-byte y (i16 big-endian each)
    // ============================================================

    /// Flag 120 (on-curve): bytes [0x00, 0x64, 0x01, 0x00] → x=100, y=256
    #[test]
    fn flag_120_2byte_xy_on_curve() {
        let (x, y, on_curve) = decode(120, &[0x00, 0x64, 0x01, 0x00]);
        // i16::from_be_bytes([0x00, 0x64]) = 100
        // i16::from_be_bytes([0x01, 0x00]) = 256
        assert_eq!(x, 100, "flag 120: x should be 100");
        assert_eq!(y, 256, "flag 120: y should be 256");
        assert!(on_curve, "flag 120: must be on-curve");
    }

    /// Flag 121 (on-curve): bytes [0xFF, 0x80, 0xFF, 0xFE] → x=-128, y=-2
    #[test]
    fn flag_121_2byte_xy_negative_on_curve() {
        let (x, y, on_curve) = decode(121, &[0xFF, 0x80, 0xFF, 0xFE]);
        // i16::from_be_bytes([0xFF, 0x80]) = -128
        // i16::from_be_bytes([0xFF, 0xFE]) = -2
        assert_eq!(x, -128, "flag 121: x should be -128");
        assert_eq!(y, -2, "flag 121: y should be -2");
        assert!(on_curve);
    }

    /// Flag 122 (off-curve): bytes [0x00, 0x00, 0x00, 0x00] → x=0, y=0
    #[test]
    fn flag_122_2byte_xy_off_curve_zeros() {
        let (x, y, on_curve) = decode(122, &[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(x, 0, "flag 122: x should be 0");
        assert_eq!(y, 0, "flag 122: y should be 0");
        assert!(!on_curve, "flag 122: must be off-curve");
    }

    /// Flag 123 (off-curve): bytes [0x7F, 0xFF, 0x80, 0x00] → x=i16::MAX=32767, y=i16::MIN=-32768
    #[test]
    fn flag_123_2byte_xy_off_curve_extremes() {
        let (x, y, on_curve) = decode(123, &[0x7F, 0xFF, 0x80, 0x00]);
        // i16::from_be_bytes([0x7F, 0xFF]) = 32767
        // i16::from_be_bytes([0x80, 0x00]) = -32768
        assert_eq!(x, 32767, "flag 123: x should be i16::MAX");
        assert_eq!(y, -32768, "flag 123: y should be i16::MIN");
        assert!(!on_curve);
    }

    // ============================================================
    // 17. Flags 124–127: 4-byte x + 4-byte y (i32 big-endian)
    // ============================================================

    /// Flag 124 (on-curve): i32 0 and i32 0.
    #[test]
    fn flag_124_4byte_xy_on_curve_zeros() {
        let bytes = [0u8; 8];
        let (x, y, on_curve) = decode(124, &bytes);
        assert_eq!(x, 0, "flag 124: x should be 0");
        assert_eq!(y, 0, "flag 124: y should be 0");
        assert!(on_curve, "flag 124: must be on-curve");
    }

    /// Flag 124 (on-curve): x=1_000_000, y=-500.
    #[test]
    fn flag_124_4byte_xy_on_curve_large() {
        let x_val: i32 = 1_000_000;
        let y_val: i32 = -500;
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&x_val.to_be_bytes());
        bytes[4..8].copy_from_slice(&y_val.to_be_bytes());
        let (x, y, on_curve) = decode(124, &bytes);
        assert_eq!(x, x_val, "flag 124: x should be 1_000_000");
        assert_eq!(y, y_val, "flag 124: y should be -500");
        assert!(on_curve);
    }

    /// Flag 126 (off-curve): x=i32::MAX, y=i32::MIN.
    #[test]
    fn flag_126_4byte_xy_off_curve_extremes() {
        let x_val: i32 = i32::MAX;
        let y_val: i32 = i32::MIN;
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&x_val.to_be_bytes());
        bytes[4..8].copy_from_slice(&y_val.to_be_bytes());
        let (x, y, on_curve) = decode(126, &bytes);
        assert_eq!(x, x_val, "flag 126: x should be i32::MAX");
        assert_eq!(y, y_val, "flag 126: y should be i32::MIN");
        assert!(!on_curve, "flag 126: must be off-curve");
    }

    /// Flag 125 (on-curve): x=-1, y=1.
    #[test]
    fn flag_125_4byte_xy_on_curve_signed() {
        let x_val: i32 = -1;
        let y_val: i32 = 1;
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&x_val.to_be_bytes());
        bytes[4..8].copy_from_slice(&y_val.to_be_bytes());
        let (x, y, on_curve) = decode(125, &bytes);
        assert_eq!(x, x_val);
        assert_eq!(y, y_val);
        assert!(on_curve);
    }

    /// Flag 127 (off-curve): x=0, y=-1.
    #[test]
    fn flag_127_4byte_xy_off_curve() {
        let x_val: i32 = 0;
        let y_val: i32 = -1;
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&x_val.to_be_bytes());
        bytes[4..8].copy_from_slice(&y_val.to_be_bytes());
        let (x, y, on_curve) = decode(127, &bytes);
        assert_eq!(x, x_val);
        assert_eq!(y, y_val);
        assert!(!on_curve);
    }

    // ============================================================
    // 18. Error cases: glyph stream exhausted
    // ============================================================

    /// Flag 40 (requires 1 glyph byte) with empty slice → MalformedGlyfTransform.
    #[test]
    fn flag_40_empty_glyph_stream_errors() {
        use oxifont_webfont::WebFontError;
        let result = decode_triplet(40, &[]);
        assert!(
            matches!(result, Err(WebFontError::MalformedGlyfTransform(_))),
            "flag 40 with empty glyph stream must return MalformedGlyfTransform"
        );
    }

    /// Flag 88 (requires 2 glyph bytes) with only 1 byte → MalformedGlyfTransform.
    #[test]
    fn flag_88_short_glyph_stream_errors() {
        use oxifont_webfont::WebFontError;
        let result = decode_triplet(88, &[0x05]); // only 1 byte, needs 2
        assert!(
            matches!(result, Err(WebFontError::MalformedGlyfTransform(_))),
            "flag 88 with 1-byte glyph stream must return MalformedGlyfTransform"
        );
    }

    /// Flag 120 (requires 4 glyph bytes) with only 3 bytes → MalformedGlyfTransform.
    #[test]
    fn flag_120_short_glyph_stream_errors() {
        use oxifont_webfont::WebFontError;
        let result = decode_triplet(120, &[0x00, 0x01, 0x00]); // only 3 bytes, needs 4
        assert!(
            matches!(result, Err(WebFontError::MalformedGlyfTransform(_))),
            "flag 120 with 3-byte glyph stream must return MalformedGlyfTransform"
        );
    }

    /// Flag 124 (requires 8 glyph bytes) with only 7 bytes → MalformedGlyfTransform.
    #[test]
    fn flag_124_short_glyph_stream_errors() {
        use oxifont_webfont::WebFontError;
        let result = decode_triplet(124, &[0u8; 7]); // only 7 bytes, needs 8
        assert!(
            matches!(result, Err(WebFontError::MalformedGlyfTransform(_))),
            "flag 124 with 7-byte glyph stream must return MalformedGlyfTransform"
        );
    }

    // ============================================================
    // 19. Bit-7 masking in decode_triplet: f and f|0x80 same result
    // ============================================================

    /// Flag 0 and 0x80 should produce identical results (bit 7 is masked).
    #[test]
    fn decode_triplet_high_bit_masked_flag_0() {
        let r0 = decode(0, &[]);
        let r128 = decode(0x80, &[]);
        assert_eq!(r0, r128, "flag 0 and 0x80 must decode identically");
    }

    /// Flag 40 and 0xA8 (40|0x80) should produce identical results.
    #[test]
    fn decode_triplet_high_bit_masked_flag_40() {
        let r40 = decode(40, &[0x12]);
        let r_high = decode(40 | 0x80, &[0x12]);
        assert_eq!(r40, r_high, "flag 40 and 0xA8 must decode identically");
    }

    /// Flag 120 and 0xF8 (120|0x80) should produce identical results.
    #[test]
    fn decode_triplet_high_bit_masked_flag_120() {
        let bytes = [0x00u8, 0x64, 0x00, 0x32];
        let r120 = decode(120, &bytes);
        let r_high = decode(120 | 0x80, &bytes);
        assert_eq!(r120, r_high, "flag 120 and 0xF8 must decode identically");
    }
}
