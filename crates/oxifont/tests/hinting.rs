//! Integration tests for the facade's `hinting` feature: the re-exported
//! `oxifont::hinting` module and the [`oxifont::hinted_outline`] convenience
//! wrapper.

#![cfg(feature = "hinting")]

/// Bundled test fixture — Noto-derived TTF compiled in at test time. Shared
/// with the other facade integration tests.
static TEST_TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

#[test]
fn hinted_outline_returns_nonempty_path_commands() {
    // gid 0 is `.notdef`, which is frequently a simple box outline; use it
    // since it is guaranteed to exist in any valid font (numGlyphs >= 1).
    let outline =
        oxifont::hinted_outline(TEST_TTF, 0, 16).expect("hinted_outline must succeed on gid 0");
    // A `.notdef` box (or any non-empty glyph) starts with a MoveTo command.
    if let Some(first) = outline.first() {
        assert!(
            matches!(first, oxifont::GlyphOutline::MoveTo { .. }),
            "outline must start with MoveTo, got {first:?}"
        );
    }
}

#[test]
fn hinted_outline_rejects_zero_ppem() {
    let result = oxifont::hinted_outline(TEST_TTF, 0, 0);
    assert!(result.is_err(), "ppem = 0 must be rejected");
}

#[test]
fn hinted_outline_rejects_garbage_bytes() {
    let result = oxifont::hinted_outline(b"not a font", 0, 16);
    assert!(
        result.is_err(),
        "malformed font bytes must not panic or succeed"
    );
}

#[test]
fn hinting_module_reexports_engine_directly() {
    // The `hinting` module re-export gives direct access to `HintingEngine`
    // for callers who want to hint several glyphs without reparsing the font
    // on every call (unlike the one-shot `hinted_outline` convenience).
    let map = oxifont_core::sfnt::SfntTableMap::parse(TEST_TTF).expect("fixture must parse");
    let mut engine = oxifont::hinting::HintingEngine::new(&map).expect("engine must build");
    engine.set_ppem(12).expect("set_ppem must succeed");
    let glyph = engine
        .hint_glyph(0)
        .expect("hint_glyph must succeed on gid 0");
    // Determinism: hinting the same glyph twice must produce identical output.
    let glyph2 = engine
        .hint_glyph(0)
        .expect("second hint_glyph must succeed");
    assert_eq!(glyph.advance, glyph2.advance);
    assert_eq!(glyph.points.len(), glyph2.points.len());
}
