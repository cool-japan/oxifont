//! WOFF2 transformed hmtx reconstruction.
//!
//! # WOFF2 Spec Reference: §6.1.1 (hmtx table transform)
//!
//! The WOFF2 `hmtx` transform compresses the horizontal metrics table by
//! encoding the advance-width array and, optionally, the left side bearing (lsb)
//! arrays in a compact stream. The `optionFlags` byte (the first byte of the
//! transformed hmtx sub-stream) controls which arrays are present:
//!
//! - **Bit 0 (`0x01`) — `FLAG_LSB_OMITTED_PROPORTIONAL`**: If set,
//!   the proportional lsb values for the `numberOfHMetrics` entries are omitted
//!   from the stream. These lsbs **must** be reconstructed from the `glyf` table's
//!   per-glyph `xMin` value (glyph header bytes 2–3). Reconstruction formula:
//!   `lsb[gid] = glyf_xMin[gid]`. If `glyf` is unavailable, the fallback is `0`.
//!
//! - **Bit 1 (`0x02`) — `FLAG_LSB_OMITTED_MONO`**: If set, the monospace lsb
//!   values for glyphs beyond `numberOfHMetrics` (i.e., `numGlyphs – numHMetrics`
//!   entries) are omitted. Same reconstruction formula as bit 0.
//!
//! - **Both bits = 0**: All lsb values are present in the stream (standard path).
//!
//! The advance-width array is always present in the transformed stream.
//!
//! ## Implementation Status
//!
//! Both bit 0 and bit 1 branches are fully implemented in [`reconstruct_hmtx`]:
//! - When a flag bit is set, the corresponding lsb sub-array is not read from the
//!   stream; instead, values are drawn from the `glyf_xmins` slice (or zeroed if
//!   `glyf_xmins` is shorter than the glyph index).
//! - When a flag bit is clear, the lsb sub-array is read normally as a sequence
//!   of `int16` big-endian values.
//! - The `get_xmin` closure in [`reconstruct_hmtx`] provides the safe fallback
//!   (`0`) for missing glyph indices.

use crate::error::WebFontError;

// Flag bits in the transformed-hmtx flags byte (optionFlags, WOFF2 spec §6.1.1).
/// Bit 0 (`0x01`): proportional glyph lsbs omitted; reconstruct from `glyf` xMin.
const FLAG_LSB_OMITTED_PROPORTIONAL: u8 = 0x01;
/// Bit 1 (`0x02`): monospace glyph lsbs (beyond `numberOfHMetrics`) omitted; reconstruct from `glyf` xMin.
const FLAG_LSB_OMITTED_MONO: u8 = 0x02;

/// Reconstruct the `hmtx` table from the WOFF2 transformed representation.
///
/// Parameters:
/// - `transform_data`: the transformed hmtx sub-stream (after brotli decompress + offset).
/// - `num_glyphs`: total number of glyphs in the font.
/// - `num_h_metrics`: numberOfHMetrics from the `hhea` table.
/// - `glyf_xmins`: per-glyph xMin extracted from the reconstructed `glyf` table.
///   Pass an empty slice if glyf is unavailable (lsbs will be zeroed).
pub fn reconstruct_hmtx(
    transform_data: &[u8],
    num_glyphs: u16,
    num_h_metrics: u16,
    glyf_xmins: &[i16],
) -> Result<Vec<u8>, WebFontError> {
    if transform_data.is_empty() {
        return Err(WebFontError::MalformedGlyfTransform(
            "transformed hmtx data is empty".to_string(),
        ));
    }

    let flags = transform_data[0];
    let mut pos = 1usize;

    let proportional_lsbs_omitted = (flags & FLAG_LSB_OMITTED_PROPORTIONAL) != 0;
    let mono_lsbs_omitted = (flags & FLAG_LSB_OMITTED_MONO) != 0;

    let num_h_metrics = num_h_metrics as usize;
    let num_glyphs = num_glyphs as usize;

    // Read advance widths (uint16, num_h_metrics entries).
    let advance_widths = read_u16_array(transform_data, &mut pos, num_h_metrics)?;

    // Read proportional lsbs (present unless bit 0 set).
    let proportional_lsbs: Vec<i16> = if proportional_lsbs_omitted {
        vec![0i16; num_h_metrics]
    } else {
        read_i16_array(transform_data, &mut pos, num_h_metrics)?
    };

    // Read monospace lsbs for glyphs beyond num_h_metrics.
    let mono_count = num_glyphs.saturating_sub(num_h_metrics);
    let mono_lsbs: Vec<i16> = if mono_lsbs_omitted || mono_count == 0 {
        vec![0i16; mono_count]
    } else {
        read_i16_array(transform_data, &mut pos, mono_count)?
    };

    // Reconstruct from glyf xMin if lsbs were omitted.
    let get_xmin = |gid: usize| -> i16 { glyf_xmins.get(gid).copied().unwrap_or(0) };

    // Build standard hmtx layout:
    // - num_h_metrics × (advanceWidth uint16 + lsb int16)
    // - (num_glyphs - num_h_metrics) × lsb int16 (uses last advanceWidth)
    let mut hmtx = Vec::with_capacity(num_h_metrics * 4 + mono_count * 2);

    for i in 0..num_h_metrics {
        let aw = advance_widths[i];
        let lsb = if proportional_lsbs_omitted {
            get_xmin(i)
        } else {
            proportional_lsbs[i]
        };
        hmtx.extend_from_slice(&aw.to_be_bytes());
        hmtx.extend_from_slice(&lsb.to_be_bytes());
    }

    for (j, &mono_lsb) in (0..mono_count).zip(mono_lsbs.iter().chain(std::iter::repeat(&0i16))) {
        let gid = num_h_metrics + j;
        let lsb = if mono_lsbs_omitted {
            get_xmin(gid)
        } else {
            mono_lsb
        };
        hmtx.extend_from_slice(&lsb.to_be_bytes());
    }

    Ok(hmtx)
}

/// Extract xMin values from a reconstructed glyf table.
///
/// Each non-empty glyph starts with: nContours (int16) + xMin (int16) + …
/// Empty glyphs (`loca[i] == loca[i+1]`) have no data; xMin defaults to 0.
pub fn extract_glyf_xmins(glyf: &[u8], loca_offsets: &[u32], num_glyphs: u16) -> Vec<i16> {
    let n = num_glyphs as usize;
    let mut xmins = Vec::with_capacity(n);

    for i in 0..n {
        let start = loca_offsets.get(i).copied().unwrap_or(0) as usize;
        let end = loca_offsets.get(i + 1).copied().unwrap_or(start as u32) as usize;

        if start == end || start + 6 > glyf.len() {
            xmins.push(0i16);
            continue;
        }

        // xMin is at bytes 2–3 of the glyph header.
        let xmin = i16::from_be_bytes([glyf[start + 2], glyf[start + 3]]);
        xmins.push(xmin);
    }

    xmins
}

// ----------------------------------------------------------------- helpers

fn read_u16_array(data: &[u8], pos: &mut usize, count: usize) -> Result<Vec<u16>, WebFontError> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if *pos + 2 > data.len() {
            return Err(WebFontError::TooShort);
        }
        let v = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
        *pos += 2;
        out.push(v);
    }
    Ok(out)
}

fn read_i16_array(data: &[u8], pos: &mut usize, count: usize) -> Result<Vec<i16>, WebFontError> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if *pos + 2 > data.len() {
            return Err(WebFontError::TooShort);
        }
        let v = i16::from_be_bytes([data[*pos], data[*pos + 1]]);
        *pos += 2;
        out.push(v);
    }
    Ok(out)
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_xmin_empty_glyph() {
        let xmins = extract_glyf_xmins(&[], &[0, 0], 1);
        assert_eq!(xmins, &[0]);
    }

    #[test]
    fn extract_xmin_simple() {
        // Minimal glyph header: nContours=1, xMin=100, yMin=0, xMax=500, yMax=700.
        let mut glyf = Vec::new();
        glyf.extend_from_slice(&1i16.to_be_bytes()); // nContours
        glyf.extend_from_slice(&100i16.to_be_bytes()); // xMin
        glyf.extend_from_slice(&0i16.to_be_bytes()); // yMin
        glyf.extend_from_slice(&500i16.to_be_bytes()); // xMax
        glyf.extend_from_slice(&700i16.to_be_bytes()); // yMax

        let loca = [0u32, glyf.len() as u32];
        let xmins = extract_glyf_xmins(&glyf, &loca, 1);
        assert_eq!(xmins, &[100]);
    }

    // ---- reconstruct_hmtx unit tests (flags combinations) ----

    /// Build a minimal transformed-hmtx byte stream.
    ///
    /// Layout: flags(1) + advance_widths(n×u16) + [prop_lsbs(n×i16)] + [mono_lsbs(m×i16)]
    fn build_transformed_hmtx(
        flags: u8,
        advance_widths: &[u16],
        prop_lsbs: Option<&[i16]>, // None → omitted (bit 0 set)
        mono_lsbs: Option<&[i16]>, // None → omitted (bit 1 set)
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(flags);
        for &aw in advance_widths {
            data.extend_from_slice(&aw.to_be_bytes());
        }
        if let Some(lsbs) = prop_lsbs {
            for &lsb in lsbs {
                data.extend_from_slice(&lsb.to_be_bytes());
            }
        }
        if let Some(lsbs) = mono_lsbs {
            for &lsb in lsbs {
                data.extend_from_slice(&lsb.to_be_bytes());
            }
        }
        data
    }

    /// flags == 0x00: all lsbs present in stream — no glyf xMin substitution.
    #[test]
    fn reconstruct_hmtx_no_flags_all_lsbs_in_stream() {
        // 2 glyphs, 2 h-metrics, no mono-lsb section.
        let advance_widths = [600u16, 500u16];
        let prop_lsbs = [10i16, 20i16];
        let stream = build_transformed_hmtx(
            0x00,
            &advance_widths,
            Some(&prop_lsbs),
            None, // no mono-glyph lsbs (num_glyphs == num_h_metrics)
        );
        let glyf_xmins = [50i16, 60i16]; // ignored because flag 0x00
        let out = reconstruct_hmtx(&stream, 2, 2, &glyf_xmins).expect("should reconstruct");

        // Expected: [aw0 lsb0 aw1 lsb1] = [600 10 500 20]
        let mut expected = Vec::new();
        expected.extend_from_slice(&600u16.to_be_bytes());
        expected.extend_from_slice(&10i16.to_be_bytes());
        expected.extend_from_slice(&500u16.to_be_bytes());
        expected.extend_from_slice(&20i16.to_be_bytes());
        assert_eq!(out, expected, "flags=0x00: lsbs must come from stream");
    }

    /// flags == 0x01 (FLAG_LSB_OMITTED_PROPORTIONAL): proportional lsbs reconstructed from glyf xMin.
    #[test]
    fn reconstruct_hmtx_flag_01_proportional_lsbs_from_glyf() {
        // 2 proportional glyphs, 0 monospace glyphs.
        let advance_widths = [800u16, 750u16];
        // Proportional lsbs omitted (bit 0 set) — use glyf xMin.
        let stream = build_transformed_hmtx(0x01, &advance_widths, None, None);
        let glyf_xmins = [33i16, 44i16];
        let out = reconstruct_hmtx(&stream, 2, 2, &glyf_xmins).expect("should reconstruct");

        // Expected: [aw0 xmin0 aw1 xmin1]
        let mut expected = Vec::new();
        expected.extend_from_slice(&800u16.to_be_bytes());
        expected.extend_from_slice(&33i16.to_be_bytes()); // from glyf_xmins[0]
        expected.extend_from_slice(&750u16.to_be_bytes());
        expected.extend_from_slice(&44i16.to_be_bytes()); // from glyf_xmins[1]
        assert_eq!(
            out, expected,
            "flags=0x01: proportional lsbs from glyf xMin"
        );
    }

    /// flags == 0x02 (FLAG_LSB_OMITTED_MONO): monospace lsbs reconstructed from glyf xMin.
    #[test]
    fn reconstruct_hmtx_flag_02_mono_lsbs_from_glyf() {
        // 1 proportional glyph + 1 mono glyph, num_h_metrics=1, num_glyphs=2.
        // Proportional lsbs are in stream; mono lsbs omitted (bit 1 set).
        let advance_widths = [600u16]; // only 1 advance width (num_h_metrics=1)
        let prop_lsbs = [15i16];
        let stream = build_transformed_hmtx(0x02, &advance_widths, Some(&prop_lsbs), None);
        let glyf_xmins = [0i16, 77i16]; // [0]=proportional (not used), [1]=mono

        let out = reconstruct_hmtx(&stream, 2, 1, &glyf_xmins).expect("should reconstruct");

        // Expected: [aw0 lsb0(from stream=15)] [lsb1(from glyf_xmins[1]=77)]
        let mut expected = Vec::new();
        expected.extend_from_slice(&600u16.to_be_bytes());
        expected.extend_from_slice(&15i16.to_be_bytes());
        expected.extend_from_slice(&77i16.to_be_bytes()); // mono from glyf xMin
        assert_eq!(out, expected, "flags=0x02: mono lsbs from glyf xMin");
    }

    /// flags == 0x03 (both bits set): both proportional and mono lsbs from glyf xMin.
    #[test]
    fn reconstruct_hmtx_flag_03_all_lsbs_from_glyf() {
        // 2 proportional + 1 mono.
        let advance_widths = [700u16, 650u16];
        let stream = build_transformed_hmtx(0x03, &advance_widths, None, None);
        let glyf_xmins = [11i16, 22i16, 33i16];

        let out = reconstruct_hmtx(&stream, 3, 2, &glyf_xmins).expect("should reconstruct");

        let mut expected = Vec::new();
        expected.extend_from_slice(&700u16.to_be_bytes());
        expected.extend_from_slice(&11i16.to_be_bytes()); // prop from glyf xMin[0]
        expected.extend_from_slice(&650u16.to_be_bytes());
        expected.extend_from_slice(&22i16.to_be_bytes()); // prop from glyf xMin[1]
        expected.extend_from_slice(&33i16.to_be_bytes()); // mono from glyf xMin[2]
        assert_eq!(out, expected, "flags=0x03: all lsbs from glyf xMin");
    }
}
