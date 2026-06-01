//! Tests for COLR, SVG, and sbix table subsetting.

use oxifont_subset::{colr, sbix, svg};
use std::collections::HashMap;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Read a big-endian u16 from `data` at `offset`.  Panics on out-of-bounds (test helper).
fn get_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

/// Read a big-endian u32 from `data` at `offset`.  Panics on out-of-bounds (test helper).
fn get_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

// ─── COLR builder helpers ─────────────────────────────────────────────────────

/// Build a minimal COLR v0 table from a list of (base_gid, layers) where
/// layers is [(layer_gid, palette_index)].
fn build_colr_v0(bases: &[(u16, Vec<(u16, u16)>)]) -> Vec<u8> {
    // Flatten layers.
    let mut all_layers: Vec<(u16, u16)> = Vec::new();
    let mut base_records: Vec<(u16, u16, u16)> = Vec::new(); // (gid, first, count)
    for (gid, layers) in bases {
        let first = all_layers.len() as u16;
        let count = layers.len() as u16;
        base_records.push((*gid, first, count));
        all_layers.extend_from_slice(layers);
    }

    let num_base = base_records.len() as u16;
    let num_layers = all_layers.len() as u16;

    // Header: 14 bytes.
    // BaseGlyphRecords start at offset 14.
    // LayerRecords start at offset 14 + num_base * 6.
    let base_offset: u32 = 14;
    let layer_offset: u32 = base_offset + num_base as u32 * 6;

    let total = layer_offset as usize + num_layers as usize * 4;
    let mut out = vec![0u8; total];

    // version = 0
    out[0] = 0;
    out[1] = 0;
    out[2] = (num_base >> 8) as u8;
    out[3] = (num_base & 0xFF) as u8;
    out[4..8].copy_from_slice(&base_offset.to_be_bytes());
    out[8..12].copy_from_slice(&layer_offset.to_be_bytes());
    out[12] = (num_layers >> 8) as u8;
    out[13] = (num_layers & 0xFF) as u8;

    for (i, (gid, first, count)) in base_records.iter().enumerate() {
        let off = 14 + i * 6;
        out[off..off + 2].copy_from_slice(&gid.to_be_bytes());
        out[off + 2..off + 4].copy_from_slice(&first.to_be_bytes());
        out[off + 4..off + 6].copy_from_slice(&count.to_be_bytes());
    }

    for (i, (gid, pal)) in all_layers.iter().enumerate() {
        let off = layer_offset as usize + i * 4;
        out[off..off + 2].copy_from_slice(&gid.to_be_bytes());
        out[off + 2..off + 4].copy_from_slice(&pal.to_be_bytes());
    }

    out
}

// ─── COLR tests ───────────────────────────────────────────────────────────────

#[test]
fn test_colr_v0_remap_base_glyph() {
    // Font has 3 base glyphs (GIDs 1, 2, 3) with 5 total layers:
    //   GID 1 → layers [(10, 0), (11, 1)]
    //   GID 2 → layers [(12, 0)]
    //   GID 3 → layers [(13, 2), (14, 0)]
    //
    // We keep GIDs {0, 1, 3, 10, 11, 13, 14} and remove GID 2 and its layer GID 12.
    // GID 1 → new GID 1, GID 3 → new GID 2, GID 10 → new GID 3, GID 11 → new GID 4,
    // GID 13 → new GID 5, GID 14 → new GID 6.

    let table = build_colr_v0(&[
        (1, vec![(10, 0), (11, 1)]),
        (2, vec![(12, 0)]),
        (3, vec![(13, 2), (14, 0)]),
    ]);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1);
    // GID 2 is NOT in remap → dropped
    remap.insert(3, 2);
    remap.insert(10, 3);
    remap.insert(11, 4);
    remap.insert(13, 5);
    remap.insert(14, 6);

    let out = colr::rewrite_colr(&table, &remap);

    // version = 0
    assert_eq!(get_u16(&out, 0), 0, "version");

    // Should have 2 surviving base glyphs (GID 1 and GID 3).
    let num_base = get_u16(&out, 2) as usize;
    assert_eq!(num_base, 2, "numBaseGlyphRecords");

    let base_off = get_u32(&out, 4) as usize;
    let layer_off = get_u32(&out, 8) as usize;
    let num_layers = get_u16(&out, 12) as usize;

    // 2 base glyphs × 6 bytes each = 12 bytes for base records.
    assert_eq!(base_off, 14, "baseGlyphRecordsOffset");
    assert_eq!(layer_off, 14 + num_base * 6, "layerRecordsOffset");
    // GID 1 keeps 2 layers, GID 3 keeps 2 layers → 4 total.
    assert_eq!(num_layers, 4, "numLayerRecords");

    // First base record: new GID 1, firstLayerIndex 0, numLayers 2.
    let b0_gid = get_u16(&out, base_off);
    let b0_first = get_u16(&out, base_off + 2);
    let b0_count = get_u16(&out, base_off + 4);
    assert_eq!(b0_gid, 1, "base[0].gID");
    assert_eq!(b0_first, 0, "base[0].firstLayerIndex");
    assert_eq!(b0_count, 2, "base[0].numLayers");

    // Second base record: new GID 2, firstLayerIndex 2, numLayers 2.
    let b1_gid = get_u16(&out, base_off + 6);
    let b1_first = get_u16(&out, base_off + 8);
    let b1_count = get_u16(&out, base_off + 10);
    assert_eq!(b1_gid, 2, "base[1].gID");
    assert_eq!(b1_first, 2, "base[1].firstLayerIndex");
    assert_eq!(b1_count, 2, "base[1].numLayers");

    // Layer 0: new GID 3 (was 10), palette 0.
    let l0_gid = get_u16(&out, layer_off);
    let l0_pal = get_u16(&out, layer_off + 2);
    assert_eq!(l0_gid, 3, "layer[0].gID");
    assert_eq!(l0_pal, 0, "layer[0].paletteIndex");

    // Layer 1: new GID 4 (was 11), palette 1.
    let l1_gid = get_u16(&out, layer_off + 4);
    let l1_pal = get_u16(&out, layer_off + 6);
    assert_eq!(l1_gid, 4, "layer[1].gID");
    assert_eq!(l1_pal, 1, "layer[1].paletteIndex");

    // Layer 2: new GID 5 (was 13), palette 2.
    let l2_gid = get_u16(&out, layer_off + 8);
    let l2_pal = get_u16(&out, layer_off + 10);
    assert_eq!(l2_gid, 5, "layer[2].gID");
    assert_eq!(l2_pal, 2, "layer[2].paletteIndex");

    // Layer 3: new GID 6 (was 14), palette 0.
    let l3_gid = get_u16(&out, layer_off + 12);
    let l3_pal = get_u16(&out, layer_off + 14);
    assert_eq!(l3_gid, 6, "layer[3].gID");
    assert_eq!(l3_pal, 0, "layer[3].paletteIndex");
}

#[test]
fn test_colr_drop_layer_with_removed_gid() {
    // Base glyph GID 1 references 2 layers: GID 10 (kept) and GID 11 (removed).
    // After subsetting, only 1 layer survives → base glyph survives.
    //
    // Base glyph GID 2 references 2 layers: GID 12 (removed) and GID 13 (removed).
    // Both layers removed → base glyph dropped entirely.

    let table = build_colr_v0(&[(1, vec![(10, 0), (11, 1)]), (2, vec![(12, 0), (13, 1)])]);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1);
    // GID 2 is also kept (base glyph survives) but its layers are removed.
    remap.insert(2, 2);
    remap.insert(10, 3);
    // 11, 12, 13 are removed.

    let out = colr::rewrite_colr(&table, &remap);

    // Only base GID 1 should survive (1 layer referencing new GID 3).
    // Base GID 2 had 0 surviving layers → dropped.
    let num_base = get_u16(&out, 2) as usize;
    assert_eq!(num_base, 1, "numBaseGlyphRecords after layer pruning");

    let base_off = get_u32(&out, 4) as usize;
    let layer_off = get_u32(&out, 8) as usize;
    let num_layers = get_u16(&out, 12) as usize;

    assert_eq!(num_layers, 1, "numLayerRecords");

    let b0_gid = get_u16(&out, base_off);
    let b0_count = get_u16(&out, base_off + 4);
    assert_eq!(b0_gid, 1, "surviving base glyph new GID");
    assert_eq!(b0_count, 1, "surviving layer count");

    let l0_gid = get_u16(&out, layer_off);
    assert_eq!(l0_gid, 3, "surviving layer new GID");
}

#[test]
fn test_colr_v1_passthrough() {
    // A COLR table with version = 1 must be returned verbatim.
    let mut table = build_colr_v0(&[(1, vec![(2, 0)])]);
    // Set version to 1.
    table[0] = 0;
    table[1] = 1;

    let remap: HashMap<u16, u16> = [(0, 0)].into_iter().collect();
    let out = colr::rewrite_colr(&table, &remap);

    assert_eq!(out, table, "COLR v1 must be returned verbatim");
}

#[test]
fn test_colr_empty_input() {
    // Zero-byte input must not panic and returns an empty (or verbatim) output.
    let remap: HashMap<u16, u16> = HashMap::new();
    let out = colr::rewrite_colr(&[], &remap);
    // We accept either empty or verbatim (both are zero-length in this case).
    assert!(out.is_empty(), "empty input → empty output");
}

// ─── SVG table builder helpers ────────────────────────────────────────────────

/// Build a minimal SVG table with the given index entries and corresponding blobs.
/// `entries`: [(start_gid, end_gid, svg_blob)].
fn build_svg_table(entries: &[(u16, u16, &[u8])]) -> Vec<u8> {
    let num = entries.len();
    // SVG header: 10 bytes (version u16, offsetToSVGDocumentList u32, reserved u32).
    // SVGDocumentList starts at offset 10.
    let list_start: u32 = 10;
    // In the list: numEntries(2) + entries×12 + blobs.
    let entries_section_size = 2 + num * 12;
    // Blob offsets are relative to the list start.
    let mut blob_offsets: Vec<u32> = Vec::with_capacity(num);
    let mut cursor = entries_section_size as u32;
    for (_, _, blob) in entries {
        blob_offsets.push(cursor);
        cursor += blob.len() as u32;
    }
    let total_list_size = cursor as usize;
    let total = 10 + total_list_size;

    let mut out = vec![0u8; total];
    // version = 0
    out[0..2].copy_from_slice(&0u16.to_be_bytes());
    // offsetToSVGDocumentList = 10
    out[2..6].copy_from_slice(&list_start.to_be_bytes());
    // reserved = 0
    out[6..10].copy_from_slice(&0u32.to_be_bytes());

    // numEntries
    let list_base = 10;
    out[list_base..list_base + 2].copy_from_slice(&(num as u16).to_be_bytes());

    for (i, ((start, end, blob), &blob_off)) in entries.iter().zip(blob_offsets.iter()).enumerate()
    {
        let eoff = list_base + 2 + i * 12;
        out[eoff..eoff + 2].copy_from_slice(&start.to_be_bytes());
        out[eoff + 2..eoff + 4].copy_from_slice(&end.to_be_bytes());
        out[eoff + 4..eoff + 8].copy_from_slice(&blob_off.to_be_bytes());
        out[eoff + 8..eoff + 12].copy_from_slice(&(blob.len() as u32).to_be_bytes());
    }

    // Blobs
    let blobs_start = list_base + entries_section_size;
    let mut bpos = blobs_start;
    for (_, _, blob) in entries {
        out[bpos..bpos + blob.len()].copy_from_slice(blob);
        bpos += blob.len();
    }

    out
}

// ─── SVG tests ────────────────────────────────────────────────────────────────

#[test]
fn test_svg_remap_entry() {
    // Two SVG document entries:
    //   Entry 0: startGlyphID=1, endGlyphID=1, blob="<svg>A</svg>"
    //   Entry 1: startGlyphID=3, endGlyphID=4, blob="<svg>B</svg>"
    //
    // Remap: keep GID 0→0, 1→1, 3→2, 4→3 (GID 2 removed).
    // Entry 0 (1→1): both start and end survive → keep.
    // Entry 1 (3→4, remapped to 2→3): both start and end survive → keep.

    let blob_a = b"<svg>A</svg>";
    let blob_b = b"<svg>B</svg>";
    let table = build_svg_table(&[(1, 1, blob_a), (3, 4, blob_b)]);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(0, 0);
    remap.insert(1, 1);
    // GID 2 removed
    remap.insert(3, 2);
    remap.insert(4, 3);

    let out = svg::rewrite_svg(&table, &remap);

    // version = 0
    assert_eq!(get_u16(&out, 0), 0, "version");

    let list_off = get_u32(&out, 2) as usize;
    let num_entries = get_u16(&out, list_off) as usize;
    assert_eq!(num_entries, 2, "numEntries");

    let e0_start = get_u16(&out, list_off + 2);
    let e0_end = get_u16(&out, list_off + 4);
    assert_eq!(e0_start, 1, "entry[0].startGlyphID");
    assert_eq!(e0_end, 1, "entry[0].endGlyphID");

    let e1_start = get_u16(&out, list_off + 14);
    let e1_end = get_u16(&out, list_off + 16);
    assert_eq!(e1_start, 2, "entry[1].startGlyphID remapped");
    assert_eq!(e1_end, 3, "entry[1].endGlyphID remapped");

    // Verify blob_a content is accessible via e0 offset+length.
    let e0_blob_off = get_u32(&out, list_off + 6) as usize;
    let e0_blob_len = get_u32(&out, list_off + 10) as usize;
    let e0_blob = &out[list_off + e0_blob_off..list_off + e0_blob_off + e0_blob_len];
    assert_eq!(e0_blob, blob_a, "entry[0] blob preserved");

    // Verify blob_b content is accessible via e1 offset+length.
    let e1_blob_off = get_u32(&out, list_off + 18) as usize;
    let e1_blob_len = get_u32(&out, list_off + 22) as usize;
    let e1_blob = &out[list_off + e1_blob_off..list_off + e1_blob_off + e1_blob_len];
    assert_eq!(e1_blob, blob_b, "entry[1] blob preserved");
}

#[test]
fn test_svg_all_removed() {
    // All GID ranges removed → 0 entries, valid empty SVGDocumentList.
    let blob = b"<svg/>";
    let table = build_svg_table(&[(5, 6, blob)]);

    // Only keep GID 0 (notdef); GIDs 5 and 6 are removed.
    let remap: HashMap<u16, u16> = [(0, 0)].into_iter().collect();

    let out = svg::rewrite_svg(&table, &remap);

    // Should still be a valid SVG table with 0 entries.
    assert_eq!(get_u16(&out, 0), 0, "version");
    let list_off = get_u32(&out, 2) as usize;
    let num_entries = get_u16(&out, list_off) as usize;
    assert_eq!(num_entries, 0, "numEntries should be 0");
}

// ─── sbix table builder helpers ───────────────────────────────────────────────

/// Build a minimal sbix table with one strike and glyph data blobs.
///
/// `glyph_blobs`: indexed by old GID; `None` means empty (no bitmap data).
/// The strike ppem=72, ppi=72.
fn build_sbix_table(glyph_blobs: &[Option<&[u8]>]) -> Vec<u8> {
    let old_gc = glyph_blobs.len();
    // Strike layout: ppem(u16) + ppi(u16) + offsets[(old_gc+1)×u32] + blobs.
    let strike_header = 4usize;
    let strike_offsets_size = (old_gc + 1) * 4;
    let blobs_start_in_strike = strike_header + strike_offsets_size;

    let mut blob_data: Vec<u8> = Vec::new();
    let mut per_glyph_offsets: Vec<u32> = Vec::with_capacity(old_gc + 1);
    for blob_opt in glyph_blobs {
        per_glyph_offsets.push((blobs_start_in_strike + blob_data.len()) as u32);
        if let Some(blob) = blob_opt {
            blob_data.extend_from_slice(blob);
        }
    }
    per_glyph_offsets.push((blobs_start_in_strike + blob_data.len()) as u32);

    let strike_size = blobs_start_in_strike + blob_data.len();
    // Table layout: header(8) + strike_offsets(1×4) + strike_blob(strike_size)
    let strike_abs_off: u32 = 8 + 4; // after table header and strike offset array.
    let total = strike_abs_off as usize + strike_size;

    let mut out = vec![0u8; total];
    // Header: version=1, flags=0x0001 (has bitmap), numStrikes=1
    out[0..2].copy_from_slice(&1u16.to_be_bytes()); // version
    out[2..4].copy_from_slice(&1u16.to_be_bytes()); // flags (has glyphs)
    out[4..8].copy_from_slice(&1u32.to_be_bytes()); // numStrikes

    // Strike offset array: one entry.
    out[8..12].copy_from_slice(&strike_abs_off.to_be_bytes());

    // Strike: ppem=72, ppi=72.
    let sb = strike_abs_off as usize;
    out[sb..sb + 2].copy_from_slice(&72u16.to_be_bytes());
    out[sb + 2..sb + 4].copy_from_slice(&72u16.to_be_bytes());

    for (i, &off) in per_glyph_offsets.iter().enumerate() {
        let o = sb + strike_header + i * 4;
        out[o..o + 4].copy_from_slice(&off.to_be_bytes());
    }

    let blob_start = sb + blobs_start_in_strike;
    out[blob_start..blob_start + blob_data.len()].copy_from_slice(&blob_data);

    out
}

// ─── sbix tests ───────────────────────────────────────────────────────────────

#[test]
fn test_sbix_remap_glyph_data() {
    // 4-glyph font (GIDs 0-3) with 1 strike.
    // GID 0: empty (notdef).
    // GID 1: blob_a = "AAAA".
    // GID 2: blob_b = "BBBB" — this GID is removed.
    // GID 3: blob_c = "CCCC".
    //
    // After removal of GID 2:
    //   new GID 0 = old 0 (empty)
    //   new GID 1 = old 1 (blob_a)
    //   new GID 2 = old 3 (blob_c)
    //
    // new_glyph_count = 3.

    let blob_a = b"AAAA";
    let blob_b = b"BBBB";
    let blob_c = b"CCCC";

    let table = build_sbix_table(&[None, Some(blob_a), Some(blob_b), Some(blob_c)]);

    // rev_remap: new→old.
    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0); // new 0 → old 0
    rev.insert(1, 1); // new 1 → old 1
    rev.insert(2, 3); // new 2 → old 3 (GID 2 was removed)

    let out = sbix::rewrite_sbix(&table, &rev, 4, 3);

    // Parse output header.
    let _version = get_u16(&out, 0);
    let _flags = get_u16(&out, 2);
    let num_strikes = get_u32(&out, 4);
    assert_eq!(num_strikes, 1, "numStrikes preserved");

    let strike_off = get_u32(&out, 8) as usize;

    let ppem = get_u16(&out, strike_off);
    let ppi = get_u16(&out, strike_off + 2);
    assert_eq!(ppem, 72, "ppem preserved");
    assert_eq!(ppi, 72, "ppi preserved");

    // Per-glyph offsets: 4 entries (new_gc=3, so 4 offsets).
    let off_base = strike_off + 4;
    let g0_start = get_u32(&out, off_base) as usize;
    let g0_end = get_u32(&out, off_base + 4) as usize;
    let g1_start = get_u32(&out, off_base + 4) as usize;
    let g1_end = get_u32(&out, off_base + 8) as usize;
    let g2_start = get_u32(&out, off_base + 8) as usize;
    let g2_end = get_u32(&out, off_base + 12) as usize;

    // GID 0: empty block.
    assert_eq!(g0_start, g0_end, "new GID 0 is empty");

    // GID 1: blob_a.
    let g1_data = &out[strike_off + g1_start..strike_off + g1_end];
    assert_eq!(g1_data, blob_a, "new GID 1 has blob_a");

    // GID 2: blob_c (remapped from old GID 3).
    let g2_data = &out[strike_off + g2_start..strike_off + g2_end];
    assert_eq!(g2_data, blob_c, "new GID 2 has blob_c (old GID 3)");
}

#[test]
fn test_sbix_empty_glyphs() {
    // Font with 2 glyphs both having empty blocks (no bitmap data).
    let table = build_sbix_table(&[None, None]);

    let mut rev: HashMap<u16, u16> = HashMap::new();
    rev.insert(0, 0);
    rev.insert(1, 1);

    let out = sbix::rewrite_sbix(&table, &rev, 2, 2);

    let strike_off = get_u32(&out, 8) as usize;
    let off_base = strike_off + 4;

    // All three offsets (sentinel included) should be equal → all empty.
    let o0 = get_u32(&out, off_base);
    let o1 = get_u32(&out, off_base + 4);
    let o2 = get_u32(&out, off_base + 8);
    assert_eq!(o0, o1, "GID 0 empty");
    assert_eq!(o1, o2, "GID 1 empty");
}
