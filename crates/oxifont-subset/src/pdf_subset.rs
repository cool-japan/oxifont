//! On-the-fly font subsetting for PDF text rendering.
//!
//! This module provides [`PdfFontSubsetter`](crate::pdf_subset::PdfFontSubsetter),
//! a streaming accumulator designed for PDF text rendering pipelines.
//! During page composition, the renderer calls
//! [`PdfFontSubsetter::add_codepoint`](crate::pdf_subset::PdfFontSubsetter::add_codepoint)
//! or [`PdfFontSubsetter::add_gid`](crate::pdf_subset::PdfFontSubsetter::add_gid)
//! for each character or glyph it places on the page.  When all pages have been
//! composed, [`PdfFontSubsetter::finalize`](crate::pdf_subset::PdfFontSubsetter::finalize)
//! produces a minimal subset font containing exactly the glyphs that were used.
//!
//! # Design goals
//!
//! - **Zero-copy during accumulation**: codepoints and GIDs are collected into
//!   compact `BTreeSet`s; no font parsing occurs until `finalize`.
//! - **Idempotent additions**: adding the same codepoint or GID multiple times
//!   has no extra cost.
//! - **Configurable output**: the caller supplies [`SubsetOptions`] at
//!   construction time, allowing strip-hints / name-table filtering etc.
//! - **Statistics return**: `finalize` returns `(Vec<u8>, SubsetStats)` so the
//!   caller can record compression ratios and glyph counts.
//!
//! # Example
//!
//! ```no_run
//! use oxifont_subset::{SubsetOptions, pdf_subset::PdfFontSubsetter};
//!
//! let font_bytes: Vec<u8> = std::fs::read("font.ttf").expect("read font");
//! let opts = SubsetOptions::default()
//!     .strip_hints(false)
//!     .retain_names(true);
//!
//! let mut subsetter = PdfFontSubsetter::new(font_bytes, opts);
//!
//! // Accumulate glyphs during page composition.
//! for ch in "Hello, PDF world!" .chars() {
//!     subsetter.add_codepoint(ch);
//! }
//!
//! // Produce the minimal subset font.
//! let (subset_bytes, stats) = subsetter.finalize().expect("subset failed");
//! println!(
//!     "Original {} bytes → subset {} bytes, {} glyphs retained",
//!     stats.original_size, stats.subset_size, stats.glyphs_retained
//! );
//! ```

use std::collections::{BTreeMap, BTreeSet};

use crate::{subset_with_gid_set, SubsetError, SubsetOptions, SubsetStats};

// ---------------------------------------------------------------------------
// PdfFontSubsetter
// ---------------------------------------------------------------------------

/// Streaming font subsetter for PDF text rendering pipelines.
///
/// Accumulates Unicode codepoints and/or raw GIDs across multiple text
/// placement operations and produces a minimal subset font on
/// [`PdfFontSubsetter::finalize`].
///
/// # Thread safety
///
/// `PdfFontSubsetter` is **not** `Sync`.  For multi-threaded PDF renderers
/// (e.g. parallel page compositing) each thread should use its own instance
/// and merge the accumulated codepoint sets before finalizing.  Use
/// [`PdfFontSubsetter::merge`] to combine two accumulators.
#[derive(Debug)]
pub struct PdfFontSubsetter {
    /// Raw font bytes that will be subset.
    font_data: Vec<u8>,
    /// Subsetting configuration (strip_hints, retain_names, …).
    opts: SubsetOptions,
    /// Accumulated Unicode codepoints (from `add_codepoint` / `add_text`).
    codepoints: BTreeSet<char>,
    /// Accumulated raw GIDs (from `add_gid` / `add_gids`).
    ///
    /// These bypass cmap lookup and are passed directly to the subset engine.
    gids: BTreeSet<u16>,
}

impl PdfFontSubsetter {
    /// Create a new accumulator that will subset `font_data` using `opts`.
    ///
    /// The font data is stored as-is; no parsing occurs until [`finalize`].
    ///
    /// [`finalize`]: PdfFontSubsetter::finalize
    pub fn new(font_data: Vec<u8>, opts: SubsetOptions) -> Self {
        Self {
            font_data,
            opts,
            codepoints: BTreeSet::new(),
            gids: BTreeSet::new(),
        }
    }

    /// Create a new accumulator using the default PDF subsetting options.
    ///
    /// The default PDF preset keeps hint tables and the full name table
    /// (matching [`crate::subset_font_for_pdf`]).
    pub fn for_pdf(font_data: Vec<u8>) -> Self {
        let opts = SubsetOptions::default()
            .strip_hints(false)
            .retain_names(true);
        Self::new(font_data, opts)
    }

    /// Create a new accumulator using the web subsetting preset.
    ///
    /// The web preset strips hint tables and trims the name table to IDs 0–6
    /// (matching [`crate::subset_font_for_web`]).
    pub fn for_web(font_data: Vec<u8>) -> Self {
        let opts = SubsetOptions::default()
            .strip_hints(true)
            .retain_names(false);
        Self::new(font_data, opts)
    }

    // -----------------------------------------------------------------------
    // Accumulation API
    // -----------------------------------------------------------------------

    /// Register a single Unicode codepoint for inclusion in the subset.
    ///
    /// The codepoint is resolved to a GID via the cmap table during
    /// [`finalize`](Self::finalize).  No-ops if `cp` has already been added.
    #[inline]
    pub fn add_codepoint(&mut self, cp: char) {
        self.codepoints.insert(cp);
    }

    /// Register every codepoint in `text` for inclusion in the subset.
    ///
    /// Iterates over Unicode scalar values (not bytes) so multi-byte UTF-8
    /// sequences are handled correctly.
    pub fn add_text(&mut self, text: &str) {
        for cp in text.chars() {
            self.codepoints.insert(cp);
        }
    }

    /// Register a raw GID (Glyph ID) for inclusion in the subset.
    ///
    /// GIDs registered via this method bypass the cmap scan.  The resulting
    /// subset font's cmap will **not** map any Unicode codepoint to these
    /// GIDs unless the same codepoints are also added via [`add_codepoint`](Self::add_codepoint) /
    /// [`add_text`](Self::add_text).  This is the correct behaviour for PDF Type3 / CIDFont
    /// workflows where text extraction is handled externally via a ToUnicode
    /// CMap.
    ///
    /// No-op if `gid` has already been added.
    ///
    /// [`add_codepoint`]: PdfFontSubsetter::add_codepoint
    #[inline]
    pub fn add_gid(&mut self, gid: u16) {
        self.gids.insert(gid);
    }

    /// Register a slice of raw GIDs.
    pub fn add_gids(&mut self, gids: &[u16]) {
        for &g in gids {
            self.gids.insert(g);
        }
    }

    // -----------------------------------------------------------------------
    // Inspection
    // -----------------------------------------------------------------------

    /// Returns the number of distinct codepoints accumulated so far.
    pub fn codepoint_count(&self) -> usize {
        self.codepoints.len()
    }

    /// Returns the number of distinct raw GIDs accumulated so far
    /// (excludes GIDs resolved from codepoints, which are counted at finalize).
    pub fn gid_count(&self) -> usize {
        self.gids.len()
    }

    /// Returns `true` if no codepoints or GIDs have been accumulated yet.
    pub fn is_empty(&self) -> bool {
        self.codepoints.is_empty() && self.gids.is_empty()
    }

    /// Returns a reference to the accumulated codepoint set.
    pub fn codepoints(&self) -> &BTreeSet<char> {
        &self.codepoints
    }

    /// Returns a reference to the accumulated raw GID set.
    pub fn raw_gids(&self) -> &BTreeSet<u16> {
        &self.gids
    }

    // -----------------------------------------------------------------------
    // Merge
    // -----------------------------------------------------------------------

    /// Merge the accumulated codepoints and GIDs from `other` into `self`.
    ///
    /// After merging, `other` is reset to an empty state (its `font_data` and
    /// `opts` are preserved so it can be reused for the next page batch).
    ///
    /// # Panics
    /// Does not panic, but merging accumulators that were constructed with
    /// different font data or options is a logical error.  The resulting subset
    /// will use `self`'s font data and options.
    pub fn merge(&mut self, other: &mut Self) {
        self.codepoints.extend(other.codepoints.iter().copied());
        self.gids.extend(other.gids.iter().copied());
        other.codepoints.clear();
        other.gids.clear();
    }

    // -----------------------------------------------------------------------
    // Finalize
    // -----------------------------------------------------------------------

    /// Produce the minimal subset font from the accumulated codepoints and GIDs.
    ///
    /// The subsetting pipeline:
    /// 1. Parses the stored font's cmap table.
    /// 2. Resolves accumulated codepoints → old GIDs via cmap.
    /// 3. Adds accumulated raw GIDs directly.
    /// 4. Always includes `.notdef` (GID 0).
    /// 5. Runs the full table rewriting pipeline (glyf/loca/cmap/hmtx/…) with
    ///    the configured [`SubsetOptions`].
    ///
    /// Returns `(subset_bytes, stats)`.  The accumulator is **not** reset by
    /// this call — call [`reset`] if you need to reuse the accumulator for a
    /// new document.
    ///
    /// # Errors
    /// Returns [`SubsetError`] if the stored font data is structurally invalid
    /// or a required table is absent.
    ///
    /// [`reset`]: PdfFontSubsetter::reset
    pub fn finalize(&self) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
        use crate::tables::read_table_directory;

        let font_data = &self.font_data;

        // Parse the cmap to resolve codepoints → old GIDs.
        let orig_tables = read_table_directory(font_data)?;
        let cmap_data = orig_tables
            .get(b"cmap")
            .copied()
            .ok_or(SubsetError::TableMissing(*b"cmap"))?;

        let cp_to_all_gids = crate::cmap_to_gid_map_pub(cmap_data)?;

        // Build the final GID set: .notdef + cmap-resolved + raw GIDs.
        let mut old_gid_set: BTreeSet<u16> = BTreeSet::new();
        old_gid_set.insert(0); // always include .notdef

        // raw GIDs bypass cmap.
        old_gid_set.extend(self.gids.iter().copied());

        // codepoints resolved through cmap.
        let mut cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();
        for &cp in &self.codepoints {
            let cp_u32 = cp as u32;
            if let Some(&old_gid) = cp_to_all_gids.get(&cp_u32) {
                if old_gid != 0 {
                    old_gid_set.insert(old_gid);
                    cp_to_old_gid.insert(cp_u32, old_gid);
                }
            }
        }

        subset_with_gid_set(font_data, &old_gid_set, &cp_to_old_gid, &self.opts)
    }

    // -----------------------------------------------------------------------
    // Reset
    // -----------------------------------------------------------------------

    /// Clear the accumulated codepoints and GIDs, ready to subset a new document.
    ///
    /// The `font_data` and `opts` are preserved.
    pub fn reset(&mut self) {
        self.codepoints.clear();
        self.gids.clear();
    }
}

// ---------------------------------------------------------------------------
// PdfSubsetResult
// ---------------------------------------------------------------------------

/// The output of a finalized [`PdfFontSubsetter`], combining the subset bytes
/// and the associated statistics.
///
/// Created by [`PdfFontSubsetter::finalize_into_result`].
#[derive(Debug, Clone)]
pub struct PdfSubsetResult {
    /// The minimal subset font as raw SFNT bytes.
    pub bytes: Vec<u8>,
    /// Statistics about the subset operation.
    pub stats: SubsetStats,
}

impl PdfFontSubsetter {
    /// Produce a [`PdfSubsetResult`] combining subset bytes and statistics.
    ///
    /// This is a convenience wrapper around [`finalize`] that bundles the
    /// output into a single struct.
    ///
    /// [`finalize`]: PdfFontSubsetter::finalize
    pub fn finalize_into_result(&self) -> Result<PdfSubsetResult, SubsetError> {
        let (bytes, stats) = self.finalize()?;
        Ok(PdfSubsetResult { bytes, stats })
    }

    /// Consume `self` and finalize, returning `(font_data, subset_bytes, stats)`.
    ///
    /// Returns the original font data alongside the subset so that callers can
    /// re-create an accumulator if they need to re-subset later.
    pub fn into_finalized(self) -> Result<(Vec<u8>, Vec<u8>, SubsetStats), SubsetError> {
        let (subset_bytes, stats) = self.finalize()?;
        Ok((self.font_data, subset_bytes, stats))
    }
}
