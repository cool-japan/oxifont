/// COLR table subsetting: remap base glyph and layer GIDs, drop records for removed GIDs.
///
/// COLR v0 format:
/// - Header (14 bytes): version(u16), numBaseGlyphRecords(u16),
///   baseGlyphRecordsOffset(u32), layerRecordsOffset(u32), numLayerRecords(u16).
/// - BaseGlyphRecord (6 bytes): gID(u16), firstLayerIndex(u16), numLayers(u16).
/// - LayerRecord (4 bytes): gID(u16), paletteIndex(u16).
///
/// COLR v1+ is preserved verbatim (complex paint graph; GID references are
/// deeply embedded and require full paint-table traversal to remap).
use std::collections::HashMap;

// ─── parse helpers ────────────────────────────────────────────────────────────

#[inline]
fn r_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

#[inline]
fn r_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

// ─── internal structures ──────────────────────────────────────────────────────

struct BaseGlyphRecord {
    gid: u16,
    first_layer_index: u16,
    num_layers: u16,
}

struct LayerRecord {
    gid: u16,
    palette_index: u16,
}

// ─── public API ───────────────────────────────────────────────────────────────

/// Rewrite a COLR table to reflect the new GID space.
///
/// For COLR v0:
/// - Base glyphs not in `gid_remap` are dropped.
/// - Layers referencing GIDs not in `gid_remap` are dropped.
/// - If a base glyph ends up with zero surviving layers it is also dropped.
/// - Surviving GIDs are remapped to their new values.
/// - Output BaseGlyphRecords are sorted by new GID (required for binary search).
///
/// For COLR v1+: returned verbatim (complex paint graph; no subsetting applied).
/// On any parse failure: returned verbatim.
pub fn rewrite_colr(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    rewrite_colr_inner(table, gid_remap).unwrap_or_else(|| table.to_vec())
}

fn rewrite_colr_inner(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    // Minimum header is 14 bytes.
    if table.len() < 14 {
        return None;
    }

    let version = r_u16(table, 0)?;
    // COLR v1+ is not subsetted — preserve verbatim.
    if version != 0 {
        return Some(table.to_vec());
    }

    let num_base = r_u16(table, 2)? as usize;
    let base_offset = r_u32(table, 4)? as usize;
    let layer_offset = r_u32(table, 8)? as usize;
    let num_layers = r_u16(table, 12)? as usize;

    // Validate offsets and record counts.
    if base_offset.checked_add(num_base * 6)? > table.len() {
        return None;
    }
    if layer_offset.checked_add(num_layers * 4)? > table.len() {
        return None;
    }

    // Parse BaseGlyphRecords.
    let mut base_records: Vec<BaseGlyphRecord> = Vec::with_capacity(num_base);
    for i in 0..num_base {
        let off = base_offset + i * 6;
        base_records.push(BaseGlyphRecord {
            gid: r_u16(table, off)?,
            first_layer_index: r_u16(table, off + 2)?,
            num_layers: r_u16(table, off + 4)?,
        });
    }

    // Parse LayerRecords.
    let mut layer_records: Vec<LayerRecord> = Vec::with_capacity(num_layers);
    for i in 0..num_layers {
        let off = layer_offset + i * 4;
        layer_records.push(LayerRecord {
            gid: r_u16(table, off)?,
            palette_index: r_u16(table, off + 2)?,
        });
    }

    // ─── Build surviving records ──────────────────────────────────────────────

    // New flat layer array (all surviving layers from all surviving base glyphs).
    let mut new_layers: Vec<(u16, u16)> = Vec::new();
    // New base glyph array: (new_gid, first_layer_index_in_new_array, num_layers).
    let mut new_bases: Vec<(u16, u16, u16)> = Vec::new();

    for bg in &base_records {
        // Skip base glyphs whose GID was removed.
        let &new_gid = match gid_remap.get(&bg.gid) {
            Some(g) => g,
            None => continue,
        };

        let first = bg.first_layer_index as usize;
        let count = bg.num_layers as usize;

        // Bounds check against the layer array.
        let end = first.checked_add(count)?;
        if end > layer_records.len() {
            return None;
        }

        // Collect surviving layers for this base glyph.
        let first_new_layer_idx = new_layers.len() as u16;
        let mut surviving_layer_count: u16 = 0;

        for layer in &layer_records[first..end] {
            // Layers referencing removed GIDs are dropped.
            if let Some(&new_layer_gid) = gid_remap.get(&layer.gid) {
                new_layers.push((new_layer_gid, layer.palette_index));
                surviving_layer_count += 1;
            }
        }

        // If all layers were dropped, drop the base glyph too.
        if surviving_layer_count == 0 {
            continue;
        }

        new_bases.push((new_gid, first_new_layer_idx, surviving_layer_count));
    }

    // Sort base glyphs by new GID (required for binary search by renderers).
    new_bases.sort_unstable_by_key(|&(g, _, _)| g);

    // ─── Serialise ────────────────────────────────────────────────────────────

    let new_num_base = new_bases.len();
    let new_num_layers = new_layers.len();

    // Offsets:
    // Header: 14 bytes
    // BaseGlyphRecords start immediately after header.
    let new_base_offset: u32 = 14;
    // LayerRecords start immediately after BaseGlyphRecords.
    let new_layer_offset: u32 = new_base_offset + (new_num_base as u32) * 6;

    let total_size = new_layer_offset as usize + new_num_layers * 4;
    let mut out: Vec<u8> = Vec::with_capacity(total_size);

    // Header
    out.extend_from_slice(&0u16.to_be_bytes()); // version = 0
    out.extend_from_slice(&(new_num_base as u16).to_be_bytes());
    out.extend_from_slice(&new_base_offset.to_be_bytes());
    out.extend_from_slice(&new_layer_offset.to_be_bytes());
    out.extend_from_slice(&(new_num_layers as u16).to_be_bytes());

    // BaseGlyphRecords
    for (gid, first_layer, num_lay) in &new_bases {
        out.extend_from_slice(&gid.to_be_bytes());
        out.extend_from_slice(&first_layer.to_be_bytes());
        out.extend_from_slice(&num_lay.to_be_bytes());
    }

    // LayerRecords
    for (gid, palette_idx) in &new_layers {
        out.extend_from_slice(&gid.to_be_bytes());
        out.extend_from_slice(&palette_idx.to_be_bytes());
    }

    Some(out)
}
