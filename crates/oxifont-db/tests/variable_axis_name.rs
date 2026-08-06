//! Regression test for variable-font axis name resolution in oxifont-db.
//!
//! Prior to this fix `load::extract_axes` stored `String::new()` (an empty
//! string) as every axis name. It must instead resolve the numeric name ID
//! against the `name` table and return the human-readable string.

use oxifont_db::FontDatabase;

fn assemble(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|t| t.0);
    let n = tables.len();
    let dir_size = 12 + n * 16;

    let mut body = Vec::new();
    let mut records: Vec<([u8; 4], u32, u32)> = Vec::new();
    for (tag, data) in &tables {
        let offset = (dir_size + body.len()) as u32;
        records.push((*tag, offset, data.len() as u32));
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(n as u16).to_be_bytes());
    let entry_selector = (usize::BITS - 1 - n.leading_zeros()) as u16;
    let search_range = (1u16 << entry_selector) * 16;
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&((n as u16) * 16 - search_range).to_be_bytes());
    for (tag, offset, len) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
    }
    out.extend_from_slice(&body);
    out
}

fn build_head() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&0u32.to_be_bytes());
    t.extend_from_slice(&0u32.to_be_bytes());
    t.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&1000u16.to_be_bytes());
    t.extend_from_slice(&0u64.to_be_bytes());
    t.extend_from_slice(&0u64.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&8u16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t
}

fn build_hhea() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    t.extend_from_slice(&800i16.to_be_bytes());
    t.extend_from_slice(&(-200i16).to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&1000u16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&1000i16.to_be_bytes());
    t.extend_from_slice(&1i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    for _ in 0..4 {
        t.extend_from_slice(&0i16.to_be_bytes());
    }
    t.extend_from_slice(&0i16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t
}

fn build_hmtx() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&500u16.to_be_bytes());
    t.extend_from_slice(&0i16.to_be_bytes());
    t
}

fn build_maxp() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    for _ in 0..13 {
        t.extend_from_slice(&0u16.to_be_bytes());
    }
    t
}

/// A `name` table (format 0) with name ID 1 (family) = "Synth" and name ID 256
/// (axis name) = "Weight", both Windows Unicode BMP / en-US.
fn build_name() -> Vec<u8> {
    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
    }
    let family = utf16("Synth");
    let axis = utf16("Weight");
    let mut t = Vec::new();
    t.extend_from_slice(&0u16.to_be_bytes()); // format 0
    t.extend_from_slice(&2u16.to_be_bytes()); // count
    let string_offset = 6 + 2 * 12; // header + 2 records
    t.extend_from_slice(&(string_offset as u16).to_be_bytes());
    // record 0: family (nameID 1)
    t.extend_from_slice(&3u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0x0409u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&(family.len() as u16).to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    // record 1: axis name (nameID 256)
    t.extend_from_slice(&3u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0x0409u16.to_be_bytes());
    t.extend_from_slice(&256u16.to_be_bytes());
    t.extend_from_slice(&(axis.len() as u16).to_be_bytes());
    t.extend_from_slice(&(family.len() as u16).to_be_bytes());
    t.extend_from_slice(&family);
    t.extend_from_slice(&axis);
    t
}

fn build_fvar() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&16u16.to_be_bytes());
    t.extend_from_slice(&2u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&20u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(b"wght");
    t.extend_from_slice(&(100i32 << 16).to_be_bytes());
    t.extend_from_slice(&(400i32 << 16).to_be_bytes());
    t.extend_from_slice(&(900i32 << 16).to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&256u16.to_be_bytes());
    t
}

fn build_variable_font() -> Vec<u8> {
    assemble(vec![
        (*b"head", build_head()),
        (*b"hhea", build_hhea()),
        (*b"hmtx", build_hmtx()),
        (*b"maxp", build_maxp()),
        (*b"name", build_name()),
        (*b"fvar", build_fvar()),
    ])
}

#[test]
fn db_axis_name_resolves_to_name_table_string() {
    let bytes = build_variable_font();
    let mut db = FontDatabase::new();
    let added = db.load_bytes(bytes);
    assert_eq!(added, 1, "synthetic variable font must load as one face");

    let face = &db.faces()[0];
    assert_eq!(face.variable_axes.len(), 1, "one variation axis expected");
    assert_eq!(&face.variable_axes[0].tag, b"wght");
    assert_eq!(
        face.variable_axes[0].name, "Weight",
        "axis name must resolve to the `name` table string, not an empty string"
    );
    assert!(
        !face.variable_axes[0].name.is_empty(),
        "axis name must not be the old empty-string placeholder"
    );
}
