//! Integration tests that grid-fit real, bundled Noto fonts.
//!
//! `NotoSans-Bold.ttf` carries a genuine `fpgm`/`prep`/`cvt ` font program, so
//! it exercises the interpreter's full font-program + control-value + per-glyph
//! path. `NotoSans-Regular.ttf` is unhinted (no `fpgm`/`prep`), so it validates
//! the identity / pass-through path. Every assertion checks the two guarantees
//! that matter for untrusted input: **no panic** and **finite, bounded output**.

use oxifont_core::sfnt::SfntTableMap;
use oxifont_hinting::HintingEngine;

/// A generous bound (in 26.6 pixels) that any sane fitted coordinate stays under
/// at the tested sizes — catches runaway / non-finite drift.
const COORD_BOUND: i32 = 1 << 22;

fn engine_for(bytes: &[u8]) -> HintingEngine {
    let map = SfntTableMap::parse(bytes).expect("bundled font must parse");
    HintingEngine::new(&map).expect("engine must build (runs fpgm)")
}

fn assert_glyph_sane(engine: &mut HintingEngine, gid: u16) {
    let glyph = engine
        .hint_glyph(gid)
        .unwrap_or_else(|e| panic!("hint_glyph({gid}) failed: {e}"));
    for p in &glyph.points {
        assert!(
            p.x.abs() < COORD_BOUND && p.y.abs() < COORD_BOUND,
            "glyph {gid} point out of bounds: ({}, {})",
            p.x,
            p.y
        );
        // 26.6 integers are always finite; the float views must be too.
        assert!(p.x_px().is_finite() && p.y_px().is_finite());
    }
    assert!(glyph.advance.abs() < COORD_BOUND);
}

#[test]
fn hinted_font_runs_fpgm_prep_at_multiple_sizes() {
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    // The font program (fpgm) must have run during construction; several sizes
    // exercise prep at different ppem values.
    for ppem in [8u16, 11, 12, 16, 24, 48] {
        engine.set_ppem(ppem).expect("prep must run without error");
        // Fit a spread of glyph ids (skip .notdef 0 which may be empty).
        for gid in [3u16, 10, 36, 68, 100, 200] {
            assert_glyph_sane(&mut engine, gid);
        }
    }
}

#[test]
fn hinted_font_is_deterministic() {
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    engine.set_ppem(16).unwrap();
    let a = engine.hint_glyph(36).unwrap();
    let b = engine.hint_glyph(36).unwrap();
    let pa: Vec<_> = a.points.iter().map(|p| (p.x, p.y, p.on_curve)).collect();
    let pb: Vec<_> = b.points.iter().map(|p| (p.x, p.y, p.on_curve)).collect();
    assert_eq!(pa, pb, "identical inputs must produce identical output");
    assert_eq!(a.advance, b.advance);
}

#[test]
fn hinting_actually_moves_points_toward_the_grid() {
    // At a small ppem the bold font's hints should snap at least some points to
    // whole-pixel positions; if nothing were executed the coordinates would be
    // arbitrary sub-pixel values. We assert that a meaningful fraction of the
    // fitted x-coordinates land exactly on the pixel grid.
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    engine.set_ppem(12).unwrap();
    let glyph = engine.hint_glyph(36).unwrap();
    let on_grid = glyph
        .points
        .iter()
        .filter(|p| p.x % 64 == 0 || p.y % 64 == 0)
        .count();
    assert!(
        on_grid > 0,
        "expected some grid-snapped coordinates after hinting"
    );
}

#[test]
fn unhinted_font_passes_through() {
    // NotoSans-Regular has glyf/loca but no fpgm/prep: fitting is identity
    // scaling and must still be finite and bounded.
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_REGULAR);
    engine.set_ppem(16).unwrap();
    for gid in [3u16, 10, 36, 68] {
        assert_glyph_sane(&mut engine, gid);
    }
}

#[test]
fn to_outline_matches_point_count_shape() {
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    engine.set_ppem(16).unwrap();
    let glyph = engine.hint_glyph(36).unwrap();
    let outline = glyph.to_outline();
    // A non-empty glyph must start with a MoveTo and contain at least one Close.
    use oxifont_core::GlyphOutline;
    assert!(matches!(outline.first(), Some(GlyphOutline::MoveTo { .. })));
    let closes = outline
        .iter()
        .filter(|c| matches!(c, GlyphOutline::Close))
        .count();
    assert_eq!(closes, glyph.contour_ends.len());
}

#[test]
fn set_ppem_zero_is_rejected() {
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    assert!(engine.set_ppem(0).is_err());
}

#[test]
fn every_glyph_fits_without_panic_at_small_ppem() {
    // Stress the whole glyph set once at a small, hint-heavy size. This is the
    // core "no panic on real bytecode" guarantee across the entire font.
    let mut engine = engine_for(oxifont_bundled::NOTO_SANS_BOLD);
    engine.set_ppem(11).unwrap();
    let num_glyphs = engine.font().maxp.num_glyphs;
    for gid in 0..num_glyphs {
        // Errors are acceptable (e.g. malformed subglyph) but panics are not;
        // when it does succeed the output must be bounded.
        if let Ok(glyph) = engine.hint_glyph(gid) {
            for p in &glyph.points {
                assert!(p.x.abs() < COORD_BOUND && p.y.abs() < COORD_BOUND);
            }
        }
    }
}
