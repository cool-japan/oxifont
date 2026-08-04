//! In-crate instruction-level, graphics-state, and adversarial-program tests.
//!
//! These build a minimal synthetic TrueType font in memory (so no external
//! assets are needed) and exercise the VM directly through pub(crate) helpers.

use crate::error::HintingError;
use crate::interp::HintingEngine;
use crate::math::{Vector, ONE_2DOT14};
use crate::state::{GraphicsState, Point, Zone};

use oxifont_core::sfnt::SfntTableMap;

// ── Test-only introspection helpers ─────────────────────────────────────────

impl HintingEngine {
    /// Run a bytecode program from a clean per-run state.
    fn exec_for_test(&mut self, code: &[u8]) -> Result<(), HintingError> {
        self.begin_run();
        self.execute(code)
    }

    /// The live operand stack.
    fn test_stack(&self) -> &[i32] {
        &self.stack
    }

    /// The live graphics state.
    fn test_gs(&self) -> &GraphicsState {
        &self.gs
    }

    /// Install a synthetic glyph zone.
    fn set_test_glyph_zone(&mut self, points: Vec<Point>, contour_ends: Vec<u16>) {
        self.glyph_zone = Zone {
            points,
            contour_ends,
        };
    }

    /// Read a glyph-zone point.
    fn test_glyph_point(&self, idx: usize) -> Point {
        self.glyph_zone.points[idx]
    }
}

// ── Minimal synthetic font construction ─────────────────────────────────────

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn build_table_directory(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let num = tables.len() as u16;
    let mut out = Vec::new();
    // Offset table (12 bytes) + directory (16 * num).
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfnt version (TrueType)
    out.extend_from_slice(&be16(num));
    // searchRange / entrySelector / rangeShift — not validated, keep zero.
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));

    let dir_start = out.len();
    let data_start = dir_start + 16 * tables.len();
    // Reserve directory space.
    out.resize(data_start, 0);

    for (i, (tag, data)) in tables.iter().enumerate() {
        let entry = dir_start + i * 16;
        let offset = out.len();
        out[entry..entry + 4].copy_from_slice(*tag);
        // checksum (unused)
        out[entry + 8..entry + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        out[entry + 12..entry + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        // 4-byte align the next table.
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    out
}

/// A triangle glyph: 3 on-curve points forming one contour, no instructions.
fn triangle_glyph() -> Vec<u8> {
    let mut g = Vec::new();
    g.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
    g.extend_from_slice(&10i16.to_be_bytes()); // xMin
    g.extend_from_slice(&10i16.to_be_bytes()); // yMin
    g.extend_from_slice(&100i16.to_be_bytes()); // xMax
    g.extend_from_slice(&100i16.to_be_bytes()); // yMax
    g.extend_from_slice(&2u16.to_be_bytes()); // endPtsOfContours[0]
    g.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
    g.extend_from_slice(&[0x01, 0x01, 0x01]); // flags: all on-curve, i16 deltas
                                              // x deltas (i16): 10, 90, -50 -> x = 10, 100, 50
    g.extend_from_slice(&10i16.to_be_bytes());
    g.extend_from_slice(&90i16.to_be_bytes());
    g.extend_from_slice(&(-50i16).to_be_bytes());
    // y deltas (i16): 10, 0, 90 -> y = 10, 10, 100
    g.extend_from_slice(&10i16.to_be_bytes());
    g.extend_from_slice(&0i16.to_be_bytes());
    g.extend_from_slice(&90i16.to_be_bytes());
    // Pad to an even length so the short `loca` (offset * 2) can address the end.
    if g.len() % 2 != 0 {
        g.push(0);
    }
    g
}

fn build_test_font() -> Vec<u8> {
    // head: 54 bytes; unitsPerEm@18, indexToLocFormat@50.
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
                                                          // indexToLocFormat = 0 (short) already zero.

    // maxp v1.0, 32 bytes.
    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&2u16.to_be_bytes()); // numGlyphs
    maxp[16..18].copy_from_slice(&16u16.to_be_bytes()); // maxTwilightPoints
    maxp[18..20].copy_from_slice(&64u16.to_be_bytes()); // maxStorage
    maxp[20..22].copy_from_slice(&16u16.to_be_bytes()); // maxFunctionDefs
    maxp[24..26].copy_from_slice(&256u16.to_be_bytes()); // maxStackElements

    // glyf: gid 0 empty, gid 1 triangle.
    let tri = triangle_glyph();
    let glyf = tri.clone();

    // loca (short): offsets/2 for gid0..=gid2. gid0 at 0 (empty), gid1 at 0, gid2 at end.
    let mut loca = Vec::new();
    loca.extend_from_slice(&0u16.to_be_bytes()); // gid0 start
    loca.extend_from_slice(&0u16.to_be_bytes()); // gid1 start (gid0 empty)
    loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes()); // gid2 = end

    // cvt: 4 entries.
    let mut cvt = Vec::new();
    for v in [0i16, 100, 200, -50] {
        cvt.extend_from_slice(&v.to_be_bytes());
    }

    // hhea: numberOfHMetrics@34; 36 bytes.
    let mut hhea = vec![0u8; 36];
    hhea[34..36].copy_from_slice(&2u16.to_be_bytes());
    // hmtx: 2 long metrics (advance, lsb).
    let mut hmtx = Vec::new();
    hmtx.extend_from_slice(&600u16.to_be_bytes()); // gid0 advance
    hmtx.extend_from_slice(&0i16.to_be_bytes());
    hmtx.extend_from_slice(&600u16.to_be_bytes()); // gid1 advance
    hmtx.extend_from_slice(&10i16.to_be_bytes());

    build_table_directory(&[
        (b"head", head),
        (b"maxp", maxp),
        (b"cvt ", cvt),
        (b"loca", loca),
        (b"glyf", glyf),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
    ])
}

fn engine() -> HintingEngine {
    let font = build_test_font();
    let map = SfntTableMap::parse(&font).expect("synthetic font must parse");
    let mut e = HintingEngine::new(&map).expect("engine must build");
    e.set_ppem(16).expect("set_ppem");
    e
}

// ── Arithmetic and stack ─────────────────────────────────────────────────────

#[test]
fn push_and_add() {
    let mut e = engine();
    // PUSHB[1] 5 10 ; ADD
    e.exec_for_test(&[0xB1, 5, 10, 0x60]).unwrap();
    assert_eq!(e.test_stack(), &[15]);
}

#[test]
fn subtract_and_mul() {
    let mut e = engine();
    // PUSHB[1] 128 192 ; MUL  (2px * 3px = 6px = 384)
    e.exec_for_test(&[0xB8, 0x00, 0x80, 0xB8, 0x00, 0xC0, 0x63])
        .unwrap();
    assert_eq!(e.test_stack(), &[384]);
}

#[test]
fn divide_by_zero_errs() {
    let mut e = engine();
    // PUSHB[1] 64 0 ; DIV
    let r = e.exec_for_test(&[0xB1, 64, 0, 0x62]);
    assert_eq!(r, Err(HintingError::DivideByZero));
}

#[test]
fn dup_swap_roll() {
    let mut e = engine();
    e.exec_for_test(&[0xB0, 7, 0x20]).unwrap(); // PUSHB[0] 7 ; DUP
    assert_eq!(e.test_stack(), &[7, 7]);

    let mut e = engine();
    e.exec_for_test(&[0xB1, 1, 2, 0x23]).unwrap(); // PUSHB[1] 1 2 ; SWAP
    assert_eq!(e.test_stack(), &[2, 1]);

    let mut e = engine();
    e.exec_for_test(&[0xB2, 1, 2, 3, 0x8A]).unwrap(); // PUSHB[2] 1 2 3 ; ROLL
    assert_eq!(e.test_stack(), &[2, 3, 1]);
}

#[test]
fn cindex_copies_deep_element() {
    let mut e = engine();
    // PUSHB[2] 10 20 30 ; PUSHB[0] 3 ; CINDEX -> copy 3rd-from-top (10)
    e.exec_for_test(&[0xB2, 10, 20, 30, 0xB0, 3, 0x25]).unwrap();
    assert_eq!(e.test_stack(), &[10, 20, 30, 10]);
}

#[test]
fn mindex_moves_deep_element() {
    let mut e = engine();
    // PUSHB[2] 10 20 30 ; PUSHB[0] 3 ; MINDEX -> move 3rd-from-top (10) to top
    e.exec_for_test(&[0xB2, 10, 20, 30, 0xB0, 3, 0x26]).unwrap();
    assert_eq!(e.test_stack(), &[20, 30, 10]);
}

#[test]
fn comparisons_and_logic() {
    let mut e = engine();
    e.exec_for_test(&[0xB1, 3, 5, 0x50]).unwrap(); // LT 3<5
    assert_eq!(e.test_stack(), &[1]);

    let mut e = engine();
    e.exec_for_test(&[0xB1, 5, 5, 0x54]).unwrap(); // EQ 5==5
    assert_eq!(e.test_stack(), &[1]);

    let mut e = engine();
    e.exec_for_test(&[0xB0, 0, 0x5C]).unwrap(); // NOT 0 -> 1
    assert_eq!(e.test_stack(), &[1]);
}

// ── Rounding and graphics state ──────────────────────────────────────────────

#[test]
fn rtg_then_round_snaps_to_pixel() {
    let mut e = engine();
    // RTG ; PUSHB[0] 70 ; ROUND[00]
    e.exec_for_test(&[0x18, 0xB0, 70, 0x68]).unwrap();
    assert_eq!(e.test_stack(), &[64]);
}

#[test]
fn rutg_rounds_up() {
    let mut e = engine();
    // RUTG ; PUSHB[0] 65 ; ROUND
    e.exec_for_test(&[0x7C, 0xB0, 65, 0x68]).unwrap();
    assert_eq!(e.test_stack(), &[128]);
}

#[test]
fn svtca_sets_axis_vectors() {
    let mut e = engine();
    e.exec_for_test(&[0x01]).unwrap(); // SVTCA[1] -> x axis
    assert_eq!(e.test_gs().projection, Vector::X_AXIS);
    assert_eq!(e.test_gs().freedom, Vector::X_AXIS);

    let mut e = engine();
    e.exec_for_test(&[0x00]).unwrap(); // SVTCA[0] -> y axis
    assert_eq!(e.test_gs().projection, Vector::Y_AXIS);
    assert_eq!(e.test_gs().freedom, Vector::Y_AXIS);
}

#[test]
fn gpv_reports_projection_vector() {
    let mut e = engine();
    // SVTCA[1] then GPV -> push x=0x4000, y=0
    e.exec_for_test(&[0x01, 0x0C]).unwrap();
    assert_eq!(e.test_stack(), &[ONE_2DOT14, 0]);
}

#[test]
fn sloop_sets_loop_counter() {
    let mut e = engine();
    e.exec_for_test(&[0xB0, 5, 0x17]).unwrap(); // PUSHB[0] 5 ; SLOOP
    assert_eq!(e.test_gs().loop_counter, 5);
}

// ── Storage and CVT ──────────────────────────────────────────────────────────

#[test]
fn storage_write_read_roundtrip() {
    let mut e = engine();
    // WS(3,42) ; RS(3)
    e.exec_for_test(&[0xB1, 3, 42, 0x42, 0xB0, 3, 0x43])
        .unwrap();
    assert_eq!(e.test_stack(), &[42]);
}

#[test]
fn cvt_write_read_roundtrip() {
    let mut e = engine();
    // WCVTP(0, 128) ; RCVT(0)
    e.exec_for_test(&[0xB1, 0, 128, 0x44, 0xB0, 0, 0x45])
        .unwrap();
    assert_eq!(e.test_stack(), &[128]);
}

// ── Control flow ─────────────────────────────────────────────────────────────

#[test]
fn if_true_branch_executes() {
    let mut e = engine();
    // PUSHB[0] 1 ; IF ; PUSHB[0] 99 ; EIF
    e.exec_for_test(&[0xB0, 1, 0x58, 0xB0, 99, 0x59]).unwrap();
    assert_eq!(e.test_stack(), &[99]);
}

#[test]
fn if_false_takes_else() {
    let mut e = engine();
    // PUSHB[0] 0 ; IF ; PUSHB[0] 11 ; ELSE ; PUSHB[0] 22 ; EIF
    e.exec_for_test(&[0xB0, 0, 0x58, 0xB0, 11, 0x1B, 0xB0, 22, 0x59])
        .unwrap();
    assert_eq!(e.test_stack(), &[22]);
}

#[test]
fn function_define_and_call() {
    let mut e = engine();
    // FDEF 0 { PUSHB[0] 7 } ENDF ; CALL 0
    // bytes: PUSHB[0] 0 ; FDEF ; PUSHB[0] 7 ; ENDF ; PUSHB[0] 0 ; CALL
    e.exec_for_test(&[0xB0, 0, 0x2C, 0xB0, 7, 0x2D, 0xB0, 0, 0x2B])
        .unwrap();
    assert_eq!(e.test_stack(), &[7]);
}

// ── Point movement ───────────────────────────────────────────────────────────

#[test]
fn mdap_rounds_point_to_grid() {
    let mut e = engine();
    // One point at x = 70 (26.6), projection/freedom on x axis (default).
    let p = Point {
        cur_x: 70,
        cur_y: 0,
        org_x: 70,
        org_y: 0,
        on_curve: true,
        touched_x: false,
        touched_y: false,
    };
    e.set_test_glyph_zone(vec![p], vec![0]);
    // SVTCA[1] (x axis) ; PUSHB[0] 0 ; MDAP[1] (round)
    e.exec_for_test(&[0x01, 0xB0, 0, 0x2F]).unwrap();
    let moved = e.test_glyph_point(0);
    assert_eq!(moved.cur_x, 64); // round(70) -> 64
    assert!(moved.touched_x);
}

#[test]
fn shpix_shifts_point_along_freedom() {
    let mut e = engine();
    let p = Point {
        cur_x: 100,
        cur_y: 0,
        org_x: 100,
        org_y: 0,
        on_curve: true,
        touched_x: false,
        touched_y: false,
    };
    e.set_test_glyph_zone(vec![p], vec![0]);
    // SVTCA[1] ; PUSHB[0] 0 (point) via loop=1 ; but SHPIX pops amount then points.
    // Program: SVTCA[1] ; PUSHB[1] 0 32 ; SHPIX
    // SHPIX pops amount(32) first? amount popped first, then point. Push point then amount.
    e.exec_for_test(&[0x01, 0xB1, 0, 32, 0x38]).unwrap();
    let moved = e.test_glyph_point(0);
    assert_eq!(moved.cur_x, 132); // 100 + 32
}

// ── Adversarial programs (typed errors, never a panic) ───────────────────────

#[test]
fn invalid_opcode_errs() {
    let mut e = engine();
    let r = e.exec_for_test(&[0x91]); // reserved / unimplemented GETVARIATION
    assert_eq!(r, Err(HintingError::InvalidOpcode(0x91)));
}

#[test]
fn stack_underflow_errs() {
    let mut e = engine();
    let r = e.exec_for_test(&[0x21]); // POP on empty stack
    assert_eq!(r, Err(HintingError::StackUnderflow));
}

#[test]
fn unbalanced_if_errs() {
    let mut e = engine();
    // PUSHB[0] 0 ; IF  (false branch, no EIF)
    let r = e.exec_for_test(&[0xB0, 0, 0x58]);
    assert_eq!(r, Err(HintingError::UnbalancedBlock));
}

#[test]
fn unbalanced_fdef_errs() {
    let mut e = engine();
    // PUSHB[0] 0 ; FDEF  (no ENDF)
    let r = e.exec_for_test(&[0xB0, 0, 0x2C]);
    assert_eq!(r, Err(HintingError::UnbalancedBlock));
}

#[test]
fn storage_out_of_bounds_errs() {
    let mut e = engine();
    // PUSHW[1] 9999 5 ; WS
    let r = e.exec_for_test(&[0xB9, 0x27, 0x0F, 0x00, 0x05, 0x42]);
    assert!(matches!(r, Err(HintingError::StorageOutOfBounds { .. })));
}

#[test]
fn cvt_out_of_bounds_errs() {
    let mut e = engine();
    // PUSHW[0] 9999 ; RCVT
    let r = e.exec_for_test(&[0xB8, 0x27, 0x0F, 0x45]);
    assert!(matches!(r, Err(HintingError::CvtOutOfBounds { .. })));
}

#[test]
fn infinite_loop_hits_budget() {
    let mut e = engine();
    // A self-jumping loop with balanced stack: PUSHW 0 ; POP ; PUSHW -7 ; JMPR.
    // JMPR at pc 7 jumps back to pc 0, forever, until the budget is exhausted.
    let code = [
        0xB8, 0x00, 0x00, // pc0: PUSHW[0] 0
        0x21, // pc3: POP
        0xB8, 0xFF, 0xF9, // pc4: PUSHW[0] -7
        0x1C, // pc7: JMPR
    ];
    let r = e.exec_for_test(&code);
    assert_eq!(r, Err(HintingError::ExecutionBudgetExceeded));
}

#[test]
fn call_undefined_function_errs() {
    let mut e = engine();
    // PUSHB[0] 5 ; CALL  (function 5 never defined)
    let r = e.exec_for_test(&[0xB0, 5, 0x2B]);
    assert_eq!(r, Err(HintingError::UndefinedFunction(5)));
}

#[test]
fn point_out_of_bounds_errs() {
    let mut e = engine();
    e.set_test_glyph_zone(vec![Point::default()], vec![0]);
    // SVTCA[1] ; PUSHB[0] 40 ; MDAP[1]  (point 40 does not exist)
    let r = e.exec_for_test(&[0x01, 0xB0, 40, 0x2F]);
    assert!(matches!(r, Err(HintingError::PointOutOfBounds { .. })));
}

// ── Whole-glyph fitting on the synthetic font ────────────────────────────────

#[test]
fn hint_triangle_glyph_is_finite_and_deterministic() {
    let mut e = engine();
    let g1 = e.hint_glyph(1).unwrap();
    assert_eq!(g1.points.len(), 3);
    assert_eq!(g1.contour_ends, vec![2]);
    for p in &g1.points {
        assert!(p.x.abs() < (1 << 20));
        assert!(p.y.abs() < (1 << 20));
    }
    assert!(g1.advance > 0);
    // Deterministic across repeated calls.
    let g2 = e.hint_glyph(1).unwrap();
    let xs1: Vec<_> = g1.points.iter().map(|p| (p.x, p.y)).collect();
    let xs2: Vec<_> = g2.points.iter().map(|p| (p.x, p.y)).collect();
    assert_eq!(xs1, xs2);
}

#[test]
fn hint_glyph_out_of_range_errs() {
    let mut e = engine();
    let r = e.hint_glyph(99);
    assert!(matches!(r, Err(HintingError::GlyphOutOfRange { .. })));
}

#[test]
fn to_outline_produces_move_and_close() {
    use oxifont_core::GlyphOutline;
    let mut e = engine();
    let g = e.hint_glyph(1).unwrap();
    let outline = g.to_outline();
    assert!(matches!(outline.first(), Some(GlyphOutline::MoveTo { .. })));
    assert!(matches!(outline.last(), Some(GlyphOutline::Close)));
}

// ── Composite point-matched components ──────────────────────────────────────

/// A composite glyph (gid 2) built from two copies of the triangle (gid 1):
/// - component 1 placed at XY offset (0, 0),
/// - component 2 placed by *point matching* — parent point 1 aligned with the
///   component's point 0. Parent point 1 is the triangle's (100, 10); the
///   component's point 0 is (10, 10), so the resolved delta is (90, 0).
fn composite_point_match_glyph() -> Vec<u8> {
    let mut g = Vec::new();
    g.extend_from_slice(&(-1i16).to_be_bytes()); // numberOfContours = -1 (composite)
    g.extend_from_slice(&10i16.to_be_bytes()); // xMin
    g.extend_from_slice(&10i16.to_be_bytes()); // yMin
    g.extend_from_slice(&190i16.to_be_bytes()); // xMax
    g.extend_from_slice(&100i16.to_be_bytes()); // yMax

    // Component 1: XY offset (0, 0), more components follow.
    g.extend_from_slice(&(0x0002u16 | 0x0020u16).to_be_bytes()); // ARGS_ARE_XY_VALUES | MORE_COMPONENTS
    g.extend_from_slice(&1u16.to_be_bytes()); // glyphIndex = 1
    g.push(0); // arg1 = dx = 0 (i8)
    g.push(0); // arg2 = dy = 0 (i8)

    // Component 2: point match (ARGS_ARE_XY_VALUES clear), byte args, last one.
    g.extend_from_slice(&0x0000u16.to_be_bytes()); // no flags
    g.extend_from_slice(&1u16.to_be_bytes()); // glyphIndex = 1
    g.push(1); // arg1 = parent point index 1
    g.push(0); // arg2 = component point index 0
    if g.len() % 2 != 0 {
        g.push(0);
    }
    g
}

fn build_composite_font() -> Vec<u8> {
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm; short loca.

    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&3u16.to_be_bytes()); // numGlyphs = 3
    maxp[16..18].copy_from_slice(&16u16.to_be_bytes());
    maxp[24..26].copy_from_slice(&256u16.to_be_bytes());

    let tri = triangle_glyph();
    let composite = composite_point_match_glyph();
    let mut glyf = Vec::new();
    glyf.extend_from_slice(&tri); // gid 1
    glyf.extend_from_slice(&composite); // gid 2

    // loca (short): gid0 empty at 0, gid1 at 0, gid2 at tri end, gid3 (end).
    let mut loca = Vec::new();
    loca.extend_from_slice(&0u16.to_be_bytes()); // gid0
    loca.extend_from_slice(&0u16.to_be_bytes()); // gid1
    loca.extend_from_slice(&((tri.len() / 2) as u16).to_be_bytes()); // gid2
    loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes()); // gid3 end

    let mut hhea = vec![0u8; 36];
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let mut hmtx = Vec::new();
    hmtx.extend_from_slice(&600u16.to_be_bytes());
    hmtx.extend_from_slice(&0i16.to_be_bytes());

    build_table_directory(&[
        (b"head", head),
        (b"maxp", maxp),
        (b"loca", loca),
        (b"glyf", glyf),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
    ])
}

#[test]
fn composite_point_match_resolves_offset() {
    use crate::font::FontProgram;
    let font = build_composite_font();
    let map = SfntTableMap::parse(&font).expect("composite font must parse");
    let prog = FontProgram::load(&map).expect("font program must load");

    let pts = prog
        .glyph_points(2)
        .expect("composite glyph must decode via point matching");

    // Two triangles = 6 points.
    assert_eq!(pts.num_points(), 6, "composite must have 6 points");
    // Component 1 sits at (10,10),(100,10),(50,100).
    assert_eq!((pts.xs[0], pts.ys[0]), (10, 10));
    assert_eq!((pts.xs[1], pts.ys[1]), (100, 10));
    // Component 2 point 0 must be aligned onto parent point 1 = (100, 10),
    // i.e. NOT the old (10, 10) zero-offset placement.
    assert_eq!(
        (pts.xs[3], pts.ys[3]),
        (100, 10),
        "point-matched component must be shifted by the resolved (90, 0) delta"
    );
    assert_eq!((pts.xs[4], pts.ys[4]), (190, 10));
    assert_eq!((pts.xs[5], pts.ys[5]), (140, 100));
}

#[test]
fn composite_point_match_out_of_range_errs() {
    use crate::font::FontProgram;
    // Rebuild the composite but with an out-of-range parent point index.
    let mut composite = composite_point_match_glyph();
    // The second component's arg1 byte is the second-to-last content byte.
    // Locate it: header(10) + comp1(6) + [flags(2)+gid(2)] = index 20.
    composite[20] = 200; // parent point index 200 — out of range (only 3 exist)
    let tri = triangle_glyph();
    let mut glyf = Vec::new();
    glyf.extend_from_slice(&tri);
    glyf.extend_from_slice(&composite);

    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&3u16.to_be_bytes());
    maxp[16..18].copy_from_slice(&16u16.to_be_bytes());
    maxp[24..26].copy_from_slice(&256u16.to_be_bytes());
    let mut loca = Vec::new();
    loca.extend_from_slice(&0u16.to_be_bytes());
    loca.extend_from_slice(&0u16.to_be_bytes());
    loca.extend_from_slice(&((tri.len() / 2) as u16).to_be_bytes());
    loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let mut hmtx = Vec::new();
    hmtx.extend_from_slice(&600u16.to_be_bytes());
    hmtx.extend_from_slice(&0i16.to_be_bytes());
    let font = build_table_directory(&[
        (b"head", head),
        (b"maxp", maxp),
        (b"loca", loca),
        (b"glyf", glyf),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
    ]);

    let map = SfntTableMap::parse(&font).expect("font must parse");
    let prog = FontProgram::load(&map).expect("font program must load");
    let r = prog.glyph_points(2);
    assert!(
        matches!(r, Err(HintingError::MalformedTable { .. })),
        "out-of-range point-match index must be a typed error, got {r:?}"
    );
}
