//! WOFF2 glyf/loca forward transform (encoder).
//!
//! Produces the transformed glyf block as described in W3C WOFF2 §5.1.
//! This is the inverse of the reconstruction performed by `woff2/glyf.rs`.

use super::varint::encode_255_u16;
use crate::error::WebFontError;

// ------------------------------------------------------------------ constants

/// Transformed glyf sub-header size (36 bytes).
const GLYF_HEADER_SIZE: usize = 36;

// ------------------------------------------------------------ simple struct

/// A parsed simple glyph from the `glyf` table.
struct SimpleGlyph {
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
    end_pts: Vec<u16>,
    instructions: Vec<u8>,
    /// Per-point: (x_delta, y_delta, on_curve).
    points: Vec<(i32, i32, bool)>,
}

// --------------------------------------------------------------- glyf parser

/// Read a big-endian i16 from a slice.
fn read_i16(data: &[u8], offset: usize) -> Result<i16, WebFontError> {
    data.get(offset..offset + 2)
        .ok_or(WebFontError::TooShort)
        .map(|b| i16::from_be_bytes([b[0], b[1]]))
}

/// Read a big-endian u16 from a slice.
fn read_u16(data: &[u8], offset: usize) -> Result<u16, WebFontError> {
    data.get(offset..offset + 2)
        .ok_or(WebFontError::TooShort)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// Parse the loca table into an array of glyph byte offsets in `glyf`.
///
/// `index_format` == 0 → short loca (uint16 × 2), 1 → long loca (uint32).
fn parse_loca(loca: &[u8], index_format: u16) -> Result<Vec<u32>, WebFontError> {
    if index_format == 0 {
        if !loca.len().is_multiple_of(2) {
            return Err(WebFontError::MalformedGlyfTransform(
                "short loca odd length".to_string(),
            ));
        }
        let mut offsets = Vec::with_capacity(loca.len() / 2);
        for chunk in loca.chunks_exact(2) {
            let short = u16::from_be_bytes([chunk[0], chunk[1]]);
            offsets.push((short as u32) * 2);
        }
        Ok(offsets)
    } else {
        if !loca.len().is_multiple_of(4) {
            return Err(WebFontError::MalformedGlyfTransform(
                "long loca not multiple of 4".to_string(),
            ));
        }
        let mut offsets = Vec::with_capacity(loca.len() / 4);
        for chunk in loca.chunks_exact(4) {
            offsets.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(offsets)
    }
}

/// Parse a simple glyph from `glyf` table bytes starting at `data`.
///
/// `n_contours` must be > 0.
fn parse_simple_glyph(data: &[u8], n_contours: usize) -> Result<SimpleGlyph, WebFontError> {
    let n_c = n_contours as i16;
    let x_min = read_i16(data, 2)?;
    let y_min = read_i16(data, 4)?;
    let x_max = read_i16(data, 6)?;
    let y_max = read_i16(data, 8)?;

    // endPtsOfContours: n_contours uint16 values, at offset 10.
    let mut end_pts = Vec::with_capacity(n_contours);
    let mut pos = 10usize;
    for _ in 0..n_contours {
        let ep = read_u16(data, pos)?;
        end_pts.push(ep);
        pos += 2;
    }

    // Total points = last endPt + 1.
    let total_points = if let Some(&last) = end_pts.last() {
        last as usize + 1
    } else {
        0
    };

    // instructionLength (uint16)
    if pos + 2 > data.len() {
        return Err(WebFontError::TooShort);
    }
    let instr_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;

    // instructions
    if pos + instr_len > data.len() {
        return Err(WebFontError::TooShort);
    }
    let instructions = data[pos..pos + instr_len].to_vec();
    pos += instr_len;

    // flags (one byte per point, with possible run-length encoding via REPEAT_FLAG = 0x08).
    let mut raw_flags = Vec::with_capacity(total_points);
    while raw_flags.len() < total_points {
        if pos >= data.len() {
            return Err(WebFontError::TooShort);
        }
        let flag = data[pos];
        pos += 1;
        raw_flags.push(flag);
        if flag & 0x08 != 0 {
            // REPEAT_FLAG: next byte is repeat count.
            if pos >= data.len() {
                return Err(WebFontError::TooShort);
            }
            let repeat = data[pos] as usize;
            pos += 1;
            for _ in 0..repeat {
                if raw_flags.len() >= total_points {
                    break;
                }
                raw_flags.push(flag);
            }
        }
    }
    if raw_flags.len() != total_points {
        return Err(WebFontError::MalformedGlyfTransform(
            "flag count mismatch".to_string(),
        ));
    }

    // x-coordinates.
    let mut x_coords = Vec::with_capacity(total_points);
    for &flag in &raw_flags {
        let x: i16 = if flag & 0x02 != 0 {
            // X_SHORT_VECTOR
            if pos >= data.len() {
                return Err(WebFontError::TooShort);
            }
            let magnitude = data[pos] as i16;
            pos += 1;
            if flag & 0x10 != 0 {
                magnitude
            } else {
                -magnitude
            }
        } else if flag & 0x10 != 0 {
            // SAME_X: repeat previous → delta = 0.
            0
        } else {
            // 2-byte signed delta.
            if pos + 2 > data.len() {
                return Err(WebFontError::TooShort);
            }
            let v = i16::from_be_bytes([data[pos], data[pos + 1]]);
            pos += 2;
            v
        };
        x_coords.push(x);
    }

    // y-coordinates.
    let mut y_coords = Vec::with_capacity(total_points);
    for &flag in &raw_flags {
        let y: i16 = if flag & 0x04 != 0 {
            // Y_SHORT_VECTOR
            if pos >= data.len() {
                return Err(WebFontError::TooShort);
            }
            let magnitude = data[pos] as i16;
            pos += 1;
            if flag & 0x20 != 0 {
                magnitude
            } else {
                -magnitude
            }
        } else if flag & 0x20 != 0 {
            // SAME_Y
            0
        } else {
            if pos + 2 > data.len() {
                return Err(WebFontError::TooShort);
            }
            let v = i16::from_be_bytes([data[pos], data[pos + 1]]);
            pos += 2;
            v
        };
        y_coords.push(y);
    }

    // Extract on-curve bits.
    let points: Vec<(i32, i32, bool)> = x_coords
        .iter()
        .zip(y_coords.iter())
        .zip(raw_flags.iter())
        .map(|((&x, &y), &flag)| (x as i32, y as i32, flag & 0x01 != 0))
        .collect();

    let _ = n_c; // n_contours is stored in nContourStream separately
    Ok(SimpleGlyph {
        x_min,
        y_min,
        x_max,
        y_max,
        end_pts,
        instructions,
        points,
    })
}

// ---------------------------------------------------------- triplet encoder

/// Encode one (dx, dy, on_curve) triplet into the flagStream and glyphStream.
///
/// Uses the most compact encoding that faithfully represents the delta.
/// Falls back to 4-byte x, 4-byte y (flags 124/126) for any delta that is
/// negative or out of compact-range — this is conservative but always correct.
fn encode_triplet(
    dx: i32,
    dy: i32,
    on_curve: bool,
    flags: &mut Vec<u8>,
    glyph_bytes: &mut Vec<u8>,
) {
    // Try compact cases first (non-negative deltas only).
    if dy == 0 && (0..=9).contains(&dx) {
        // Flags 0-9 (on-curve) or 10-19 (off-curve). Delta = flag % 10 = dx.
        let base: u8 = if on_curve { 0 } else { 10 };
        flags.push(base + dx as u8);
        return;
    }
    if dx == 0 && (0..=9).contains(&dy) {
        // Flags 20-29 (on) or 30-39 (off). Delta = flag % 10 = dy.
        let base: u8 = if on_curve { 20 } else { 30 };
        flags.push(base + dy as u8);
        return;
    }
    if (0..=15).contains(&dx) && (0..=15).contains(&dy) {
        // Flags 40-47 (on) or 48-55 (off). 1 extra byte: high nibble = dx, low nibble = dy.
        let base: u8 = if on_curve { 40 } else { 48 };
        flags.push(base);
        glyph_bytes.push(((dx as u8) << 4) | (dy as u8));
        return;
    }
    // Fallback: 4-byte x + 4-byte y (flags 124 on-curve, 126 off-curve).
    let flag = if on_curve { 124u8 } else { 126u8 };
    flags.push(flag);
    glyph_bytes.extend_from_slice(&dx.to_be_bytes());
    glyph_bytes.extend_from_slice(&dy.to_be_bytes());
}

// --------------------------------------------------------- bbox helpers

/// Set a single bit in a bitmap (MSB-first within each byte).
fn set_bbox_bit(bitmap: &mut [u8], gid: usize) {
    let byte_idx = gid / 8;
    let bit_idx = 7 - (gid % 8);
    if byte_idx < bitmap.len() {
        bitmap[byte_idx] |= 1 << bit_idx;
    }
}

// --------------------------------------------------------- forward transform

/// Result of the glyf/loca forward transform.
pub struct TransformedGlyfBlock {
    /// The transformed glyf block bytes (header + sub-streams).
    pub block: Vec<u8>,
    /// The number of glyphs (for computing transformLength in the WOFF2 directory).
    pub num_glyphs: u16,
    /// The loca index_format (0 = short, 1 = long) from `head`.
    pub index_format: u16,
}

/// Apply the WOFF2 glyf/loca forward transform.
///
/// `glyf_data` and `loca_data` are the raw SFNT table bytes.
/// `index_format` comes from `head.indexToLocFormat`.
/// `num_glyphs` comes from `maxp.numGlyphs`.
pub fn transform_glyf_loca(
    glyf_data: &[u8],
    loca_data: &[u8],
    index_format: u16,
    num_glyphs: u16,
) -> Result<TransformedGlyfBlock, WebFontError> {
    let loca_offsets = parse_loca(loca_data, index_format)?;

    // Sub-streams.
    let mut n_contour_stream: Vec<u8> = Vec::new();
    let mut n_points_stream: Vec<u8> = Vec::new();
    let mut flag_stream: Vec<u8> = Vec::new();
    let mut glyph_stream: Vec<u8> = Vec::new();
    let mut composite_stream: Vec<u8> = Vec::new();
    let mut bbox_stream_bbox: Vec<u8> = Vec::new(); // bbox entries (8 bytes each)

    let num_glyphs_usize = num_glyphs as usize;
    let bitmap_len = num_glyphs_usize.div_ceil(8);
    let mut bbox_bitmap = vec![0u8; bitmap_len];

    let mut instruction_stream: Vec<u8> = Vec::new();

    for gid in 0..num_glyphs_usize {
        let start = loca_offsets
            .get(gid)
            .copied()
            .ok_or(WebFontError::OutOfBounds {
                context: "loca[gid]",
            })? as usize;
        let end = loca_offsets
            .get(gid + 1)
            .copied()
            .ok_or(WebFontError::OutOfBounds {
                context: "loca[gid+1]",
            })? as usize;

        if start == end {
            // Empty glyph.
            n_contour_stream.extend_from_slice(&0i16.to_be_bytes());
            continue;
        }

        let glyph_bytes = glyf_data.get(start..end).ok_or(WebFontError::OutOfBounds {
            context: "glyf slice",
        })?;

        if glyph_bytes.len() < 10 {
            return Err(WebFontError::MalformedGlyfTransform(
                "glyph data too short for header".to_string(),
            ));
        }

        let n_contours = read_i16(glyph_bytes, 0)?;
        n_contour_stream.extend_from_slice(&n_contours.to_be_bytes());

        if n_contours < 0 {
            // Composite glyph.
            let x_min = read_i16(glyph_bytes, 2)?;
            let y_min = read_i16(glyph_bytes, 4)?;
            let x_max = read_i16(glyph_bytes, 6)?;
            let y_max = read_i16(glyph_bytes, 8)?;

            // Always write bbox for composites.
            set_bbox_bit(&mut bbox_bitmap, gid);
            bbox_stream_bbox.extend_from_slice(&x_min.to_be_bytes());
            bbox_stream_bbox.extend_from_slice(&y_min.to_be_bytes());
            bbox_stream_bbox.extend_from_slice(&x_max.to_be_bytes());
            bbox_stream_bbox.extend_from_slice(&y_max.to_be_bytes());

            // Raw composite data: everything after the 10-byte header.
            // This includes all component records and optional instructions.
            composite_stream.extend_from_slice(&glyph_bytes[10..]);
        } else if n_contours > 0 {
            // Simple glyph.
            let n_c = n_contours as usize;
            let glyph = parse_simple_glyph(glyph_bytes, n_c)?;

            let total_points = glyph.points.len();

            // Determine per-contour point counts from endPts.
            let mut contour_counts: Vec<u16> = Vec::with_capacity(n_c);
            let mut prev_end: i32 = -1;
            for &ep in &glyph.end_pts {
                let count = ep as i32 - prev_end;
                contour_counts.push(count as u16);
                prev_end = ep as i32;
            }

            // Write nPoints to n_points_stream.
            for &np in &contour_counts {
                encode_255_u16(&mut n_points_stream, np);
            }

            // Write triplets: flag bytes → flag_stream, extra bytes → glyph_stream.
            for &(dx, dy, on_curve) in &glyph.points {
                encode_triplet(dx, dy, on_curve, &mut flag_stream, &mut glyph_stream);
            }

            // Write instruction length to glyph_stream.
            encode_255_u16(&mut glyph_stream, glyph.instructions.len() as u16);

            // Write instructions to instruction_stream.
            if !glyph.instructions.is_empty() {
                instruction_stream.extend_from_slice(&glyph.instructions);
            }

            // Write bbox if glyph has points (always explicit for simple glyphs).
            if total_points > 0 {
                set_bbox_bit(&mut bbox_bitmap, gid);
                bbox_stream_bbox.extend_from_slice(&glyph.x_min.to_be_bytes());
                bbox_stream_bbox.extend_from_slice(&glyph.y_min.to_be_bytes());
                bbox_stream_bbox.extend_from_slice(&glyph.x_max.to_be_bytes());
                bbox_stream_bbox.extend_from_slice(&glyph.y_max.to_be_bytes());
            }
        } else {
            // n_contours == 0 but glyph data is non-empty — treat as empty.
            // (This is unusual but valid; we already wrote nContours = 0.)
        }
    }

    // Combine bbox_stream = bitmap || bbox entries.
    let mut bbox_stream = bbox_bitmap;
    bbox_stream.extend(bbox_stream_bbox);

    // Build the sub-stream size fields.
    let n_contour_stream_size = n_contour_stream.len() as u32;
    let n_points_stream_size = n_points_stream.len() as u32;
    let flag_stream_size = flag_stream.len() as u32;
    let glyph_stream_size = glyph_stream.len() as u32;
    let composite_stream_size = composite_stream.len() as u32;
    let bbox_stream_size = bbox_stream.len() as u32;
    let instruction_stream_size = instruction_stream.len() as u32;

    // Write the 36-byte header.
    let mut block = Vec::with_capacity(
        GLYF_HEADER_SIZE
            + n_contour_stream.len()
            + n_points_stream.len()
            + flag_stream.len()
            + glyph_stream.len()
            + composite_stream.len()
            + bbox_stream.len()
            + instruction_stream.len(),
    );

    block.extend_from_slice(&0x0003u16.to_be_bytes()); // version
    block.extend_from_slice(&0x0000u16.to_be_bytes()); // option_flags
    block.extend_from_slice(&num_glyphs.to_be_bytes()); // num_glyphs
    block.extend_from_slice(&index_format.to_be_bytes()); // index_format
    block.extend_from_slice(&n_contour_stream_size.to_be_bytes());
    block.extend_from_slice(&n_points_stream_size.to_be_bytes());
    block.extend_from_slice(&flag_stream_size.to_be_bytes());
    block.extend_from_slice(&glyph_stream_size.to_be_bytes());
    block.extend_from_slice(&composite_stream_size.to_be_bytes());
    block.extend_from_slice(&bbox_stream_size.to_be_bytes());
    block.extend_from_slice(&instruction_stream_size.to_be_bytes());
    debug_assert_eq!(block.len(), GLYF_HEADER_SIZE);

    // Append sub-streams.
    block.extend(n_contour_stream);
    block.extend(n_points_stream);
    block.extend(flag_stream);
    block.extend(glyph_stream);
    block.extend(composite_stream);
    block.extend(bbox_stream);
    block.extend(instruction_stream);

    Ok(TransformedGlyfBlock {
        block,
        num_glyphs,
        index_format,
    })
}
