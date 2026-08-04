//! Loading of the hinting-relevant SFNT tables and decoding of raw glyph points.
//!
//! This module turns a [`SfntTableMap`] into a [`FontProgram`] holding the font
//! (`fpgm`) and control-value (`prep`) bytecode, the scaled-at-runtime CVT, the
//! `maxp` resource limits, and enough of `head`/`hhea`/`hmtx`/`loca`/`glyf` to
//! decode a glyph's outline points and its horizontal metrics.

use oxifont_core::sfnt::SfntTableMap;

use crate::error::HintingError;

// Simple-glyph flag bits.
const ON_CURVE: u8 = 0x01;
const X_SHORT: u8 = 0x02;
const Y_SHORT: u8 = 0x04;
const REPEAT_FLAG: u8 = 0x08;
const X_SAME_OR_POSITIVE: u8 = 0x10;
const Y_SAME_OR_POSITIVE: u8 = 0x20;

// Composite-glyph flag bits.
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

/// Maximum composite-component nesting depth (guards adversarial recursion).
const MAX_COMPOSITE_DEPTH: u8 = 8;

/// The `maxp`-derived resource limits used to size the VM's fixed arrays.
#[derive(Debug, Clone, Copy)]
pub struct MaxProfile {
    /// The number of glyphs in the font.
    pub num_glyphs: u16,
    /// The maximum number of points in the twilight zone (zone 0).
    pub max_twilight_points: u16,
    /// The size of the storage area.
    pub max_storage: u16,
    /// The number of function definitions.
    pub max_function_defs: u16,
    /// The maximum operand-stack depth.
    pub max_stack_elements: u16,
}

/// The decoded outline of a single glyph, in **font units** (unscaled).
#[derive(Debug, Clone, Default)]
pub struct GlyphPoints {
    /// The x coordinates in font units.
    pub xs: Vec<i32>,
    /// The y coordinates in font units.
    pub ys: Vec<i32>,
    /// Per-point on-curve flags.
    pub on_curve: Vec<bool>,
    /// Inclusive contour end-point indices.
    pub contour_ends: Vec<u16>,
    /// The glyph's own instruction stream (empty for unhinted / composite w/o).
    pub instructions: Vec<u8>,
    /// The horizontal advance width in font units.
    pub advance: u16,
    /// The left side bearing in font units.
    pub lsb: i16,
    /// The glyph bounding box `yMax` in font units (for the top phantom point).
    pub y_max: i16,
    /// The glyph bounding box `yMin` in font units (for the bottom phantom point).
    pub y_min: i16,
}

impl GlyphPoints {
    /// The number of contour points (excluding phantom points).
    #[inline]
    pub fn num_points(&self) -> usize {
        self.xs.len()
    }
}

/// The parsed hinting context of a single font face.
#[derive(Debug)]
pub struct FontProgram {
    /// Units per em from `head`.
    pub units_per_em: u16,
    /// `head.indexToLocFormat` (0 = short, 1 = long).
    loca_long: bool,
    /// The `maxp` resource limits.
    pub maxp: MaxProfile,
    /// The font program (`fpgm`) bytecode.
    pub fpgm: Vec<u8>,
    /// The control-value program (`prep`) bytecode.
    pub prep: Vec<u8>,
    /// The raw CVT entries in font units (FUnits).
    pub cvt: Vec<i16>,
    /// The `glyf` table bytes.
    glyf: Vec<u8>,
    /// The `loca` table bytes.
    loca: Vec<u8>,
    /// The number of `hmtx` long metrics (`hhea.numberOfHMetrics`).
    num_h_metrics: u16,
    /// The `hmtx` table bytes.
    hmtx: Vec<u8>,
}

impl FontProgram {
    /// Load the hinting tables from a parsed SFNT directory.
    ///
    /// Requires `head`, `maxp`, `glyf`, and `loca`; `fpgm`, `prep`, `cvt `,
    /// `hhea`, and `hmtx` are optional (a glyf font with none of them is simply
    /// unhinted and yields identity grid-fitting).
    pub fn load(map: &SfntTableMap<'_>) -> Result<Self, HintingError> {
        let head = map
            .table(b"head")
            .ok_or(HintingError::MissingTable(*b"head"))?;
        if head.len() < 54 {
            return Err(HintingError::MalformedTable {
                tag: *b"head",
                reason: "shorter than 54 bytes",
            });
        }
        let units_per_em = be_u16(head, 18);
        if units_per_em == 0 {
            return Err(HintingError::MalformedTable {
                tag: *b"head",
                reason: "unitsPerEm is zero",
            });
        }
        let loca_long = be_i16(head, 50) != 0;

        let maxp_tbl = map
            .table(b"maxp")
            .ok_or(HintingError::MissingTable(*b"maxp"))?;
        if maxp_tbl.len() < 6 {
            return Err(HintingError::MalformedTable {
                tag: *b"maxp",
                reason: "shorter than 6 bytes",
            });
        }
        let num_glyphs = be_u16(maxp_tbl, 4);
        let maxp = if be_u32(maxp_tbl, 0) == 0x0001_0000 && maxp_tbl.len() >= 32 {
            MaxProfile {
                num_glyphs,
                max_twilight_points: be_u16(maxp_tbl, 16),
                max_storage: be_u16(maxp_tbl, 18),
                max_function_defs: be_u16(maxp_tbl, 20),
                max_stack_elements: be_u16(maxp_tbl, 24),
            }
        } else {
            MaxProfile {
                num_glyphs,
                max_twilight_points: 0,
                max_storage: 0,
                max_function_defs: 0,
                max_stack_elements: 0,
            }
        };

        let glyf = map
            .table(b"glyf")
            .ok_or(HintingError::MissingTable(*b"glyf"))?
            .to_vec();
        let loca = map
            .table(b"loca")
            .ok_or(HintingError::MissingTable(*b"loca"))?
            .to_vec();

        let fpgm = map.table(b"fpgm").map(<[u8]>::to_vec).unwrap_or_default();
        let prep = map.table(b"prep").map(<[u8]>::to_vec).unwrap_or_default();

        let cvt = match map.table(b"cvt ") {
            Some(bytes) => bytes
                .chunks_exact(2)
                .map(|c| i16::from_be_bytes([c[0], c[1]]))
                .collect(),
            None => Vec::new(),
        };

        let num_h_metrics = match map.table(b"hhea") {
            Some(hhea) if hhea.len() >= 36 => be_u16(hhea, 34),
            _ => 0,
        };
        let hmtx = map.table(b"hmtx").map(<[u8]>::to_vec).unwrap_or_default();

        Ok(FontProgram {
            units_per_em,
            loca_long,
            maxp,
            fpgm,
            prep,
            cvt,
            glyf,
            loca,
            num_h_metrics,
            hmtx,
        })
    }

    /// Look up the `(advance, lsb)` for a glyph from `hmtx`.
    fn h_metrics(&self, gid: u16) -> (u16, i16) {
        if self.num_h_metrics == 0 || self.hmtx.is_empty() {
            return (0, 0);
        }
        let last = self.num_h_metrics.saturating_sub(1);
        if gid < self.num_h_metrics {
            let off = gid as usize * 4;
            if off + 4 <= self.hmtx.len() {
                return (be_u16(&self.hmtx, off), be_i16(&self.hmtx, off + 2));
            }
        } else {
            // Monospaced tail: advance is the last long metric's advance;
            // lsb is read from the trailing lsb-only array.
            let adv_off = last as usize * 4;
            let advance = if adv_off + 2 <= self.hmtx.len() {
                be_u16(&self.hmtx, adv_off)
            } else {
                0
            };
            let lsb_off = self.num_h_metrics as usize * 4 + (gid - self.num_h_metrics) as usize * 2;
            let lsb = if lsb_off + 2 <= self.hmtx.len() {
                be_i16(&self.hmtx, lsb_off)
            } else {
                0
            };
            return (advance, lsb);
        }
        (0, 0)
    }

    /// Read a `(start, end)` byte range into `glyf` for glyph `gid`.
    fn loca_range(&self, gid: u16) -> Result<(usize, usize), HintingError> {
        let idx = gid as usize;
        let (start, end) = if self.loca_long {
            let s = idx * 4;
            if s + 8 > self.loca.len() {
                return Err(HintingError::GlyphOutOfRange {
                    gid,
                    count: self.maxp.num_glyphs,
                });
            }
            (
                be_u32(&self.loca, s) as usize,
                be_u32(&self.loca, s + 4) as usize,
            )
        } else {
            let s = idx * 2;
            if s + 4 > self.loca.len() {
                return Err(HintingError::GlyphOutOfRange {
                    gid,
                    count: self.maxp.num_glyphs,
                });
            }
            (
                be_u16(&self.loca, s) as usize * 2,
                be_u16(&self.loca, s + 2) as usize * 2,
            )
        };
        if end < start || end > self.glyf.len() {
            return Err(HintingError::MalformedTable {
                tag: *b"loca",
                reason: "glyph range out of bounds",
            });
        }
        Ok((start, end))
    }

    /// Decode glyph `gid` into unscaled outline points, resolving composites.
    pub fn glyph_points(&self, gid: u16) -> Result<GlyphPoints, HintingError> {
        if gid >= self.maxp.num_glyphs {
            return Err(HintingError::GlyphOutOfRange {
                gid,
                count: self.maxp.num_glyphs,
            });
        }
        let (advance, lsb) = self.h_metrics(gid);
        let mut pts = self.decode_glyph(gid, 0)?;
        pts.advance = advance;
        pts.lsb = lsb;
        Ok(pts)
    }

    fn decode_glyph(&self, gid: u16, depth: u8) -> Result<GlyphPoints, HintingError> {
        if depth > MAX_COMPOSITE_DEPTH {
            return Err(HintingError::CompositeTooDeep);
        }
        let (start, end) = self.loca_range(gid)?;
        // An empty loca range denotes a glyph with no outline (e.g. space).
        if end == start {
            return Ok(GlyphPoints::default());
        }
        let data = &self.glyf[start..end];
        if data.len() < 10 {
            return Err(HintingError::MalformedTable {
                tag: *b"glyf",
                reason: "glyph header truncated",
            });
        }
        let num_contours = be_i16(data, 0);
        let y_min = be_i16(data, 4);
        let y_max = be_i16(data, 8);
        if num_contours >= 0 {
            let mut g = decode_simple_glyph(data, num_contours as usize)?;
            g.y_min = y_min;
            g.y_max = y_max;
            Ok(g)
        } else {
            let mut g = self.decode_composite_glyph(data, depth)?;
            g.y_min = y_min;
            g.y_max = y_max;
            Ok(g)
        }
    }

    fn decode_composite_glyph(&self, data: &[u8], depth: u8) -> Result<GlyphPoints, HintingError> {
        let mut out = GlyphPoints::default();
        let mut pos = 10usize;
        let mut own_instructions: Vec<u8> = Vec::new();
        loop {
            if pos + 4 > data.len() {
                return Err(HintingError::MalformedTable {
                    tag: *b"glyf",
                    reason: "composite component truncated",
                });
            }
            let flags = be_u16(data, pos);
            let component_gid = be_u16(data, pos + 2);
            pos += 4;

            let (arg1, arg2) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                if pos + 4 > data.len() {
                    return Err(HintingError::MalformedTable {
                        tag: *b"glyf",
                        reason: "composite args truncated",
                    });
                }
                let a = be_i16(data, pos) as i32;
                let b = be_i16(data, pos + 2) as i32;
                pos += 4;
                (a, b)
            } else {
                if pos + 2 > data.len() {
                    return Err(HintingError::MalformedTable {
                        tag: *b"glyf",
                        reason: "composite args truncated",
                    });
                }
                let a = data[pos] as i8 as i32;
                let b = data[pos + 1] as i8 as i32;
                pos += 2;
                (a, b)
            };

            // Transform matrix in 2.14 fixed point.
            let (mut xx, mut xy, mut yx, mut yy) = (0x4000i32, 0i32, 0i32, 0x4000i32);
            if flags & WE_HAVE_A_SCALE != 0 {
                if pos + 2 > data.len() {
                    return Err(component_trunc());
                }
                let s = be_i16(data, pos) as i32;
                xx = s;
                yy = s;
                pos += 2;
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                if pos + 4 > data.len() {
                    return Err(component_trunc());
                }
                xx = be_i16(data, pos) as i32;
                yy = be_i16(data, pos + 2) as i32;
                pos += 4;
            } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                if pos + 8 > data.len() {
                    return Err(component_trunc());
                }
                xx = be_i16(data, pos) as i32;
                xy = be_i16(data, pos + 2) as i32;
                yx = be_i16(data, pos + 4) as i32;
                yy = be_i16(data, pos + 6) as i32;
                pos += 8;
            }

            let comp = self.decode_glyph(component_gid, depth + 1)?;
            let base = out.xs.len();

            // Resolve the component placement offset.
            //
            // With ARGS_ARE_XY_VALUES set, arg1/arg2 are a direct (x, y) delta
            // in font units. Otherwise they are point indices: point `arg1` of
            // the composite assembled so far must be aligned with point `arg2`
            // of the incoming component, *after* the component's 2x2 transform
            // is applied (matching FreeType's behaviour). Out-of-range indices
            // are a malformed glyph rather than a silent zero offset.
            let (dx, dy) = if flags & ARGS_ARE_XY_VALUES != 0 {
                (arg1, arg2)
            } else {
                let (parent_idx, comp_idx) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                    (arg1 as u16 as usize, arg2 as u16 as usize)
                } else {
                    (arg1 as u8 as usize, arg2 as u8 as usize)
                };
                if parent_idx >= base || comp_idx >= comp.num_points() {
                    return Err(HintingError::MalformedTable {
                        tag: *b"glyf",
                        reason: "composite point-match index out of range",
                    });
                }
                let cx = comp.xs[comp_idx];
                let cy = comp.ys[comp_idx];
                let tx = mul_2dot14(cx, xx) + mul_2dot14(cy, yx);
                let ty = mul_2dot14(cx, xy) + mul_2dot14(cy, yy);
                (out.xs[parent_idx] - tx, out.ys[parent_idx] - ty)
            };
            for i in 0..comp.num_points() {
                let x = comp.xs[i];
                let y = comp.ys[i];
                let nx = (mul_2dot14(x, xx) + mul_2dot14(y, yx)) + dx;
                let ny = (mul_2dot14(x, xy) + mul_2dot14(y, yy)) + dy;
                out.xs.push(nx);
                out.ys.push(ny);
                out.on_curve.push(comp.on_curve[i]);
            }
            for &e in &comp.contour_ends {
                out.contour_ends.push(e + base as u16);
            }

            if flags & WE_HAVE_INSTRUCTIONS != 0 {
                // The composite's own instructions follow the last component.
                if pos + 2 <= data.len() {
                    let ins_len = be_u16(data, pos) as usize;
                    pos += 2;
                    if pos + ins_len <= data.len() {
                        own_instructions = data[pos..pos + ins_len].to_vec();
                    }
                    pos += ins_len;
                }
            }

            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        out.instructions = own_instructions;
        Ok(out)
    }
}

fn component_trunc() -> HintingError {
    HintingError::MalformedTable {
        tag: *b"glyf",
        reason: "composite transform truncated",
    }
}

/// Multiply a font-unit coordinate by a 2.14 matrix entry, rounding.
#[inline]
fn mul_2dot14(coord: i32, factor: i32) -> i32 {
    ((coord as i64 * factor as i64 + 0x2000) >> 14) as i32
}

/// Decode a simple (non-composite) glyph body into points.
fn decode_simple_glyph(data: &[u8], num_contours: usize) -> Result<GlyphPoints, HintingError> {
    let trunc = || HintingError::MalformedTable {
        tag: *b"glyf",
        reason: "simple glyph truncated",
    };
    // endPtsOfContours[num_contours]
    let ends_end = 10 + num_contours * 2;
    if ends_end + 2 > data.len() {
        return Err(trunc());
    }
    let mut contour_ends = Vec::with_capacity(num_contours);
    let mut num_points = 0usize;
    for i in 0..num_contours {
        let e = be_u16(data, 10 + i * 2);
        contour_ends.push(e);
        num_points = e as usize + 1;
    }
    if num_contours == 0 {
        num_points = 0;
    }

    let instruction_length = be_u16(data, ends_end) as usize;
    let ins_start = ends_end + 2;
    let flags_start = ins_start + instruction_length;
    if flags_start > data.len() {
        return Err(trunc());
    }
    let instructions = data[ins_start..flags_start].to_vec();

    // Decode flags (with repeat runs).
    let mut flags = Vec::with_capacity(num_points);
    let mut pos = flags_start;
    while flags.len() < num_points {
        if pos >= data.len() {
            return Err(trunc());
        }
        let flag = data[pos];
        pos += 1;
        flags.push(flag);
        if flag & REPEAT_FLAG != 0 {
            if pos >= data.len() {
                return Err(trunc());
            }
            let mut repeat = data[pos];
            pos += 1;
            while repeat > 0 && flags.len() < num_points {
                flags.push(flag);
                repeat -= 1;
            }
        }
    }

    // Decode x coordinates (deltas).
    let mut xs = Vec::with_capacity(num_points);
    let mut x = 0i32;
    for &flag in &flags {
        if flag & X_SHORT != 0 {
            if pos >= data.len() {
                return Err(trunc());
            }
            let d = data[pos] as i32;
            pos += 1;
            x += if flag & X_SAME_OR_POSITIVE != 0 {
                d
            } else {
                -d
            };
        } else if flag & X_SAME_OR_POSITIVE == 0 {
            if pos + 2 > data.len() {
                return Err(trunc());
            }
            x += be_i16(data, pos) as i32;
            pos += 2;
        }
        xs.push(x);
    }

    // Decode y coordinates (deltas).
    let mut ys = Vec::with_capacity(num_points);
    let mut y = 0i32;
    for &flag in &flags {
        if flag & Y_SHORT != 0 {
            if pos >= data.len() {
                return Err(trunc());
            }
            let d = data[pos] as i32;
            pos += 1;
            y += if flag & Y_SAME_OR_POSITIVE != 0 {
                d
            } else {
                -d
            };
        } else if flag & Y_SAME_OR_POSITIVE == 0 {
            if pos + 2 > data.len() {
                return Err(trunc());
            }
            y += be_i16(data, pos) as i32;
            pos += 2;
        }
        ys.push(y);
    }

    let on_curve = flags.iter().map(|f| f & ON_CURVE != 0).collect();

    Ok(GlyphPoints {
        xs,
        ys,
        on_curve,
        contour_ends,
        instructions,
        advance: 0,
        lsb: 0,
        y_max: 0,
        y_min: 0,
    })
}

#[inline]
fn be_u16(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}

#[inline]
fn be_i16(b: &[u8], off: usize) -> i16 {
    i16::from_be_bytes([b[off], b[off + 1]])
}

#[inline]
fn be_u32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
