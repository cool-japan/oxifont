use std::collections::HashMap;

/// Rewrite a kern table, pruning pairs with removed GIDs and remapping surviving GIDs.
///
/// Format-0 subtables are handled; other formats are dropped entirely.
/// Returns the rewritten kern table bytes, or the original verbatim on parse failure.
///
/// If the result would have 0 subtables, returns an empty `Vec<u8>` so the caller
/// can decide whether to omit the kern table entirely.
pub fn rewrite_kern(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    rewrite_kern_inner(table, gid_remap).unwrap_or_else(|| table.to_vec())
}

/// Inner implementation — returns `None` on any parse error (triggers verbatim fallback).
fn rewrite_kern_inner(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    if table.len() < 4 {
        return None;
    }

    // Detect AAT kern (version == 0x0001_0000 as u32) — return verbatim.
    let first_u32 = u32::from_be_bytes([table[0], table[1], table[2], table[3]]);
    if first_u32 == 0x0001_0000 {
        return Some(table.to_vec());
    }

    // Old format: version u16 must be 0, then nTables u16.
    let version = u16::from_be_bytes([table[0], table[1]]);
    if version != 0 {
        // Unknown version — return verbatim.
        return Some(table.to_vec());
    }

    let n_tables = u16::from_be_bytes([table[2], table[3]]) as usize;

    // Walk subtables.
    let mut pos = 4usize;
    let mut rewritten_subtables: Vec<Vec<u8>> = Vec::with_capacity(n_tables);

    for _ in 0..n_tables {
        // Each subtable header: version u16, length u16, coverage u16.
        if pos + 6 > table.len() {
            return None;
        }
        // subtable version (ignored)
        let length = u16::from_be_bytes([table[pos + 2], table[pos + 3]]) as usize;
        let coverage = u16::from_be_bytes([table[pos + 4], table[pos + 5]]);

        if length < 6 {
            return None;
        }
        if pos + length > table.len() {
            return None;
        }

        let format = (coverage >> 8) & 0xFF;

        if format != 0 {
            // Non-format-0: drop this subtable.
            pos += length;
            continue;
        }

        // Parse format-0 body, starting at pos+6.
        let body_start = pos + 6;
        // Body needs at least 8 bytes for nPairs + binary-search header.
        if body_start + 8 > table.len() {
            return None;
        }
        let n_pairs = u16::from_be_bytes([table[body_start], table[body_start + 1]]) as usize;

        // Verify enough bytes for pairs.
        let pairs_start = body_start + 8;
        if pairs_start + n_pairs * 6 > table.len() {
            return None;
        }

        // Collect surviving pairs.
        let mut surviving: Vec<(u16, u16, i16)> = Vec::with_capacity(n_pairs);
        for i in 0..n_pairs {
            let base = pairs_start + i * 6;
            let left = u16::from_be_bytes([table[base], table[base + 1]]);
            let right = u16::from_be_bytes([table[base + 2], table[base + 3]]);
            let value = i16::from_be_bytes([table[base + 4], table[base + 5]]);

            if let (Some(&new_left), Some(&new_right)) =
                (gid_remap.get(&left), gid_remap.get(&right))
            {
                surviving.push((new_left, new_right, value));
            }
        }

        // Sort by (left, right) ascending.
        surviving.sort_unstable_by_key(|&(l, r, _)| (l, r));

        // Recompute binary-search header.
        let n = surviving.len();
        let (search_range, entry_selector, range_shift) = if n == 0 {
            (0u16, 0u16, 0u16)
        } else {
            let entry_selector = (n as u32).ilog2() as u16;
            let search_range = 6u16 * (1u16 << entry_selector);
            let range_shift = 6u16 * n as u16 - search_range;
            (search_range, entry_selector, range_shift)
        };

        // Serialise the subtable.
        // Subtable layout:
        //   version u16 (0)
        //   length  u16 (6 + 8 + n*6)
        //   coverage u16
        //   nPairs u16
        //   searchRange u16
        //   entrySelector u16
        //   rangeShift u16
        //   pairs...
        let subtable_length = 6 + 8 + n * 6;
        let mut st: Vec<u8> = Vec::with_capacity(subtable_length);
        st.extend_from_slice(&0u16.to_be_bytes()); // subtable version
        st.extend_from_slice(&(subtable_length as u16).to_be_bytes());
        st.extend_from_slice(&coverage.to_be_bytes());
        st.extend_from_slice(&(n as u16).to_be_bytes());
        st.extend_from_slice(&search_range.to_be_bytes());
        st.extend_from_slice(&entry_selector.to_be_bytes());
        st.extend_from_slice(&range_shift.to_be_bytes());
        for (l, r, v) in &surviving {
            st.extend_from_slice(&l.to_be_bytes());
            st.extend_from_slice(&r.to_be_bytes());
            st.extend_from_slice(&v.to_be_bytes());
        }

        rewritten_subtables.push(st);
        pos += length;
    }

    // If no format-0 subtables survived, return empty Vec so caller can omit kern.
    if rewritten_subtables.is_empty() {
        return Some(Vec::new());
    }

    // Build table header: version=0, nTables.
    let n_out = rewritten_subtables.len() as u16;
    let mut out: Vec<u8> =
        Vec::with_capacity(4 + rewritten_subtables.iter().map(|s| s.len()).sum::<usize>());
    out.extend_from_slice(&0u16.to_be_bytes()); // version
    out.extend_from_slice(&n_out.to_be_bytes()); // nTables
    for st in rewritten_subtables {
        out.extend_from_slice(&st);
    }

    Some(out)
}
