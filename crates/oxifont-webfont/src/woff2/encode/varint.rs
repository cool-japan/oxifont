//! Variable-length integer encoders for WOFF2.
//!
//! Implements `UIntBase128` (for the WOFF2 table directory) and
//! `255UInt16` (for nPoints per contour and instruction lengths in the
//! transformed glyf block).

// --------------------------------------------------------------- UIntBase128

/// Write a WOFF2 `UIntBase128` unsigned integer to `out`.
///
/// Each byte contributes 7 bits; the MSB is the continuation bit.
/// The most-significant byte is written first (big-endian bit order).
pub fn encode_uint_base128(out: &mut Vec<u8>, value: u32) {
    // Collect up to 5 7-bit groups, LSB-first, then reverse.
    let mut buf = [0u8; 5];
    let mut len = 0usize;
    let mut v = value;
    loop {
        buf[len] = (v & 0x7F) as u8;
        len += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    // Write MSB-first with continuation bits set on all but the last byte.
    for i in (0..len).rev() {
        let byte = buf[i];
        if i > 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte); // last byte: no continuation bit
        }
    }
}

// --------------------------------------------------------------- 255UInt16

/// Write a WOFF2 `255UInt16` value to `out`.
///
/// This is the inverse of `read_255_uint16` in `woff2/glyf.rs`.
///
/// Decoder reference (from glyf.rs):
/// - b0 < 253: value = b0 (1 byte total)
/// - b0 == 253: value = next_u16_be (3 bytes total, range 0..65535)
/// - b0 == 254: value = next_u16_be + 506 (3 bytes total, range 506..66041)
/// - b0 == 255: value = next_u8 + 253 (2 bytes total, range 253..508)
///
/// Encoding scheme (using the most compact representation):
/// - 0..=252: 1 byte `[value]`
/// - 253..=508: 2 bytes `[255, value - 253]`
/// - else (509..=65535): 3 bytes `[253, high, low]`
pub fn encode_255_u16(out: &mut Vec<u8>, value: u16) {
    if value <= 252 {
        out.push(value as u8);
    } else if value <= 508 {
        // b0=255, b1=(value - 253) which decodes to b1 + 253 = value.
        out.push(255u8);
        out.push((value - 253) as u8);
    } else {
        // b0=253, followed by big-endian uint16 = value directly.
        out.push(253u8);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------- UIntBase128

    #[test]
    fn uint_base128_zero() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 0);
        assert_eq!(out, [0x00]);
    }

    #[test]
    fn uint_base128_one() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 1);
        assert_eq!(out, [0x01]);
    }

    #[test]
    fn uint_base128_127() {
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 127);
        assert_eq!(out, [0x7F]);
    }

    #[test]
    fn uint_base128_128() {
        // 128 = 0b1000_0000 → two bytes: (1 << 7) | 0 → [0x81, 0x00]
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 128);
        assert_eq!(out, [0x81, 0x00]);
    }

    #[test]
    fn uint_base128_16383() {
        // 16383 = 0x3FFF = 0b0111_1111_1111_1111 → 2 bytes: [0xFF, 0x7F]
        let mut out = Vec::new();
        encode_uint_base128(&mut out, 16383);
        assert_eq!(out, [0xFF, 0x7F]);
    }

    #[test]
    fn uint_base128_round_trip() {
        use crate::woff2::header::decode_uint_base128;
        for value in [
            0u32,
            1,
            63,
            127,
            128,
            255,
            256,
            16383,
            16384,
            (1 << 21) - 1,
            1 << 21,
            (1 << 28) - 1,
        ] {
            let mut out = Vec::new();
            encode_uint_base128(&mut out, value);
            let (decoded, consumed) = decode_uint_base128(&out).expect("should decode");
            assert_eq!(decoded, value, "round-trip failed for {value}");
            assert_eq!(consumed, out.len(), "consumed != length for {value}");
        }
    }

    // ---------------------------------------------------------- 255UInt16

    #[test]
    fn encode_255_u16_direct_small() {
        let mut out = Vec::new();
        encode_255_u16(&mut out, 42);
        assert_eq!(out, [42]);
    }

    #[test]
    fn encode_255_u16_zero() {
        let mut out = Vec::new();
        encode_255_u16(&mut out, 0);
        assert_eq!(out, [0]);
    }

    #[test]
    fn encode_255_u16_252() {
        let mut out = Vec::new();
        encode_255_u16(&mut out, 252);
        assert_eq!(out, [252]);
    }

    #[test]
    fn encode_255_u16_253_two_bytes() {
        // 253 → [255, 0] (b0=255, b1=253-253=0; decoder: 0 + 253 = 253)
        let mut out = Vec::new();
        encode_255_u16(&mut out, 253);
        assert_eq!(out, [255, 0]);
    }

    #[test]
    fn encode_255_u16_508_three_bytes() {
        // 508 → [255, 255] (b0=255, b1=508-253=255; decoder: 255 + 253 = 508)
        let mut out = Vec::new();
        encode_255_u16(&mut out, 508);
        assert_eq!(out, [255, 255]);
    }

    #[test]
    fn encode_255_u16_509_three_bytes() {
        // 509 → [253, 0x01, 0xFD] (b0=253, 3-byte form: value = 509)
        let mut out = Vec::new();
        encode_255_u16(&mut out, 509);
        assert_eq!(out, [253, 0x01, 0xFD]);
    }

    #[test]
    fn encode_255_u16_roundtrip_via_glyf() {
        // Verify encode/decode round-trip using values spanning all branches.
        for value in [0u16, 1, 252, 253, 254, 507, 508, 509, 1000, 65535] {
            let mut encoded = Vec::new();
            encode_255_u16(&mut encoded, value);
            let decoded = decode_255_u16_test(&encoded);
            assert_eq!(decoded, value, "round-trip failed for {value}");
        }
    }

    /// Inline re-implementation of `read_255_uint16` logic for test verification.
    ///
    /// Matches `woff2/glyf.rs:read_255_uint16`.
    ///
    /// - `253`: next 2 bytes are a big-endian uint16 (value as-is).
    /// - `254`: next 2 bytes big-endian uint16 + 506.
    /// - `255`: next 1 byte + 253.
    /// - else: the byte value itself.
    fn decode_255_u16_test(data: &[u8]) -> u16 {
        let b0 = data[0];
        match b0 {
            253 => u16::from_be_bytes([data[1], data[2]]),
            254 => u16::from_be_bytes([data[1], data[2]]).wrapping_add(506),
            255 => (data[1] as u16).wrapping_add(253),
            _ => b0 as u16,
        }
    }
}
