//! Regression test for variable-font axis name resolution.
//!
//! Prior to this fix `extract_axes` stored `name_id.to_string()` (e.g. `"256"`)
//! as the axis name. It must instead resolve the numeric name ID against the
//! `name` table and return the human-readable string (e.g. `"Weight"`).

use oxifont_core::FontFace as _;
use oxifont_parser::ParsedFace;

/// Assemble a minimal SFNT font from `(tag, data)` tables, padding each table
/// to a 4-byte boundary and writing a correct table directory.
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
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfnt version 1.0
    out.extend_from_slice(&(n as u16).to_be_bytes()); // numTables

    // searchRange / entrySelector / rangeShift (values are ignored by parsers,
    // but we compute them so the header is well-formed).
    let entry_selector = (usize::BITS - 1 - n.leading_zeros()) as u16;
    let search_range = (1u16 << entry_selector) * 16;
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&((n as u16) * 16 - search_range).to_be_bytes());
    for (tag, offset, len) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum (unverified)
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
    }
    out.extend_from_slice(&body);
    out
}

fn build_head() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    t.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    t.extend_from_slice(&0u32.to_be_bytes()); // fontRevision
    t.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    t.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    t.extend_from_slice(&0u16.to_be_bytes()); // flags
    t.extend_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    t.extend_from_slice(&0u64.to_be_bytes()); // created
    t.extend_from_slice(&0u64.to_be_bytes()); // modified
    t.extend_from_slice(&0i16.to_be_bytes()); // xMin
    t.extend_from_slice(&0i16.to_be_bytes()); // yMin
    t.extend_from_slice(&0i16.to_be_bytes()); // xMax
    t.extend_from_slice(&0i16.to_be_bytes()); // yMax
    t.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    t.extend_from_slice(&8u16.to_be_bytes()); // lowestRecPPEM
    t.extend_from_slice(&0i16.to_be_bytes()); // fontDirectionHint
    t.extend_from_slice(&0i16.to_be_bytes()); // indexToLocFormat
    t.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat
    t
}

fn build_maxp() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    t.extend_from_slice(&1u16.to_be_bytes()); // numGlyphs
    for _ in 0..13 {
        t.extend_from_slice(&0u16.to_be_bytes()); // remaining v1.0 fields
    }
    t
}

fn build_hhea() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    t.extend_from_slice(&800i16.to_be_bytes()); // ascender
    t.extend_from_slice(&(-200i16).to_be_bytes()); // descender
    t.extend_from_slice(&0i16.to_be_bytes()); // lineGap
    t.extend_from_slice(&1000u16.to_be_bytes()); // advanceWidthMax
    t.extend_from_slice(&0i16.to_be_bytes()); // minLeftSideBearing
    t.extend_from_slice(&0i16.to_be_bytes()); // minRightSideBearing
    t.extend_from_slice(&1000i16.to_be_bytes()); // xMaxExtent
    t.extend_from_slice(&1i16.to_be_bytes()); // caretSlopeRise
    t.extend_from_slice(&0i16.to_be_bytes()); // caretSlopeRun
    t.extend_from_slice(&0i16.to_be_bytes()); // caretOffset
    for _ in 0..4 {
        t.extend_from_slice(&0i16.to_be_bytes()); // reserved
    }
    t.extend_from_slice(&0i16.to_be_bytes()); // metricDataFormat
    t.extend_from_slice(&1u16.to_be_bytes()); // numberOfHMetrics
    t
}

fn build_hmtx() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&500u16.to_be_bytes()); // advanceWidth
    t.extend_from_slice(&0i16.to_be_bytes()); // leftSideBearing
    t
}

/// A `name` table (format 0) with one record: Windows Unicode BMP, en-US
/// (`0x0409`), name ID 256 → `"Weight"`.
fn build_name() -> Vec<u8> {
    let s: Vec<u8> = "Weight"
        .encode_utf16()
        .flat_map(|u| u.to_be_bytes())
        .collect();
    let mut t = Vec::new();
    t.extend_from_slice(&0u16.to_be_bytes()); // format 0
    t.extend_from_slice(&1u16.to_be_bytes()); // count
    t.extend_from_slice(&18u16.to_be_bytes()); // stringOffset = 6 + 1*12
    t.extend_from_slice(&3u16.to_be_bytes()); // platformID = Windows
    t.extend_from_slice(&1u16.to_be_bytes()); // encodingID = Unicode BMP
    t.extend_from_slice(&0x0409u16.to_be_bytes()); // languageID = en-US
    t.extend_from_slice(&256u16.to_be_bytes()); // nameID
    t.extend_from_slice(&(s.len() as u16).to_be_bytes()); // length
    t.extend_from_slice(&0u16.to_be_bytes()); // offset within storage
    t.extend_from_slice(&s);
    t
}

/// An `fvar` table with a single `wght` axis whose axisNameID is 256.
fn build_fvar() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    t.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    t.extend_from_slice(&16u16.to_be_bytes()); // axesArrayOffset
    t.extend_from_slice(&2u16.to_be_bytes()); // reserved
    t.extend_from_slice(&1u16.to_be_bytes()); // axisCount
    t.extend_from_slice(&20u16.to_be_bytes()); // axisSize
    t.extend_from_slice(&0u16.to_be_bytes()); // instanceCount
    t.extend_from_slice(&0u16.to_be_bytes()); // instanceSize

    // VariationAxisRecord (20 bytes)
    t.extend_from_slice(b"wght"); // axisTag
    t.extend_from_slice(&(100i32 << 16).to_be_bytes()); // minValue 100.0 (16.16)
    t.extend_from_slice(&(400i32 << 16).to_be_bytes()); // defaultValue 400.0
    t.extend_from_slice(&(900i32 << 16).to_be_bytes()); // maxValue 900.0
    t.extend_from_slice(&0u16.to_be_bytes()); // flags
    t.extend_from_slice(&256u16.to_be_bytes()); // axisNameID
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
fn axis_name_resolves_to_name_table_string() {
    let bytes = build_variable_font();
    let face = ParsedFace::builder(bytes)
        .build()
        .expect("synthetic variable font must parse");

    let axes = face.axes();
    assert_eq!(axes.len(), 1, "one variation axis expected");
    assert_eq!(&axes[0].tag, b"wght");
    assert_eq!(
        axes[0].name, "Weight",
        "axis name must resolve to the `name` table string, not the numeric ID"
    );
    // Guard against the regression: the raw name ID was 256.
    assert_ne!(axes[0].name, "256");
}
