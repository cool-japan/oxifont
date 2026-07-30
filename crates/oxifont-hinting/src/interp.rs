//! The bytecode interpreter core: engine lifecycle, execution loop, and the
//! shared stack / zone / scaling helpers used by every opcode handler.

use std::collections::HashMap;
use std::rc::Rc;

use oxifont_core::sfnt::SfntTableMap;

use crate::error::HintingError;
use crate::font::FontProgram;
use crate::math::{mul_div, F26Dot6, Vector};
use crate::opcodes::{
    apply_jump, next_pc, scan_function, skip_past_eif, skip_to_else_or_eif, OP_CALL, OP_EIF,
    OP_ELSE, OP_ENDF, OP_FDEF, OP_IDEF, OP_IF, OP_JMPR, OP_JROF, OP_JROT, OP_LOOPCALL,
};
use crate::state::{GraphicsState, Point, Zone, ZonePointer};

/// The maximum function-call / instruction-definition recursion depth.
const MAX_CALL_DEPTH: u32 = 128;

/// The default per-run executed-instruction budget (loop / recursion guard).
const DEFAULT_BUDGET: u64 = 8_000_000;

/// A hardware upper bound on the operand stack, independent of `maxp`.
const STACK_HARD_CAP: usize = 1 << 20;

/// The number of phantom points appended to every glyph.
pub(crate) const PHANTOM_COUNT: usize = 4;

/// A fully grid-fitted glyph outline.
#[derive(Debug, Clone)]
pub struct HintedGlyph {
    /// Fitted contour points (phantom points excluded), coordinates in 26.6.
    pub points: Vec<HintedPoint>,
    /// Inclusive contour end-point indices into `points`.
    pub contour_ends: Vec<u16>,
    /// The fitted horizontal advance width in 26.6 pixels.
    pub advance: F26Dot6,
    /// The pixels-per-em this glyph was fitted at.
    pub ppem: u16,
}

/// A single fitted outline point.
#[derive(Debug, Clone, Copy)]
pub struct HintedPoint {
    /// Fitted x coordinate in 26.6 pixels.
    pub x: F26Dot6,
    /// Fitted y coordinate in 26.6 pixels.
    pub y: F26Dot6,
    /// True when the point lies on the curve.
    pub on_curve: bool,
}

impl HintedPoint {
    /// The x coordinate in floating-point pixels.
    #[inline]
    pub fn x_px(&self) -> f32 {
        self.x as f32 / 64.0
    }

    /// The y coordinate in floating-point pixels.
    #[inline]
    pub fn y_px(&self) -> f32 {
        self.y as f32 / 64.0
    }
}

impl HintedGlyph {
    /// The fitted horizontal advance in floating-point pixels.
    #[inline]
    pub fn advance_px(&self) -> f32 {
        self.advance as f32 / 64.0
    }

    /// Decompose the fitted points into [`GlyphOutline`](oxifont_core::GlyphOutline) path commands, in
    /// floating-point pixels (font Y-up orientation, matching the parser's
    /// unhinted outline surface).
    ///
    /// TrueType contours are quadratic; consecutive off-curve points imply an
    /// on-curve midpoint, and a contour that starts off-curve is anchored at a
    /// synthesized midpoint.
    pub fn to_outline(&self) -> Vec<oxifont_core::GlyphOutline> {
        let mut out = Vec::new();
        let mut start = 0usize;
        for &end in &self.contour_ends {
            let end = end as usize;
            if end >= self.points.len() {
                break;
            }
            emit_contour(&self.points[start..=end], &mut out);
            start = end + 1;
        }
        out
    }
}

/// One step in a contour walk: a point plus its on-curve flag, in pixels.
struct ContourStep {
    x: f32,
    y: f32,
    on: bool,
}

/// Emit the quadratic path commands for a single contour.
fn emit_contour(pts: &[HintedPoint], out: &mut Vec<oxifont_core::GlyphOutline>) {
    use oxifont_core::GlyphOutline;
    let n = pts.len();
    if n == 0 {
        return;
    }
    let px = |p: &HintedPoint| (p.x_px(), p.y_px());
    let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);

    // Build the walk sequence and the starting on-curve anchor.
    let mut seq: Vec<ContourStep> = Vec::with_capacity(n);
    let start = match pts.iter().position(|p| p.on_curve) {
        Some(s) => {
            for k in 1..n {
                let p = &pts[(s + k) % n];
                let (x, y) = px(p);
                seq.push(ContourStep {
                    x,
                    y,
                    on: p.on_curve,
                });
            }
            px(&pts[s])
        }
        None => {
            // All points off-curve: anchor at the midpoint of the first and last.
            for p in pts {
                let (x, y) = px(p);
                seq.push(ContourStep { x, y, on: false });
            }
            mid(px(&pts[0]), px(&pts[n - 1]))
        }
    };

    out.push(GlyphOutline::MoveTo {
        x: start.0,
        y: start.1,
    });
    let mut ctrl: Option<(f32, f32)> = None;
    for ContourStep { x, y, on } in seq {
        if on {
            match ctrl.take() {
                Some(c) => out.push(GlyphOutline::QuadTo {
                    cx: c.0,
                    cy: c.1,
                    x,
                    y,
                }),
                None => out.push(GlyphOutline::LineTo { x, y }),
            }
        } else if let Some(c) = ctrl {
            let m = mid(c, (x, y));
            out.push(GlyphOutline::QuadTo {
                cx: c.0,
                cy: c.1,
                x: m.0,
                y: m.1,
            });
            ctrl = Some((x, y));
        } else {
            ctrl = Some((x, y));
        }
    }
    if let Some(c) = ctrl {
        out.push(GlyphOutline::QuadTo {
            cx: c.0,
            cy: c.1,
            x: start.0,
            y: start.1,
        });
    }
    out.push(GlyphOutline::Close);
}

/// A reusable TrueType hinting engine for one font face.
///
/// Construction runs the font program (`fpgm`) once. Call [`set_ppem`] to run the
/// control-value program (`prep`) at a size, then [`hint_glyph`] for each glyph.
///
/// [`set_ppem`]: HintingEngine::set_ppem
/// [`hint_glyph`]: HintingEngine::hint_glyph
#[derive(Debug)]
pub struct HintingEngine {
    /// The parsed font tables.
    pub(crate) font: FontProgram,
    /// Function definitions collected from `fpgm` (and any later `FDEF`).
    pub(crate) functions: HashMap<u32, Rc<[u8]>>,
    /// Custom instruction definitions (`IDEF`), keyed by opcode.
    pub(crate) instruction_defs: HashMap<u8, Rc<[u8]>>,
    /// The persistent storage area.
    pub(crate) storage: Vec<i32>,
    /// The CVT, scaled to the current ppem (rebuilt by `set_ppem`).
    pub(crate) cvt: Vec<F26Dot6>,
    /// The graphics state captured after `prep` (per-glyph starting point).
    pub(crate) glyph_gs: GraphicsState,
    /// The twilight zone captured after `prep`.
    pub(crate) glyph_twilight: Zone,
    /// The current pixels-per-em.
    pub(crate) ppem: u16,

    // ── Per-run scratch (valid only during an execute call) ─────────────────
    /// The live operand stack.
    pub(crate) stack: Vec<i32>,
    /// The live graphics state.
    pub(crate) gs: GraphicsState,
    /// The live glyph zone (zone 1).
    pub(crate) glyph_zone: Zone,
    /// The live twilight zone (zone 0).
    pub(crate) twilight_zone: Zone,
    /// Remaining instruction budget.
    pub(crate) budget: u64,
    /// Current call/recursion depth.
    pub(crate) call_depth: u32,
    /// The effective operand-stack cap (from `maxp`, floored and hard-capped).
    pub(crate) max_stack: usize,
}

impl HintingEngine {
    /// Build an engine from a parsed SFNT directory and run its `fpgm`.
    pub fn new(map: &SfntTableMap<'_>) -> Result<Self, HintingError> {
        let font = FontProgram::load(map)?;
        let storage = vec![0i32; font.maxp.max_storage as usize];
        let twilight = Zone::twilight(font.maxp.max_twilight_points as usize);
        let max_stack = (font.maxp.max_stack_elements as usize).clamp(256, STACK_HARD_CAP);

        let mut engine = HintingEngine {
            font,
            functions: HashMap::new(),
            instruction_defs: HashMap::new(),
            storage,
            cvt: Vec::new(),
            glyph_gs: GraphicsState::default(),
            glyph_twilight: twilight.clone(),
            ppem: 0,
            stack: Vec::new(),
            gs: GraphicsState::default(),
            glyph_zone: Zone::default(),
            twilight_zone: twilight,
            budget: DEFAULT_BUDGET,
            call_depth: 0,
            max_stack,
        };

        // Run the font program once to collect function definitions.
        let fpgm = std::mem::take(&mut engine.font.fpgm);
        if !fpgm.is_empty() {
            engine.begin_run();
            engine.execute(&fpgm)?;
        }
        engine.font.fpgm = fpgm;
        Ok(engine)
    }

    /// Scale the CVT and run the `prep` program at `ppem` pixels-per-em.
    ///
    /// Must be called before [`hint_glyph`](Self::hint_glyph). Re-scales the CVT
    /// from font units each time so `prep` always starts from a clean size.
    pub fn set_ppem(&mut self, ppem: u16) -> Result<(), HintingError> {
        if ppem == 0 {
            return Err(HintingError::InvalidPpem);
        }
        self.ppem = ppem;
        // Scale CVT entries (font units → 26.6 pixels).
        self.cvt = self
            .font
            .cvt
            .iter()
            .map(|&raw| self.scale_funit(raw as i32))
            .collect();

        // Reset graphics state and twilight to defaults, then run prep.
        self.gs = GraphicsState::default();
        self.twilight_zone = Zone::twilight(self.font.maxp.max_twilight_points as usize);
        self.glyph_zone = Zone::default();

        let prep = std::mem::take(&mut self.font.prep);
        if !prep.is_empty() {
            self.begin_run();
            let r = self.execute(&prep);
            self.font.prep = prep;
            r?;
        } else {
            self.font.prep = prep;
        }

        // Capture the post-prep state for per-glyph runs.
        self.glyph_gs = self.gs.clone();
        self.glyph_twilight = self.twilight_zone.clone();
        Ok(())
    }

    /// Grid-fit glyph `gid` at the current ppem, returning its fitted outline.
    ///
    /// Runs the glyph's own instruction stream (if any) over its points plus the
    /// four phantom points. When `prep`/`INSTCTRL` disabled instructions, or the
    /// glyph has none, the scaled outline is returned unchanged.
    pub fn hint_glyph(&mut self, gid: u16) -> Result<HintedGlyph, HintingError> {
        if self.ppem == 0 {
            return Err(HintingError::InvalidPpem);
        }
        let glyph = self.font.glyph_points(gid)?;

        // Build the glyph zone: scaled contour points + 4 phantom points.
        let num = glyph.num_points();
        let mut points = Vec::with_capacity(num + PHANTOM_COUNT);
        for i in 0..num {
            let x = self.scale_funit(glyph.xs[i]);
            let y = self.scale_funit(glyph.ys[i]);
            points.push(Point {
                cur_x: x,
                cur_y: y,
                org_x: x,
                org_y: y,
                on_curve: glyph.on_curve[i],
                touched_x: false,
                touched_y: false,
            });
        }
        // Phantom points (font units → 26.6).
        let advance_units = glyph.advance as i32;
        let phantoms = [
            (0, 0),
            (advance_units, 0),
            (0, glyph.y_max as i32),
            (0, glyph.y_min as i32),
        ];
        for (px, py) in phantoms {
            let x = self.scale_funit(px);
            let y = self.scale_funit(py);
            points.push(Point {
                cur_x: x,
                cur_y: y,
                org_x: x,
                org_y: y,
                on_curve: true,
                touched_x: false,
                touched_y: false,
            });
        }

        self.glyph_zone = Zone {
            points,
            contour_ends: glyph.contour_ends.clone(),
        };
        // Restore per-glyph starting state captured after prep.
        self.gs = self.glyph_gs.clone();
        self.twilight_zone = self.glyph_twilight.clone();

        // Execute glyph instructions unless disabled.
        let instructions_disabled = self.gs.instruct_control & 0x01 != 0;
        if !instructions_disabled && !glyph.instructions.is_empty() {
            self.begin_run();
            self.execute(&glyph.instructions)?;
        }

        // Collect the fitted contour points (phantoms excluded).
        let out_points = self.glyph_zone.points[..num]
            .iter()
            .map(|p| HintedPoint {
                x: p.cur_x,
                y: p.cur_y,
                on_curve: p.on_curve,
            })
            .collect();
        let advance = if self.glyph_zone.points.len() >= num + 2 {
            self.glyph_zone.points[num + 1].cur_x - self.glyph_zone.points[num].cur_x
        } else {
            self.scale_funit(advance_units)
        };

        Ok(HintedGlyph {
            points: out_points,
            contour_ends: glyph.contour_ends,
            advance,
            ppem: self.ppem,
        })
    }

    /// The current pixels-per-em.
    #[inline]
    pub fn ppem(&self) -> u16 {
        self.ppem
    }

    /// Access the loaded font tables.
    #[inline]
    pub fn font(&self) -> &FontProgram {
        &self.font
    }

    // ── Scaling ─────────────────────────────────────────────────────────────

    /// Scale a font-unit value to 26.6 pixels at the current ppem.
    #[inline]
    pub(crate) fn scale_funit(&self, v: i32) -> F26Dot6 {
        mul_div(
            v as i64,
            self.ppem as i64 * 64,
            self.font.units_per_em as i64,
        ) as F26Dot6
    }

    // ── Run lifecycle ───────────────────────────────────────────────────────

    /// Reset the per-run scratch state (stack, budget, depth) before a program.
    pub(crate) fn begin_run(&mut self) {
        self.stack.clear();
        self.budget = DEFAULT_BUDGET;
        self.call_depth = 0;
    }

    /// Execute a bytecode program to completion.
    pub(crate) fn execute(&mut self, code: &[u8]) -> Result<(), HintingError> {
        self.call_depth += 1;
        if self.call_depth > MAX_CALL_DEPTH {
            self.call_depth -= 1;
            return Err(HintingError::CallDepthExceeded);
        }
        let result = self.execute_inner(code);
        self.call_depth -= 1;
        result
    }

    fn execute_inner(&mut self, code: &[u8]) -> Result<(), HintingError> {
        let mut pc = 0usize;
        while pc < code.len() {
            self.budget = self
                .budget
                .checked_sub(1)
                .ok_or(HintingError::ExecutionBudgetExceeded)?;
            let op = code[pc];
            match op {
                OP_IF => {
                    let cond = self.pop()?;
                    if cond != 0 {
                        pc = next_pc(code, pc)?;
                    } else {
                        pc = skip_to_else_or_eif(code, pc)?;
                    }
                }
                OP_ELSE => {
                    // Reached only at the end of a taken then-branch.
                    pc = skip_past_eif(code, pc)?;
                }
                OP_EIF => {
                    pc = next_pc(code, pc)?;
                }
                OP_JMPR => {
                    let offset = self.pop()?;
                    pc = apply_jump(pc, offset, code.len())?;
                }
                OP_JROT => {
                    let cond = self.pop()?;
                    let offset = self.pop()?;
                    pc = if cond != 0 {
                        apply_jump(pc, offset, code.len())?
                    } else {
                        next_pc(code, pc)?
                    };
                }
                OP_JROF => {
                    let cond = self.pop()?;
                    let offset = self.pop()?;
                    pc = if cond == 0 {
                        apply_jump(pc, offset, code.len())?
                    } else {
                        next_pc(code, pc)?
                    };
                }
                OP_FDEF => {
                    let n = self.pop()? as u32;
                    let (start, endf, end) = scan_function(code, pc)?;
                    self.functions.insert(n, Rc::from(&code[start..endf]));
                    pc = end;
                }
                OP_IDEF => {
                    let n = (self.pop()? & 0xFF) as u8;
                    let (start, endf, end) = scan_function(code, pc)?;
                    self.instruction_defs
                        .insert(n, Rc::from(&code[start..endf]));
                    pc = end;
                }
                OP_ENDF => {
                    // A bare ENDF outside a function body is unbalanced.
                    return Err(HintingError::UnbalancedBlock);
                }
                OP_CALL => {
                    let n = self.pop()? as u32;
                    self.call_function(n)?;
                    pc = next_pc(code, pc)?;
                }
                OP_LOOPCALL => {
                    let n = self.pop()? as u32;
                    let count = self.pop()?;
                    if count > 0 {
                        let body = self
                            .functions
                            .get(&n)
                            .cloned()
                            .ok_or(HintingError::UndefinedFunction(n))?;
                        for _ in 0..count {
                            self.budget = self
                                .budget
                                .checked_sub(1)
                                .ok_or(HintingError::ExecutionBudgetExceeded)?;
                            self.execute(&body)?;
                        }
                    }
                    pc = next_pc(code, pc)?;
                }
                _ => {
                    pc = self.exec_simple(code, pc, op)?;
                }
            }
        }
        Ok(())
    }

    fn call_function(&mut self, n: u32) -> Result<(), HintingError> {
        let body = self
            .functions
            .get(&n)
            .cloned()
            .ok_or(HintingError::UndefinedFunction(n))?;
        self.execute(&body)
    }

    // ── Stack helpers ───────────────────────────────────────────────────────

    /// Push a value, guarding the configured stack cap.
    #[inline]
    pub(crate) fn push(&mut self, v: i32) -> Result<(), HintingError> {
        if self.stack.len() >= self.max_stack {
            return Err(HintingError::StackOverflow);
        }
        self.stack.push(v);
        Ok(())
    }

    /// Pop a value, erroring on underflow.
    #[inline]
    pub(crate) fn pop(&mut self) -> Result<i32, HintingError> {
        self.stack.pop().ok_or(HintingError::StackUnderflow)
    }

    /// Pop a value interpreted as an unsigned index.
    #[inline]
    pub(crate) fn pop_uint(&mut self) -> Result<usize, HintingError> {
        Ok((self.pop()? as u32) as usize)
    }

    // ── Zone helpers ────────────────────────────────────────────────────────

    /// A shared reference to the zone selected by `zp`.
    #[inline]
    pub(crate) fn zone(&self, zp: ZonePointer) -> &Zone {
        match zp {
            ZonePointer::Twilight => &self.twilight_zone,
            ZonePointer::Glyph => &self.glyph_zone,
        }
    }

    /// A mutable reference to the zone selected by `zp`.
    #[inline]
    pub(crate) fn zone_mut(&mut self, zp: ZonePointer) -> &mut Zone {
        match zp {
            ZonePointer::Twilight => &mut self.twilight_zone,
            ZonePointer::Glyph => &mut self.glyph_zone,
        }
    }

    /// Read a point (copy) from `zp`, bounds-checked.
    #[inline]
    pub(crate) fn point(&self, zp: ZonePointer, idx: usize) -> Result<Point, HintingError> {
        let zone = self.zone(zp);
        zone.points
            .get(idx)
            .copied()
            .ok_or(HintingError::PointOutOfBounds {
                zone: zp.number(),
                index: idx,
                len: zone.points.len(),
            })
    }

    /// A mutable reference to a point in `zp`, bounds-checked.
    #[inline]
    pub(crate) fn point_mut(
        &mut self,
        zp: ZonePointer,
        idx: usize,
    ) -> Result<&mut Point, HintingError> {
        let num = zp.number();
        let zone = self.zone_mut(zp);
        let len = zone.points.len();
        zone.points
            .get_mut(idx)
            .ok_or(HintingError::PointOutOfBounds {
                zone: num,
                index: idx,
                len,
            })
    }

    /// The projection of a point's current position onto the projection vector.
    #[inline]
    pub(crate) fn project_cur(&self, zp: ZonePointer, idx: usize) -> Result<F26Dot6, HintingError> {
        let p = self.point(zp, idx)?;
        Ok(self.gs.projection.project(p.cur_x, p.cur_y))
    }

    /// The dual projection of a point's original position.
    #[inline]
    pub(crate) fn dual_project_org(
        &self,
        zp: ZonePointer,
        idx: usize,
    ) -> Result<F26Dot6, HintingError> {
        let p = self.point(zp, idx)?;
        Ok(self.gs.dual_projection.project(p.org_x, p.org_y))
    }

    /// The 2.14 dot product of freedom and projection vectors, clamped away from
    /// zero so movement math never divides by a degenerate value.
    #[inline]
    pub(crate) fn fdotp(&self) -> i32 {
        let d = self.gs.freedom.dot(&self.gs.projection);
        if d == 0 {
            crate::math::ONE_2DOT14
        } else {
            d
        }
    }

    /// Move point `idx` in `zp` by `distance` (26.6) along the freedom vector.
    pub(crate) fn move_point(
        &mut self,
        zp: ZonePointer,
        idx: usize,
        distance: F26Dot6,
    ) -> Result<(), HintingError> {
        let freedom = self.gs.freedom;
        let fdotp = self.fdotp();
        let p = self.point_mut(zp, idx)?;
        if freedom.x != 0 {
            p.cur_x += mul_div(distance as i64, freedom.x as i64, fdotp as i64) as F26Dot6;
            p.touched_x = true;
        }
        if freedom.y != 0 {
            p.cur_y += mul_div(distance as i64, freedom.y as i64, fdotp as i64) as F26Dot6;
            p.touched_y = true;
        }
        Ok(())
    }

    /// Set both freedom and projection vectors (used by `SVTCA` etc.).
    #[inline]
    pub(crate) fn set_both_vectors(&mut self, v: Vector) {
        self.gs.projection = v;
        self.gs.dual_projection = v;
        self.gs.freedom = v;
    }

    /// Read and consume the loop counter, resetting it to `1`.
    ///
    /// The count is clamped to a non-negative, bounded value so an adversarial
    /// `SLOOP` cannot force an unbounded pop loop (the instruction budget also
    /// backstops this).
    pub(crate) fn take_loop_counter(&mut self) -> usize {
        let n = self.gs.loop_counter;
        self.gs.loop_counter = 1;
        n.clamp(0, u16::MAX as i32) as usize
    }
}
