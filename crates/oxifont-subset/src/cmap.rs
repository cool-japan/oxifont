/// `cmap` table rewriter.
use std::collections::BTreeMap;

use crate::tables::SubsetError;

// ---------------------------------------------------------------------------
// cmap format 4 builder
// ---------------------------------------------------------------------------

/// Encode `codepoints_to_new_gid` as a format-4 cmap sub-table.
///
/// Only BMP (≤ 0xFFFF) codepoints are encoded; higher codepoints must go
/// through a format-12 sub-table.
fn build_format4(bmp_map: &BTreeMap<u16, u16>) -> Vec<u8> {
    // Build segments: consecutive codepoints with constant (gid - cp) delta.
    // Terminal sentinel segment: (0xFFFF, 0xFFFF, 1, 0) — required by spec.
    struct Seg {
        start: u16,
        end: u16,
        delta: i32, // signed; applied mod 65536
    }

    let mut segments: Vec<Seg> = Vec::new();

    if !bmp_map.is_empty() {
        let mut iter = bmp_map.iter();
        let (&first_cp, &first_gid) = iter.next().expect("non-empty");
        let mut seg_start = first_cp;
        let mut seg_end = first_cp;
        let mut seg_delta = first_gid as i32 - first_cp as i32;

        for (&cp, &gid) in iter {
            let delta = gid as i32 - cp as i32;
            if cp == seg_end + 1 && delta == seg_delta {
                // Extend current segment.
                seg_end = cp;
            } else {
                segments.push(Seg {
                    start: seg_start,
                    end: seg_end,
                    delta: seg_delta,
                });
                seg_start = cp;
                seg_end = cp;
                seg_delta = delta;
            }
        }
        segments.push(Seg {
            start: seg_start,
            end: seg_end,
            delta: seg_delta,
        });
    }

    // Sentinel.
    segments.push(Seg {
        start: 0xFFFF,
        end: 0xFFFF,
        delta: 1,
    });

    let seg_count = segments.len() as u16;
    // searchRange = 2 * 2^floor(log2(segCount))
    let search_range = 2u16 * seg_count.next_power_of_two() / 2;
    // next_power_of_two on u16 works correctly for segCount=1 → returns 1
    // so searchRange = 2 * 1/2 = 1 which is wrong; handle minimum:
    let search_range = if seg_count == 1 { 2u16 } else { search_range };
    let entry_selector = ((search_range / 2) as f64).log2().floor() as u16;
    let range_shift = seg_count * 2 - search_range;

    // Header (14 bytes) + 4 parallel arrays (each seg_count * 2 bytes) + reservedPad (2).
    // Total: 14 + 2 + seg_count * 8 bytes.
    let length = 14u16 + 2 + seg_count * 8;

    let mut out = Vec::with_capacity(length as usize);
    out.extend_from_slice(&4u16.to_be_bytes()); // format
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // language
    out.extend_from_slice(&(seg_count * 2).to_be_bytes()); // segCountX2
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());
    // endCode array
    for seg in &segments {
        out.extend_from_slice(&seg.end.to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes()); // reservedPad
                                                // startCode array
    for seg in &segments {
        out.extend_from_slice(&seg.start.to_be_bytes());
    }
    // idDelta array (signed i16)
    for seg in &segments {
        let d = (seg.delta as i16).to_be_bytes();
        out.extend_from_slice(&d);
    }
    // idRangeOffset array — all 0 (use delta-only encoding)
    for _ in &segments {
        out.extend_from_slice(&0u16.to_be_bytes());
    }
    // No glyphIdArray entries (idRangeOffset all 0).

    out
}

// ---------------------------------------------------------------------------
// cmap format 12 builder
// ---------------------------------------------------------------------------

/// Encode `codepoints_to_new_gid` as a format-12 cmap sub-table (full Unicode).
fn build_format12(map: &BTreeMap<u32, u16>) -> Vec<u8> {
    // Build sequential map groups (contiguous cp with contiguous gid).
    struct Group {
        start_char: u32,
        end_char: u32,
        start_glyph: u32,
    }

    let mut groups: Vec<Group> = Vec::new();

    if !map.is_empty() {
        let mut iter = map.iter();
        let (&first_cp, &first_gid) = iter.next().expect("non-empty");
        let mut g_start_cp = first_cp;
        let mut g_end_cp = first_cp;
        let mut g_start_gid = first_gid as u32;
        let mut g_end_gid = first_gid as u32;

        for (&cp, &gid) in iter {
            if cp == g_end_cp + 1 && gid as u32 == g_end_gid + 1 {
                g_end_cp = cp;
                g_end_gid = gid as u32;
            } else {
                groups.push(Group {
                    start_char: g_start_cp,
                    end_char: g_end_cp,
                    start_glyph: g_start_gid,
                });
                g_start_cp = cp;
                g_end_cp = cp;
                g_start_gid = gid as u32;
                g_end_gid = gid as u32;
            }
        }
        groups.push(Group {
            start_char: g_start_cp,
            end_char: g_end_cp,
            start_glyph: g_start_gid,
        });
    }

    let num_groups = groups.len() as u32;
    // format(2) + reserved(2) + length(4) + language(4) + numGroups(4) + groups * 12
    let length = 16u32 + num_groups * 12;

    let mut out = Vec::with_capacity(length as usize);
    out.extend_from_slice(&12u16.to_be_bytes()); // format
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // language
    out.extend_from_slice(&num_groups.to_be_bytes());
    for g in &groups {
        out.extend_from_slice(&g.start_char.to_be_bytes());
        out.extend_from_slice(&g.end_char.to_be_bytes());
        out.extend_from_slice(&g.start_glyph.to_be_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a new `cmap` table containing only the given codepoint → new-GID
/// mappings.
///
/// Emits:
/// - Encoding record (Platform 3 / Encoding 1 / Format 4) for BMP codepoints.
/// - Encoding record (Platform 0 / Encoding 3 / Format 4) — same data.
/// - If any codepoints > 0xFFFF: additionally Format 12 for both platforms.
///
/// # Errors
/// Returns [`SubsetError::InvalidFont`] only if something is structurally
/// impossible (currently infallible; kept for API consistency).
pub fn rewrite_cmap(codepoints_to_new_gid: &BTreeMap<u32, u16>) -> Result<Vec<u8>, SubsetError> {
    // Split into BMP and supplementary.
    let bmp_map: BTreeMap<u16, u16> = codepoints_to_new_gid
        .iter()
        .filter(|(&cp, _)| cp <= 0xFFFF)
        .map(|(&cp, &gid)| (cp as u16, gid))
        .collect();

    let has_supplementary = codepoints_to_new_gid.keys().any(|&cp| cp > 0xFFFF);

    let f4_data = build_format4(&bmp_map);

    // Number of encoding records.
    let mut num_records: u16 = 2; // Windows/Unicode BMP + Unicode/BMP
    if has_supplementary {
        num_records += 2; // + Windows Unicode Full + Unicode Full
    }

    // cmap header: version (2) + numTables (2) + records (8 each).
    let header_size = 4usize + num_records as usize * 8;

    // Sub-table offsets (from start of cmap table).
    let f4_offset = header_size;
    let f4_len = f4_data.len();

    let mut sub_tables: Vec<Vec<u8>> = vec![f4_data.clone()];
    let mut f12_offset: usize = 0;

    if has_supplementary {
        // Format 12 placed right after format 4.
        f12_offset = f4_offset + f4_len;
        let f12_data = build_format12(codepoints_to_new_gid);
        sub_tables.push(f12_data);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes()); // version
    out.extend_from_slice(&num_records.to_be_bytes());

    // Record 0: Platform 0 (Unicode), Encoding 3, Format 4.
    out.extend_from_slice(&0u16.to_be_bytes()); // platformID
    out.extend_from_slice(&3u16.to_be_bytes()); // encodingID
    out.extend_from_slice(&(f4_offset as u32).to_be_bytes());

    // Record 1: Platform 3 (Windows), Encoding 1, Format 4.
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(f4_offset as u32).to_be_bytes());

    if has_supplementary {
        // Record 2: Platform 0 (Unicode), Encoding 4, Format 12.
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&(f12_offset as u32).to_be_bytes());

        // Record 3: Platform 3 (Windows), Encoding 10, Format 12.
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&10u16.to_be_bytes());
        out.extend_from_slice(&(f12_offset as u32).to_be_bytes());
    }

    // Append sub-tables.
    for st in sub_tables {
        out.extend_from_slice(&st);
    }

    Ok(out)
}
