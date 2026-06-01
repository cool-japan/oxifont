//! Tests for `subset_with_table_map` — the pre-parsed SFNT table map entry point.

use oxifont_core::sfnt::SfntTableMap;
use std::collections::{BTreeMap, BTreeSet};

static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

/// Verify that `SfntTableMap::parse` succeeds on the real TTF fixture and
/// the essential tables are present.
#[test]
fn sfnt_table_map_parses_fixture() {
    let map = SfntTableMap::parse(TTF).expect("SfntTableMap must parse the TTF fixture");
    assert!(map.table(b"cmap").is_some(), "fixture TTF must have cmap");
    assert!(map.table(b"glyf").is_some(), "fixture TTF must have glyf");
    assert!(map.table(b"head").is_some(), "fixture TTF must have head");
}

/// Verify that `subset_with_table_map` produces the same bytes as
/// `subset_font_with_options` when given the same glyph set derived from
/// a known codepoint set.
#[test]
fn subset_with_table_map_matches_standard_path() {
    let codepoints: BTreeSet<char> = ('A'..='Z').chain('a'..='z').collect();

    // Standard path: subset_font_with_options with default options.
    let opts = oxifont_subset::SubsetOptions::default();
    let (standard_bytes, standard_stats) =
        oxifont_subset::subset_font_with_options(TTF, &codepoints, &opts)
            .expect("standard subset must succeed");
    assert!(
        !standard_bytes.is_empty(),
        "standard subset must produce non-empty output"
    );

    // Derive the GID set and codepoint→GID map from the standard stats so we
    // can feed the same selection to `subset_with_table_map`.
    // Build the GID set from the cmap table via SfntTableMap.
    let map = SfntTableMap::parse(TTF).expect("SfntTableMap must parse");

    // Read the cmap from the map to resolve codepoints → GIDs.
    let cmap_data = map.table(b"cmap").expect("fixture TTF must have cmap");
    let (gid_set, cp_to_old_gid) = resolve_codepoints(codepoints, cmap_data);

    // SfntTableMap path.
    let (map_bytes, map_stats) =
        oxifont_subset::subset_with_table_map(&map, &gid_set, &cp_to_old_gid, &opts)
            .expect("subset_with_table_map must succeed");
    assert!(
        !map_bytes.is_empty(),
        "subset_with_table_map must produce non-empty output"
    );

    assert_eq!(
        map_stats.glyphs_retained, standard_stats.glyphs_retained,
        "both paths must retain the same number of glyphs"
    );
    assert_eq!(
        map_bytes, standard_bytes,
        "subset_with_table_map must produce identical output to the standard path"
    );
}

/// Verify that `subset_with_table_map` always includes `.notdef` (GID 0)
/// even when it is not explicitly in the input `gid_set`.
#[test]
fn subset_with_table_map_includes_notdef() {
    let map = SfntTableMap::parse(TTF).expect("SfntTableMap must parse");
    // Empty GID set — only .notdef should appear.
    let gid_set: BTreeSet<u16> = BTreeSet::new();
    let cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();
    let opts = oxifont_subset::SubsetOptions::default();

    let (bytes, stats) =
        oxifont_subset::subset_with_table_map(&map, &gid_set, &cp_to_old_gid, &opts)
            .expect("subset_with_table_map must succeed for empty GID set");
    assert!(!bytes.is_empty(), "must produce non-empty SFNT output");
    assert_eq!(stats.glyphs_retained, 1, "only .notdef must be retained");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a cmap table and resolve a set of codepoints into a GID set and a
/// codepoint→GID map. Supports format 4 and format 12.
fn resolve_codepoints(
    codepoints: BTreeSet<char>,
    cmap_data: &[u8],
) -> (BTreeSet<u16>, BTreeMap<u32, u16>) {
    let mut gid_set = BTreeSet::new();
    gid_set.insert(0); // always include .notdef

    let mut cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();

    if cmap_data.len() < 4 {
        return (gid_set, cp_to_old_gid);
    }
    let num_subtables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    if cmap_data.len() < 4 + num_subtables * 8 {
        return (gid_set, cp_to_old_gid);
    }

    let mut full_map: BTreeMap<u32, u16> = BTreeMap::new();

    for i in 0..num_subtables {
        let base = 4 + i * 8;
        let platform_id = u16::from_be_bytes([cmap_data[base], cmap_data[base + 1]]);
        let encoding_id = u16::from_be_bytes([cmap_data[base + 2], cmap_data[base + 3]]);
        let offset = u32::from_be_bytes([
            cmap_data[base + 4],
            cmap_data[base + 5],
            cmap_data[base + 6],
            cmap_data[base + 7],
        ]) as usize;

        if offset + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[offset], cmap_data[offset + 1]]);

        match (platform_id, encoding_id, format) {
            (0, 3, 4) | (3, 1, 4) => {
                if let Some(m) = parse_format4(&cmap_data[offset..]) {
                    for (cp, gid) in m {
                        full_map.entry(cp as u32).or_insert(gid);
                    }
                }
            }
            (0, 4, 12) | (3, 10, 12) => {
                if let Some(m) = parse_format12(&cmap_data[offset..]) {
                    for (cp, gid) in m {
                        full_map.insert(cp, gid);
                    }
                }
            }
            _ => {}
        }
    }

    for cp in codepoints {
        let cp_u32 = cp as u32;
        if let Some(&gid) = full_map.get(&cp_u32) {
            if gid != 0 {
                gid_set.insert(gid);
                cp_to_old_gid.insert(cp_u32, gid);
            }
        }
    }

    (gid_set, cp_to_old_gid)
}

fn parse_format4(data: &[u8]) -> Option<Vec<(u16, u16)>> {
    if data.len() < 14 {
        return None;
    }
    let seg_count = u16::from_be_bytes([data[6], data[7]]) as usize / 2;
    if seg_count == 0 {
        return Some(vec![]);
    }
    let end_base = 14usize;
    let start_base = end_base + seg_count * 2 + 2;
    let delta_base = start_base + seg_count * 2;
    let range_base = delta_base + seg_count * 2;
    if data.len() < range_base + seg_count * 2 {
        return None;
    }
    let mut pairs = Vec::new();
    for i in 0..seg_count {
        let end = u16::from_be_bytes([data[end_base + i * 2], data[end_base + i * 2 + 1]]);
        if end == 0xFFFF {
            break;
        }
        let start = u16::from_be_bytes([data[start_base + i * 2], data[start_base + i * 2 + 1]]);
        let delta =
            i16::from_be_bytes([data[delta_base + i * 2], data[delta_base + i * 2 + 1]]) as i32;
        let range_offset =
            u16::from_be_bytes([data[range_base + i * 2], data[range_base + i * 2 + 1]]) as usize;
        for cp in start..=end {
            let gid = if range_offset == 0 {
                ((cp as i32 + delta) & 0xFFFF) as u16
            } else {
                let ptr = range_base + i * 2 + range_offset + (cp - start) as usize * 2;
                if ptr + 2 > data.len() {
                    0
                } else {
                    let raw = u16::from_be_bytes([data[ptr], data[ptr + 1]]);
                    if raw == 0 {
                        0
                    } else {
                        ((raw as i32 + delta) & 0xFFFF) as u16
                    }
                }
            };
            if gid != 0 {
                pairs.push((cp, gid));
            }
        }
    }
    Some(pairs)
}

fn parse_format12(data: &[u8]) -> Option<BTreeMap<u32, u16>> {
    if data.len() < 16 {
        return None;
    }
    let num_groups = u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize;
    if data.len() < 16 + num_groups * 12 {
        return None;
    }
    let mut map = BTreeMap::new();
    for i in 0..num_groups {
        let base = 16 + i * 12;
        let start_cp =
            u32::from_be_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
        let end_cp = u32::from_be_bytes([
            data[base + 4],
            data[base + 5],
            data[base + 6],
            data[base + 7],
        ]);
        let start_gid = u32::from_be_bytes([
            data[base + 8],
            data[base + 9],
            data[base + 10],
            data[base + 11],
        ]);
        for offset in 0..=(end_cp.saturating_sub(start_cp)) {
            let cp = start_cp + offset;
            let gid = (start_gid + offset) as u16;
            if gid != 0 {
                map.insert(cp, gid);
            }
        }
    }
    Some(map)
}
