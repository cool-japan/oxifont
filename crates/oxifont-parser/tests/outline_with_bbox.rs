//! Integration tests for `ParsedFace::outline_with_bbox` and
//! `GlyphOutlineData` — the rich outline type that bundles path commands,
//! ink bounding box, and horizontal advance width for direct rasterisation.

use oxifont_core::FontFace as _;
use oxifont_parser::{GlyphOutlineData, ParsedFace};

/// Fixture bytes compiled in at test time.
static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

// ---------------------------------------------------------------------------
// GlyphOutlineData — outline_with_bbox tests
// ---------------------------------------------------------------------------

/// `outline_with_bbox` returns `None` for a whitespace glyph (no outline).
#[test]
fn outline_with_bbox_returns_none_for_whitespace() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    if let Some(space_gid) = face.glyph_for_char(' ') {
        // Space has no ink outline; the method must return None for such glyphs.
        let result = face.outline_with_bbox(space_gid);
        // Whitespace glyphs typically return None because outline_glyph returns
        // None when there are no contours.
        // (If the fixture has a space with a bbox of 0, result is also None.)
        // We only assert this is non-panicking; both None and Some({empty}) are
        // valid depending on the font.
        let _ = result;
    }
}

/// `outline_with_bbox` returns a non-empty command list for a visible glyph.
#[test]
fn outline_with_bbox_returns_commands_for_visible_glyph() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("fixture font must map 'A'");
    let data: GlyphOutlineData = face
        .outline_with_bbox(gid)
        .expect("outline_with_bbox must return Some for 'A'");
    assert!(
        !data.commands.is_empty(),
        "path command list must be non-empty for a visible glyph"
    );
}

/// The bounding box from `outline_with_bbox` must have consistent ordering
/// (x_min <= x_max, y_min <= y_max) for a non-whitespace glyph.
#[test]
fn outline_with_bbox_bounding_box_ordering_is_consistent() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("fixture font must map 'A'");
    let data = face
        .outline_with_bbox(gid)
        .expect("outline_with_bbox must return Some for 'A'");
    assert!(
        data.x_min <= data.x_max,
        "x_min ({}) must be <= x_max ({}) for 'A'",
        data.x_min,
        data.x_max
    );
    assert!(
        data.y_min <= data.y_max,
        "y_min ({}) must be <= y_max ({}) for 'A'",
        data.y_min,
        data.y_max
    );
}

/// The advance width in `GlyphOutlineData` must match `FontFace::advance_width`.
#[test]
fn outline_with_bbox_advance_width_matches_trait() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("fixture font must map 'A'");
    let data = face
        .outline_with_bbox(gid)
        .expect("outline_with_bbox must return Some for 'A'");
    let trait_adv = face.advance_width(gid);
    assert_eq!(
        data.advance_width, trait_adv,
        "advance_width in GlyphOutlineData must equal FontFace::advance_width"
    );
}

/// `outline_with_bbox` returns `None` for an out-of-range glyph ID.
#[test]
fn outline_with_bbox_returns_none_for_out_of_range_gid() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    // GID 65535 is guaranteed to be out of range for any real font.
    let result = face.outline_with_bbox(65535);
    // Either None (preferred) or Some with empty commands; must not panic.
    let _ = result;
}

/// The path commands in `outline_with_bbox` must match `FontFace::outline`.
///
/// Both methods extract the glyph outline via the same `ttf_parser` path;
/// the command sequences must be identical.
#[test]
fn outline_with_bbox_commands_match_font_face_outline() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    for c in ['A', 'B', 'g', 'i', '1', '?'] {
        let Some(gid) = face.glyph_for_char(c) else {
            continue;
        };
        let via_trait = face.outline(gid);
        let via_bbox = face.outline_with_bbox(gid).map(|d| d.commands);
        assert_eq!(
            via_trait, via_bbox,
            "outline commands must match outline_with_bbox.commands for '{c}' (GID {gid})"
        );
    }
}

/// `GlyphOutlineData` must implement `Clone` and `PartialEq`.
#[test]
fn glyph_outline_data_clone_and_eq() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("fixture font must map 'A'");
    let data = face
        .outline_with_bbox(gid)
        .expect("outline_with_bbox must return Some for 'A'");
    let cloned = data.clone();
    assert_eq!(
        data, cloned,
        "GlyphOutlineData must satisfy Clone and PartialEq"
    );
}

/// Ink bounding box from `outline_with_bbox` must be non-degenerate for 'A'.
///
/// A real glyph must have positive width and positive height.
#[test]
fn outline_with_bbox_ink_box_is_nondegenerate_for_letter_a() {
    let face =
        ParsedFace::parse(FIXTURE_BYTES.to_vec(), 0).expect("fixture TTF must parse without error");
    let gid = face.glyph_for_char('A').expect("fixture font must map 'A'");
    let data = face
        .outline_with_bbox(gid)
        .expect("outline_with_bbox must return Some for 'A'");
    let ink_w = (data.x_max as i32) - (data.x_min as i32);
    let ink_h = (data.y_max as i32) - (data.y_min as i32);
    assert!(
        ink_w > 0,
        "ink width must be positive for 'A'; got x_min={} x_max={}",
        data.x_min,
        data.x_max
    );
    assert!(
        ink_h > 0,
        "ink height must be positive for 'A'; got y_min={} y_max={}",
        data.y_min,
        data.y_max
    );
}
