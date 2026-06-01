/// `glyf` + `loca` table rewriter.
use std::collections::HashMap;

use crate::tables::SubsetError;

// ---------------------------------------------------------------------------
// Composite glyph flag bits
// ---------------------------------------------------------------------------

const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

// ---------------------------------------------------------------------------
// loca helpers
// ---------------------------------------------------------------------------

/// Read a `loca` entry at index `gid`.  Returns `(start, end)` in bytes inside
/// `glyf`.
fn loca_entry(loca: &[u8], format: i16, gid: u16) -> Option<(usize, usize)> {
    let idx = gid as usize;
    if format == 0 {
        // Short format: u16, multiply by 2.
        let start_bytes = loca.get(idx * 2..idx * 2 + 2)?;
        let end_bytes = loca.get((idx + 1) * 2..(idx + 1) * 2 + 2)?;
        let start = (u16::from_be_bytes([start_bytes[0], start_bytes[1]]) as usize) * 2;
        let end = (u16::from_be_bytes([end_bytes[0], end_bytes[1]]) as usize) * 2;
        Some((start, end))
    } else {
        // Long format: u32.
        let start_bytes = loca.get(idx * 4..idx * 4 + 4)?;
        let end_bytes = loca.get((idx + 1) * 4..(idx + 1) * 4 + 4)?;
        let start = u32::from_be_bytes([
            start_bytes[0],
            start_bytes[1],
            start_bytes[2],
            start_bytes[3],
        ]) as usize;
        let end =
            u32::from_be_bytes([end_bytes[0], end_bytes[1], end_bytes[2], end_bytes[3]]) as usize;
        Some((start, end))
    }
}

// ---------------------------------------------------------------------------
// Composite component closure
// ---------------------------------------------------------------------------

/// Walk a composite glyph and add all referenced component GIDs to `out`.
///
/// Returns an error if the glyph data is truncated.
pub fn collect_composite_components(
    glyf_data: &[u8],
    glyph_start: usize,
    glyph_end: usize,
    out: &mut Vec<u16>,
) -> Result<(), SubsetError> {
    let glyph = glyf_data
        .get(glyph_start..glyph_end)
        .ok_or_else(|| SubsetError::InvalidFont("glyph data out of bounds".into()))?;

    if glyph.len() < 2 {
        return Ok(());
    }

    let num_contours = i16::from_be_bytes([glyph[0], glyph[1]]);
    if num_contours >= 0 {
        // Simple glyph — no components.
        return Ok(());
    }

    // Composite glyph: iterate components.
    let mut pos = 10; // skip header (2 + 2 + 2 + 2 + 2)
    loop {
        if pos + 4 > glyph.len() {
            return Err(SubsetError::InvalidFont(
                "composite glyph data truncated at component flags".into(),
            ));
        }
        let flags = u16::from_be_bytes([glyph[pos], glyph[pos + 1]]);
        let component_gid = u16::from_be_bytes([glyph[pos + 2], glyph[pos + 3]]);
        out.push(component_gid);
        pos += 4; // flags + glyphIndex

        // Skip arguments.
        if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            pos += 4;
        } else {
            pos += 2;
        }

        // Skip transformation.
        if flags & WE_HAVE_A_SCALE != 0 {
            pos += 2; // one F2Dot14
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            pos += 4; // two F2Dot14
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            pos += 8; // four F2Dot14
        }

        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Composite glyph GID remapping
// ---------------------------------------------------------------------------

/// Rewrite a composite glyph's component GIDs using `gid_remap`.
///
/// Returns the rewritten glyph bytes.
fn rewrite_composite(glyph: &[u8], gid_remap: &HashMap<u16, u16>) -> Result<Vec<u8>, SubsetError> {
    let mut out = glyph.to_vec();

    let mut pos = 10; // skip header
    loop {
        if pos + 4 > out.len() {
            return Err(SubsetError::InvalidFont(
                "composite glyph truncated during rewrite".into(),
            ));
        }
        let flags = u16::from_be_bytes([out[pos], out[pos + 1]]);
        let old_gid = u16::from_be_bytes([out[pos + 2], out[pos + 3]]);

        let new_gid = gid_remap.get(&old_gid).copied().unwrap_or(0);
        out[pos + 2] = (new_gid >> 8) as u8;
        out[pos + 3] = (new_gid & 0xFF) as u8;

        pos += 4;

        // Skip arguments.
        if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            pos += 4;
        } else {
            pos += 2;
        }

        // Skip transformation.
        if flags & WE_HAVE_A_SCALE != 0 {
            pos += 2;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            pos += 4;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            pos += 8;
        }

        if flags & MORE_COMPONENTS == 0 {
            // Skip instructions if present.
            if flags & WE_HAVE_INSTRUCTIONS != 0 {
                if pos + 2 > out.len() {
                    return Err(SubsetError::InvalidFont(
                        "composite glyph truncated at instruction length".into(),
                    ));
                }
                let instr_len = u16::from_be_bytes([out[pos], out[pos + 1]]) as usize;
                pos += 2 + instr_len;
            }
            break;
        }
    }

    let _ = pos; // pos not needed after loop
    Ok(out)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rewrite `glyf` and `loca` tables for a subset font.
///
/// For each new GID (0 .. `new_glyph_count`):
/// - Find the corresponding old GID via the reverse of `gid_remap`.
/// - Copy its glyph data (4-byte padded) into the new `glyf`.
/// - Record the loca offsets.
///
/// Returns `(new_glyf, new_loca, new_loca_format)`.
pub fn rewrite_glyf_loca(
    glyf_data: &[u8],
    old_loca: &[u8],
    loca_format: i16,
    gid_remap: &HashMap<u16, u16>,
    new_glyph_count: u16,
) -> Result<(Vec<u8>, Vec<u8>, i16), SubsetError> {
    // Build reverse map: new GID → old GID.
    let mut rev_remap: HashMap<u16, u16> = HashMap::with_capacity(gid_remap.len());
    for (&old, &new) in gid_remap {
        rev_remap.insert(new, old);
    }

    let mut new_glyf: Vec<u8> = Vec::new();
    // loca needs new_glyph_count + 1 entries.
    let mut loca_offsets: Vec<u32> = Vec::with_capacity(new_glyph_count as usize + 1);

    for new_gid in 0..new_glyph_count {
        loca_offsets.push(new_glyf.len() as u32);

        let old_gid = match rev_remap.get(&new_gid) {
            Some(&g) => g,
            None => {
                // No mapping — empty glyph.
                continue;
            }
        };

        let (start, end) = match loca_entry(old_loca, loca_format, old_gid) {
            Some(se) => se,
            None => continue,
        };

        if start >= end {
            // Empty glyph (loca[i] == loca[i+1]).
            continue;
        }

        if end > glyf_data.len() {
            return Err(SubsetError::InvalidFont(format!(
                "glyf entry for old GID {old_gid} at {start}..{end} exceeds glyf length {}",
                glyf_data.len()
            )));
        }

        let glyph_bytes = &glyf_data[start..end];

        if glyph_bytes.len() >= 2 {
            let num_contours = i16::from_be_bytes([glyph_bytes[0], glyph_bytes[1]]);
            if num_contours < 0 {
                // Composite — rewrite component GIDs.
                let rewritten = rewrite_composite(glyph_bytes, gid_remap)?;
                new_glyf.extend_from_slice(&rewritten);
            } else {
                new_glyf.extend_from_slice(glyph_bytes);
            }
        } else {
            new_glyf.extend_from_slice(glyph_bytes);
        }

        // Pad to 4-byte boundary.
        while !new_glyf.len().is_multiple_of(4) {
            new_glyf.push(0);
        }
    }

    // Final loca entry (end of last glyph).
    loca_offsets.push(new_glyf.len() as u32);

    // Choose loca format: format 0 (u16 halved) if fits in 131070 bytes.
    let new_loca_format = if new_glyf.len() <= 0x1FFFE {
        0i16
    } else {
        1i16
    };

    let new_loca = if new_loca_format == 0 {
        // Short: u16, divide by 2.
        let mut loca_bytes = Vec::with_capacity(loca_offsets.len() * 2);
        for &off in &loca_offsets {
            let halved = (off / 2) as u16;
            loca_bytes.extend_from_slice(&halved.to_be_bytes());
        }
        loca_bytes
    } else {
        // Long: u32.
        let mut loca_bytes = Vec::with_capacity(loca_offsets.len() * 4);
        for &off in &loca_offsets {
            loca_bytes.extend_from_slice(&off.to_be_bytes());
        }
        loca_bytes
    };

    Ok((new_glyf, new_loca, new_loca_format))
}
