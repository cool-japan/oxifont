//! `gvar` tuple variation store: binary decoding and region scalars.
//!
//! The two encodings this module implements — packed point numbers and packed
//! deltas — both use run-length control bytes whose runs may overrun the
//! requested element count. Runs are therefore always consumed **whole** and the
//! result truncated afterwards: the X and Y delta arrays share one cursor, so
//! stopping mid-run would desynchronise the Y read.

use crate::gvar::{parse_header, parse_offsets};
use crate::tables::{get_i16, get_u16, SubsetError};

use super::outline::iup_contour;

/// `tupleVariationCount` bit: a shared packed point-number list precedes the
/// per-tuple data.
const SHARED_POINT_NUMBERS: u16 = 0x8000;
/// `tupleVariationCount` low bits: the number of tuple variation headers.
const TUPLE_COUNT_MASK: u16 = 0x0FFF;

/// `tupleIndex` bit: the peak tuple follows the header inline.
const EMBEDDED_PEAK_TUPLE: u16 = 0x8000;
/// `tupleIndex` bit: intermediate start/end tuples follow the peak.
const INTERMEDIATE_REGION: u16 = 0x4000;
/// `tupleIndex` bit: this tuple carries its own packed point-number list.
const PRIVATE_POINT_NUMBERS: u16 = 0x2000;
/// `tupleIndex` low bits: index into the shared tuple array.
const TUPLE_INDEX_MASK: u16 = 0x0FFF;

/// A packed point-number list, decoded.
enum PointSet {
    /// `count == 0`: every point of the glyph, phantom points included.
    All,
    /// An explicit, cumulative-decoded list of point numbers.
    Explicit(Vec<u16>),
}

/// Sequential reader over a byte slice; every read is bounds-checked.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn read_i8(&mut self) -> Option<i8> {
        self.read_u8().map(|b| b as i8)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let v = get_u16(self.data, self.pos)?;
        self.pos += 2;
        Some(v)
    }

    fn read_i16(&mut self) -> Option<i16> {
        let v = get_i16(self.data, self.pos)?;
        self.pos += 2;
        Some(v)
    }
}

/// Decode a packed point-number list.
///
/// `count == 0` selects *all* points and consumes no further bytes — the
/// dominant encoding in shipping fonts, and the trap that turns "all points"
/// into "no points" if read as an empty list.
fn read_packed_points(cur: &mut Cursor<'_>) -> Option<PointSet> {
    let b0 = cur.read_u8()?;
    let count = if b0 & 0x80 != 0 {
        (usize::from(b0 & 0x7F) << 8) | usize::from(cur.read_u8()?)
    } else {
        usize::from(b0)
    };
    if count == 0 {
        return Some(PointSet::All);
    }
    // Every point costs at least one payload byte, so the remaining length is a
    // hard upper bound on a legitimate `count`; this is what keeps a hostile
    // 32 767 from reserving memory the table cannot possibly justify.
    if count > cur.remaining() {
        return None;
    }
    let mut pts: Vec<u16> = Vec::with_capacity(count);
    let mut acc: u16 = 0;
    while pts.len() < count {
        let ctrl = cur.read_u8()?;
        let words = ctrl & 0x80 != 0;
        let run_len = usize::from(ctrl & 0x7F) + 1;
        for _ in 0..run_len {
            let delta = if words {
                cur.read_u16()?
            } else {
                u16::from(cur.read_u8()?)
            };
            acc = acc.wrapping_add(delta);
            pts.push(acc);
        }
    }
    pts.truncate(count);
    Some(PointSet::Explicit(pts))
}

/// Decode `n` packed deltas into `out`.
///
/// `DELTAS_ARE_ZERO` (`0x80`) consumes no payload and is tested before
/// `DELTAS_ARE_WORDS` (`0x40`); the combination is undefined and every
/// implementation lets `0x80` win.
fn read_packed_deltas(cur: &mut Cursor<'_>, n: usize, out: &mut Vec<i32>) -> Option<()> {
    out.clear();
    out.reserve(n);
    while out.len() < n {
        let ctrl = cur.read_u8()?;
        let run_len = usize::from(ctrl & 0x3F) + 1;
        if ctrl & 0x80 != 0 {
            out.resize(out.len() + run_len, 0);
        } else if ctrl & 0x40 != 0 {
            for _ in 0..run_len {
                out.push(i32::from(cur.read_i16()?));
            }
        } else {
            for _ in 0..run_len {
                out.push(i32::from(cur.read_i8()?));
            }
        }
    }
    out.truncate(n);
    Some(())
}

/// The region scalar for one tuple at `coords`.
///
/// The `instanceCoord == 0` test comes **before** the malformed-region guards,
/// matching the OpenType pseudo-code and skrifa; the two orderings differ only
/// on regions that straddle zero with a non-zero peak, which no conforming font
/// emits.
fn region_scalar(peak: &[i16], start: Option<&[i16]>, end: Option<&[i16]>, coords: &[i16]) -> f64 {
    let mut s = 1.0f64;
    for (a, &peak_raw) in peak.iter().enumerate() {
        let p = f64::from(peak_raw) / 16384.0;
        if p == 0.0 {
            continue;
        }
        let c = f64::from(coords.get(a).copied().unwrap_or(0)) / 16384.0;
        if c == p {
            continue;
        }
        if c == 0.0 {
            return 0.0;
        }
        let (lo, hi) = match (start, end) {
            (Some(st), Some(en)) => (
                f64::from(st.get(a).copied().unwrap_or(0)) / 16384.0,
                f64::from(en.get(a).copied().unwrap_or(0)) / 16384.0,
            ),
            _ => (p.min(0.0), p.max(0.0)),
        };
        if lo > p || p > hi {
            continue;
        }
        if lo < 0.0 && hi > 0.0 {
            continue;
        }
        if c < lo || c > hi {
            return 0.0;
        }
        s *= if c < p {
            (c - lo) / (p - lo)
        } else {
            (hi - c) / (hi - p)
        };
    }
    s
}

/// Reusable per-glyph buffers, so a 17 936-glyph face does not allocate four
/// vectors per tuple.
#[derive(Default)]
pub(crate) struct TupleScratch {
    dx: Vec<i32>,
    dy: Vec<i32>,
    sparse: Vec<Option<(f64, f64)>>,
    inferred: Vec<(f64, f64)>,
}

/// A parsed `gvar` table, ready to be evaluated glyph by glyph.
pub(crate) struct GvarStore<'a> {
    table: &'a [u8],
    axis_count: usize,
    shared_tuple_count: usize,
    shared_tuples_offset: usize,
    offsets: Vec<usize>,
}

impl<'a> GvarStore<'a> {
    /// Parse and structurally validate a `gvar` table.
    ///
    /// Everything checked here is fatal by design: a broken header or offset
    /// array poisons every glyph, so degrading per-glyph would silently emit a
    /// font at the default location.
    ///
    /// # Errors
    /// [`SubsetError::InvalidFont`] for a bad header, an `axisCount` that
    /// disagrees with `fvar`, a non-monotonic offset array, or an offset that
    /// leaves the table.
    pub(crate) fn parse(table: &'a [u8], fvar_axis_count: usize) -> Result<Self, SubsetError> {
        let bad = |what: &str| SubsetError::InvalidFont(format!("gvar: {what}"));

        let hdr = parse_header(table).ok_or_else(|| bad("bad header"))?;
        if usize::from(hdr.axis_count) != fvar_axis_count {
            return Err(bad("axisCount disagrees with fvar"));
        }
        let offsets = parse_offsets(table, &hdr).ok_or_else(|| bad("bad offset array"))?;

        // `off[g+1] == off[g]` means "no data"; `off[g+1] < off[g]` is breakage.
        let mut prev = 0usize;
        for (i, &off) in offsets.iter().enumerate() {
            if off > table.len() {
                return Err(bad("glyph variation data offset leaves the table"));
            }
            if i > 0 && off < prev {
                return Err(bad("glyph variation data offsets are not monotonic"));
            }
            prev = off;
        }

        let shared_tuples_size = usize::from(hdr.axis_count)
            .checked_mul(usize::from(hdr.shared_tuple_count))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| bad("shared tuple region size overflows"))?;
        if shared_tuples_size > 0 {
            let end = hdr
                .shared_tuples_offset
                .checked_add(shared_tuples_size)
                .ok_or_else(|| bad("shared tuple region overflows"))?;
            if end > table.len() {
                return Err(bad("shared tuple region leaves the table"));
            }
        }

        Ok(GvarStore {
            table,
            axis_count: usize::from(hdr.axis_count),
            shared_tuple_count: usize::from(hdr.shared_tuple_count),
            shared_tuples_offset: hdr.shared_tuples_offset,
            offsets,
        })
    }

    /// The shared tuple at `index`, as `axisCount` F2Dot14 values.
    fn shared_tuple(&self, index: usize) -> Option<Vec<i16>> {
        if index >= self.shared_tuple_count {
            return None;
        }
        let base = self
            .shared_tuples_offset
            .checked_add(index.checked_mul(self.axis_count.checked_mul(2)?)?)?;
        read_tuple(self.table, base, self.axis_count)
    }

    /// Accumulate glyph `gid`'s scaled variation deltas into `acc`.
    ///
    /// `default_pts` is the glyph's **default** outline (real points followed by
    /// the four phantoms) — IUP interpolates against it, never against the
    /// partially accumulated result. `contours` are the IUP spans: one per real
    /// contour, then four single-point spans for the phantoms.
    ///
    /// Returns `false` when this glyph's variation data was malformed. The
    /// caller then keeps the glyph's default outline: one glyph at the wrong
    /// weight is a far better outcome than failing the whole instance.
    pub(crate) fn accumulate(
        &self,
        gid: u16,
        coords: &[i16],
        default_pts: &[(f64, f64)],
        contours: &[(usize, usize)],
        acc: &mut [(f64, f64)],
        scratch: &mut TupleScratch,
    ) -> bool {
        self.accumulate_inner(gid, coords, default_pts, contours, acc, scratch)
            .is_some()
    }

    fn accumulate_inner(
        &self,
        gid: u16,
        coords: &[i16],
        default_pts: &[(f64, f64)],
        contours: &[(usize, usize)],
        acc: &mut [(f64, f64)],
        scratch: &mut TupleScratch,
    ) -> Option<()> {
        let idx = usize::from(gid);
        // A `glyphCount` below `maxp.numGlyphs` is legal: the tail simply has no
        // variation data.
        if idx + 1 >= self.offsets.len() {
            return Some(());
        }
        let start = self.offsets[idx];
        let end = self.offsets[idx + 1];
        if start >= end {
            return Some(());
        }
        let block = self.table.get(start..end)?;

        let tvc = get_u16(block, 0)?;
        let data_offset = usize::from(get_u16(block, 2)?);
        let tuple_count = usize::from(tvc & TUPLE_COUNT_MASK);
        if data_offset > block.len() {
            return None;
        }

        let mut header_pos = 4usize;
        let mut data = Cursor::new(block.get(data_offset..)?);

        let shared_points = if tvc & SHARED_POINT_NUMBERS != 0 {
            Some(read_packed_points(&mut data)?)
        } else {
            None
        };

        let num_points = default_pts.len();

        for _ in 0..tuple_count {
            let variation_data_size = usize::from(get_u16(block, header_pos)?);
            let tuple_index = get_u16(block, header_pos + 2)?;
            header_pos += 4;

            let peak = if tuple_index & EMBEDDED_PEAK_TUPLE != 0 {
                let t = read_tuple(block, header_pos, self.axis_count)?;
                header_pos += self.axis_count * 2;
                t
            } else {
                self.shared_tuple(usize::from(tuple_index & TUPLE_INDEX_MASK))?
            };

            let (region_start, region_end) = if tuple_index & INTERMEDIATE_REGION != 0 {
                let s = read_tuple(block, header_pos, self.axis_count)?;
                header_pos += self.axis_count * 2;
                let e = read_tuple(block, header_pos, self.axis_count)?;
                header_pos += self.axis_count * 2;
                (Some(s), Some(e))
            } else {
                (None, None)
            };

            // The data cursor advances by the declared size whatever the delta
            // reader consumes; a short read is breakage, not a resync point.
            let tuple_start = data.pos;
            let tuple_end = tuple_start.checked_add(variation_data_size)?;
            if tuple_end > data.data.len() {
                return None;
            }
            data.pos = tuple_end;

            let scalar = region_scalar(
                &peak,
                region_start.as_deref(),
                region_end.as_deref(),
                coords,
            );
            if scalar == 0.0 {
                // A zero scalar skips the tuple's delta bytes entirely.
                continue;
            }

            let mut tuple_data = Cursor::new(data.data.get(tuple_start..tuple_end)?);

            let private_points = if tuple_index & PRIVATE_POINT_NUMBERS != 0 {
                Some(read_packed_points(&mut tuple_data)?)
            } else {
                None
            };
            let explicit: Option<&[u16]> = match private_points.as_ref().or(shared_points.as_ref())
            {
                Some(PointSet::Explicit(v)) => Some(v.as_slice()),
                Some(PointSet::All) | None => None,
            };

            let n = match explicit {
                Some(list) => list.len(),
                None => num_points,
            };
            read_packed_deltas(&mut tuple_data, n, &mut scratch.dx)?;
            read_packed_deltas(&mut tuple_data, n, &mut scratch.dy)?;
            if scratch.dx.len() < n || scratch.dy.len() < n {
                return None;
            }

            match explicit {
                None => {
                    // Every point is explicit: no inference, no IUP.
                    let count = num_points.min(n);
                    for (slot, (&dx, &dy)) in acc
                        .iter_mut()
                        .zip(scratch.dx.iter().zip(scratch.dy.iter()))
                        .take(count)
                    {
                        slot.0 += f64::from(dx) * scalar;
                        slot.1 += f64::from(dy) * scalar;
                    }
                }
                Some(list) => {
                    scratch.sparse.clear();
                    scratch.sparse.resize(num_points, None);
                    for (k, &p) in list.iter().enumerate() {
                        // Out-of-range point numbers are dropped per entry; the
                        // rest of the tuple stays usable.
                        let (Some(slot), Some(&dx), Some(&dy)) = (
                            scratch.sparse.get_mut(usize::from(p)),
                            scratch.dx.get(k),
                            scratch.dy.get(k),
                        ) else {
                            continue;
                        };
                        *slot = Some((f64::from(dx) * scalar, f64::from(dy) * scalar));
                    }
                    for &(a, b) in contours {
                        let (Some(sparse), Some(defaults)) =
                            (scratch.sparse.get(a..b), default_pts.get(a..b))
                        else {
                            continue;
                        };
                        iup_contour(sparse, defaults, &mut scratch.inferred);
                        for (i, &(ix, iy)) in scratch.inferred.iter().enumerate() {
                            if let Some(slot) = acc.get_mut(a + i) {
                                slot.0 += ix;
                                slot.1 += iy;
                            }
                        }
                    }
                }
            }
        }

        Some(())
    }
}

/// Read `axis_count` F2Dot14 values at `offset`.
fn read_tuple(data: &[u8], offset: usize, axis_count: usize) -> Option<Vec<i16>> {
    let len = axis_count.checked_mul(2)?;
    let end = offset.checked_add(len)?;
    let bytes = data.get(offset..end)?;
    Some(
        bytes
            .chunks_exact(2)
            .map(|c| i16::from_be_bytes([c[0], c[1]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_points_count_zero_means_all_and_consumes_nothing() {
        let data = [0x00u8, 0xAA, 0xBB];
        let mut cur = Cursor::new(&data);
        assert!(matches!(read_packed_points(&mut cur), Some(PointSet::All)));
        assert_eq!(cur.pos, 1);
    }

    #[test]
    fn packed_points_are_cumulative() {
        // count = 3, one byte-run of 3 values: 1, +2, +5 → 1, 3, 8.
        let data = [0x03u8, 0x02, 1, 2, 5];
        let mut cur = Cursor::new(&data);
        match read_packed_points(&mut cur) {
            Some(PointSet::Explicit(v)) => assert_eq!(v, vec![1, 3, 8]),
            other => panic!("expected explicit points, got {}", other.is_some()),
        }
    }

    #[test]
    fn packed_points_word_runs() {
        // count = 2, word-run of 2: 300, +1000 → 300, 1300.
        let data = [0x02u8, 0x81, 0x01, 0x2C, 0x03, 0xE8];
        let mut cur = Cursor::new(&data);
        match read_packed_points(&mut cur) {
            Some(PointSet::Explicit(v)) => assert_eq!(v, vec![300, 1300]),
            other => panic!("expected explicit points, got {}", other.is_some()),
        }
    }

    #[test]
    fn packed_points_two_byte_count() {
        // 0x80 0x02 → count 2.
        let data = [0x80u8, 0x02, 0x01, 7, 9];
        let mut cur = Cursor::new(&data);
        match read_packed_points(&mut cur) {
            Some(PointSet::Explicit(v)) => assert_eq!(v, vec![7, 16]),
            other => panic!("expected explicit points, got {}", other.is_some()),
        }
    }

    #[test]
    fn packed_deltas_zero_run_consumes_no_payload() {
        // 0x84 = zero run of 5, then a byte run of 1 with value -3.
        let data = [0x84u8, 0x00, 0xFD];
        let mut cur = Cursor::new(&data);
        let mut out = Vec::new();
        assert!(read_packed_deltas(&mut cur, 6, &mut out).is_some());
        assert_eq!(out, vec![0, 0, 0, 0, 0, -3]);
    }

    #[test]
    fn packed_deltas_word_run() {
        // 0x41 = word run of 2: 1000, -1000.
        let data = [0x41u8, 0x03, 0xE8, 0xFC, 0x18];
        let mut cur = Cursor::new(&data);
        let mut out = Vec::new();
        assert!(read_packed_deltas(&mut cur, 2, &mut out).is_some());
        assert_eq!(out, vec![1000, -1000]);
    }

    #[test]
    fn packed_deltas_truncate_but_consume_whole_runs() {
        // A run of 4 bytes read for n = 2 must still leave the cursor past all 4.
        let data = [0x03u8, 1, 2, 3, 4, 0x00, 9];
        let mut cur = Cursor::new(&data);
        let mut out = Vec::new();
        assert!(read_packed_deltas(&mut cur, 2, &mut out).is_some());
        assert_eq!(out, vec![1, 2]);
        assert_eq!(cur.pos, 5);
    }

    #[test]
    fn scalar_implied_tent() {
        // Peak 1.0, no intermediate region: a tent from 0 to 1.
        let peak = [16384i16];
        assert_eq!(region_scalar(&peak, None, None, &[16384]), 1.0);
        assert_eq!(region_scalar(&peak, None, None, &[0]), 0.0);
        assert!((region_scalar(&peak, None, None, &[8192]) - 0.5).abs() < 1e-12);
        // Opposite side of the default: outside the implied region.
        assert_eq!(region_scalar(&peak, None, None, &[-8192]), 0.0);
    }

    #[test]
    fn scalar_intermediate_region() {
        // start 0.25, peak 0.5, end 0.75.
        let peak = [8192i16];
        let start = [4096i16];
        let end = [12288i16];
        let s = |c: i16| region_scalar(&peak, Some(&start), Some(&end), &[c]);
        assert_eq!(s(8192), 1.0);
        assert_eq!(s(4096), 0.0);
        assert_eq!(s(12288), 0.0);
        assert!((s(6144) - 0.5).abs() < 1e-12);
        assert!((s(10240) - 0.5).abs() < 1e-12);
        assert_eq!(s(0), 0.0);
    }

    #[test]
    fn scalar_zero_peak_axis_is_ignored() {
        let peak = [0i16, 16384];
        assert_eq!(region_scalar(&peak, None, None, &[16384, 16384]), 1.0);
    }
}
