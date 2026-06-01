use oxifont_subset::kern::rewrite_kern;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a format-0 kern subtable (6-byte header + body).
///
/// coverage = 0x0001 (horizontal, format 0)
fn build_kern_subtable_f0(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
    let n = pairs.len();
    let subtable_len = 6 + 8 + n * 6; // header + body-header + pairs

    // Binary-search header values.
    let (search_range, entry_selector, range_shift) = if n == 0 {
        (0u16, 0u16, 0u16)
    } else {
        let es = (n as u32).ilog2() as u16;
        let sr = 6u16 * (1u16 << es);
        let rs = 6u16 * n as u16 - sr;
        (sr, es, rs)
    };

    let mut out = Vec::with_capacity(subtable_len);

    // Subtable header (6 bytes).
    out.extend_from_slice(&0u16.to_be_bytes()); // subtable version
    out.extend_from_slice(&(subtable_len as u16).to_be_bytes()); // length
    out.extend_from_slice(&0x0001u16.to_be_bytes()); // coverage (format=0, horizontal)

    // Body header (8 bytes).
    out.extend_from_slice(&(n as u16).to_be_bytes()); // nPairs
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // Pairs.
    for &(left, right, value) in pairs {
        out.extend_from_slice(&left.to_be_bytes());
        out.extend_from_slice(&right.to_be_bytes());
        out.extend_from_slice(&value.to_be_bytes());
    }

    out
}

/// Build a kern table header + concatenated subtables (old format, version=0).
fn build_kern_table(subtables: &[Vec<u8>]) -> Vec<u8> {
    let n_tables = subtables.len() as u16;
    let body_len: usize = subtables.iter().map(|s| s.len()).sum();
    let mut out = Vec::with_capacity(4 + body_len);
    out.extend_from_slice(&0u16.to_be_bytes()); // version = 0
    out.extend_from_slice(&n_tables.to_be_bytes());
    for st in subtables {
        out.extend_from_slice(st);
    }
    out
}

/// Extract the pairs from the first format-0 subtable in a rewritten kern table.
/// Returns Vec<(left, right, value)>.
fn extract_pairs(kern_bytes: &[u8]) -> Vec<(u16, u16, i16)> {
    assert!(kern_bytes.len() >= 4, "kern table too short for header");
    let n_tables = u16::from_be_bytes([kern_bytes[2], kern_bytes[3]]) as usize;
    assert!(n_tables >= 1, "no subtables in output");

    // First subtable starts at byte 4.
    let st = &kern_bytes[4..];
    assert!(st.len() >= 6, "subtable header too short");
    let length = u16::from_be_bytes([st[2], st[3]]) as usize;
    assert!(st.len() >= length, "subtable truncated");

    // Body starts at offset 6 from subtable start.
    let body = &st[6..length];
    assert!(body.len() >= 8, "subtable body too short");
    let n_pairs = u16::from_be_bytes([body[0], body[1]]) as usize;
    assert!(body.len() >= 8 + n_pairs * 6, "pairs truncated");

    let mut pairs = Vec::with_capacity(n_pairs);
    for i in 0..n_pairs {
        let base = 8 + i * 6;
        let left = u16::from_be_bytes([body[base], body[base + 1]]);
        let right = u16::from_be_bytes([body[base + 2], body[base + 3]]);
        let value = i16::from_be_bytes([body[base + 4], body[base + 5]]);
        pairs.push((left, right, value));
    }
    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Basic remap: 3 pairs, one dropped (GID not in remap), two remapped.
#[test]
fn test_kern_remap_basic() {
    // Original GIDs: 10, 11, 12
    // Pairs: (10,11,100), (10,12,-50), (11,12,200)
    // Remap: 10→1, 12→2  (GID 11 is removed)
    let pairs = vec![(10u16, 11u16, 100i16), (10, 12, -50), (11, 12, 200)];
    let subtable = build_kern_subtable_f0(&pairs);
    let table = build_kern_table(&[subtable]);

    let mut remap: HashMap<u16, u16> = HashMap::new();
    remap.insert(10, 1);
    remap.insert(12, 2);

    let result = rewrite_kern(&table, &remap);
    assert!(!result.is_empty(), "should produce output");

    let out_pairs = extract_pairs(&result);
    // Only (10→1, 12→2, -50) survives.
    assert_eq!(out_pairs.len(), 1);
    assert_eq!(out_pairs[0], (1, 2, -50));
}

/// All pairs removed → valid kern subtable with 0 pairs (non-empty bytes).
#[test]
fn test_kern_all_removed() {
    let pairs = vec![(10u16, 11u16, 100i16), (12, 13, -50)];
    let subtable = build_kern_subtable_f0(&pairs);
    let table = build_kern_table(&[subtable]);

    // Remap has none of 10,11,12,13.
    let remap: HashMap<u16, u16> = HashMap::new();
    let result = rewrite_kern(&table, &remap);

    // Not empty — a valid kern table with 0 pairs is returned.
    assert!(!result.is_empty());

    let header_n_tables = u16::from_be_bytes([result[2], result[3]]);
    assert_eq!(header_n_tables, 1, "should still have 1 subtable");

    let out_pairs = extract_pairs(&result);
    assert_eq!(out_pairs.len(), 0, "no pairs should survive");
}

/// Verify searchRange/entrySelector/rangeShift for known n values.
#[test]
fn test_kern_binary_search_header() {
    // n → (searchRange, entrySelector, rangeShift)
    // searchRange = 6 * 2^floor(log2(n))
    // rangeShift  = 6*n - searchRange
    let cases: &[(usize, u16, u16, u16)] = &[
        (1, 6, 0, 0),
        (2, 12, 1, 0),
        (3, 12, 1, 6),
        (4, 24, 2, 0),
        (7, 24, 2, 18),
        (8, 48, 3, 0),
    ];

    for &(n, exp_sr, exp_es, exp_rs) in cases {
        // Build a remap that keeps exactly n pairs numbered 0..n.
        let pairs: Vec<(u16, u16, i16)> =
            (0..n as u16).map(|i| (i * 2, i * 2 + 1, 10i16)).collect();
        let subtable = build_kern_subtable_f0(&pairs);
        let table = build_kern_table(&[subtable]);

        // Identity remap (all GIDs map to themselves).
        let remap: HashMap<u16, u16> = pairs
            .iter()
            .flat_map(|&(l, r, _)| [(l, l), (r, r)])
            .collect();

        let result = rewrite_kern(&table, &remap);
        assert!(!result.is_empty(), "n={n}: output should not be empty");

        // Read binary-search fields from first subtable body.
        // Table header: 4 bytes. Subtable header: 6 bytes. Body at offset 10.
        assert!(result.len() >= 4 + 6 + 8, "n={n}: result too short");
        let body_off = 4 + 6; // table header + subtable header
        let sr = u16::from_be_bytes([result[body_off + 2], result[body_off + 3]]);
        let es = u16::from_be_bytes([result[body_off + 4], result[body_off + 5]]);
        let rs = u16::from_be_bytes([result[body_off + 6], result[body_off + 7]]);

        assert_eq!(sr, exp_sr, "n={n} searchRange mismatch");
        assert_eq!(es, exp_es, "n={n} entrySelector mismatch");
        assert_eq!(rs, exp_rs, "n={n} rangeShift mismatch");
    }
}

/// Non-format-0 subtable is dropped; nothing remains → empty output.
#[test]
fn test_kern_nonzero_format_dropped() {
    // Construct a subtable with format=2 (coverage = 0x0201).
    let subtable_len: u16 = 6 + 8; // minimal non-zero body
    let mut fake_subtable = Vec::new();
    fake_subtable.extend_from_slice(&0u16.to_be_bytes()); // version
    fake_subtable.extend_from_slice(&subtable_len.to_be_bytes()); // length
    fake_subtable.extend_from_slice(&0x0201u16.to_be_bytes()); // coverage (format=2)
                                                               // Pad out to declared length.
    fake_subtable.resize(subtable_len as usize, 0u8);

    let table = build_kern_table(&[fake_subtable]);
    let remap: HashMap<u16, u16> = [(1, 1), (2, 2)].into_iter().collect();

    let result = rewrite_kern(&table, &remap);
    // No format-0 subtables → empty output.
    assert!(
        result.is_empty(),
        "non-format-0 subtables should be dropped, resulting in empty output"
    );
}

/// Input shorter than 4 bytes → returns verbatim.
#[test]
fn test_kern_empty_input() {
    let short = vec![0u8, 0u8, 0u8]; // 3 bytes < 4
    let remap: HashMap<u16, u16> = [(1, 1)].into_iter().collect();
    let result = rewrite_kern(&short, &remap);
    // Verbatim fallback.
    assert_eq!(result, short);
}

/// Surviving pairs are sorted by (left, right) ascending.
#[test]
fn test_kern_sorted_output() {
    // Provide pairs in reverse order.
    let pairs = vec![
        (30u16, 40u16, 10i16),
        (10, 20, 20),
        (20, 30, 30),
        (10, 30, 40),
    ];
    let subtable = build_kern_subtable_f0(&pairs);
    let table = build_kern_table(&[subtable]);

    // All GIDs survive, identity remap.
    let remap: HashMap<u16, u16> = [10, 20, 30, 40].iter().map(|&g| (g, g)).collect();

    let result = rewrite_kern(&table, &remap);
    let out_pairs = extract_pairs(&result);

    assert_eq!(out_pairs.len(), 4);
    // Should be sorted ascending by (left, right).
    let expected = vec![
        (10u16, 20u16, 20i16),
        (10, 30, 40),
        (20, 30, 30),
        (30, 40, 10),
    ];
    assert_eq!(out_pairs, expected);
}
