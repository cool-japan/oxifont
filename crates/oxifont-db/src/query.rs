//! CSS Fonts Level 4 + fontconfig hybrid query engine.
//!
//! # Matching algorithm
//!
//! The algorithm is a **hybrid**: CSS Fonts Module Level 4 §4.5 defines the
//! precedence and tie-breaking rules; fontconfig's generic-alias resolution is
//! layered on top to map generic family names (`"sans-serif"`, `"serif"`,
//! `"monospace"`, `"cursive"`, `"fantasy"`) to concrete candidate families
//! before the CSS narrowing phase begins.
//!
//! ## Step-by-step
//!
//! 1. **Generic alias resolution** (fontconfig-inspired static table):
//!    If `family` is a recognised generic name, expand it to an ordered list
//!    of concrete family names.
//!
//! 2. **Family filter** (CSS §4.5.2):
//!    Keep only faces whose lower-cased family matches one of the candidate
//!    families.  The comparison is case-insensitive.
//!
//! 3. **Stretch filter** (CSS §4.5.3):
//!    - Requested stretch ≤ 5 (normal or condensed): prefer nearest at-or-below
//!      in descending order; if none, take nearest above ascending.
//!    - Requested stretch > 5 (expanded): prefer nearest at-or-above ascending;
//!      if none, take nearest below descending.
//!
//! 4. **Style filter** (CSS §4.5.4):
//!    - Italic requested: prefer italic/oblique faces; fall back to normal.
//!    - Normal requested: prefer normal faces; fall back to italic/oblique.
//!
//! 5. **Weight filter** (CSS §4.5.5 — see below):
//!    - == 400: 400, 500, <400 descending, >500 ascending.
//!    - == 500: 500, 400, <400 descending, >500 ascending.
//!    - < 400: nearest below descending, then nearest above ascending.
//!    - > 500: nearest above ascending, then nearest below descending.
//!
//! 6. **Variable-font preference**:
//!    If a candidate face is a variable font whose `wght` axis covers the
//!    requested weight, it is preferred over a static face for the same family.
//!    When italic is requested, preference is also given to variable faces with
//!    an `ital` axis (max ≥ 1.0).  When a non-normal stretch is requested,
//!    faces with a `wdth` axis covering the equivalent percentage are preferred.
//!
//! 7. The first survivor is returned (or all survivors sorted by quality for
//!    [`Query::match_all`]).

use smallvec::SmallVec;

use crate::db::FontDatabase;
use crate::face::FaceInfo;

// ---------------------------------------------------------------------------
// SmallVec capacity constants
// ---------------------------------------------------------------------------

/// Inline capacity for face-index buffers.
///
/// Most font families have ≤ 16 faces (Regular, Bold, Italic, BoldItalic, …),
/// so this capacity avoids heap allocation in the common case.
const FACE_VEC_INLINE: usize = 16;

/// Inline capacity for weight-pair buffers used inside CSS band processing.
///
/// Chosen to match `FACE_VEC_INLINE` since the number of weight pairs equals
/// the number of candidate faces.
const BAND_VEC_INLINE: usize = 16;

// ---------------------------------------------------------------------------
// Private type aliases to keep complex SmallVec types readable
// ---------------------------------------------------------------------------

/// `(face_index, (locale_miss, axis_miss, oblique_miss))` — the scored tuple
/// used in the tiebreaker step of `ordered_indices`.
type ScoredFace = (usize, (u8, u8, u8));

// ---------------------------------------------------------------------------
// CSS wdth axis mapping (stretch u8 → percentage)
// ---------------------------------------------------------------------------

/// Maps a CSS stretch value (1–9, `usWidthClass`) to the corresponding
/// percentage used by the OpenType `wdth` variation axis.
fn stretch_to_wdth_percent(stretch: u8) -> f32 {
    match stretch {
        1 => 50.0,
        2 => 62.5,
        3 => 75.0,
        4 => 87.5,
        5 => 100.0,
        6 => 112.5,
        7 => 125.0,
        8 => 150.0,
        _ => 200.0, // 9 and above → ultra-expanded
    }
}

// ---------------------------------------------------------------------------
// Fontconfig generic alias table
// ---------------------------------------------------------------------------

/// Static mapping from CSS generic family name to an ordered list of concrete
/// candidate family names (fontconfig-compatible).
///
/// CJK-specific generics (`"cjk-sans-serif"`, `"cjk-serif"`) are also
/// provided.  The existing `"sans-serif"` and `"serif"` entries include CJK
/// fallback families at the end of their candidate lists.
static GENERIC_ALIASES: &[(&str, &[&str])] = &[
    (
        "sans-serif",
        &[
            "Arial",
            "Helvetica Neue",
            "Helvetica",
            "Liberation Sans",
            "DejaVu Sans",
            "FreeSans",
            "Noto Sans",
            // CJK fallbacks for multi-script text
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Source Han Sans",
            "SimHei",
            "MS Gothic",
        ],
    ),
    (
        "serif",
        &[
            "Times New Roman",
            "Georgia",
            "Liberation Serif",
            "DejaVu Serif",
            "FreeSerif",
            "Noto Serif",
            // CJK fallbacks for multi-script text
            "Noto Serif CJK SC",
            "Noto Serif CJK JP",
            "Source Han Serif",
            "SimSun",
            "MS Mincho",
        ],
    ),
    (
        "monospace",
        &[
            "Courier New",
            "Courier",
            "Liberation Mono",
            "DejaVu Sans Mono",
            "FreeMono",
            "Noto Sans Mono",
        ],
    ),
    ("cursive", &["Comic Sans MS", "Brush Script MT"]),
    ("fantasy", &["Impact", "Papyrus"]),
    (
        "cjk-sans-serif",
        &[
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Source Han Sans",
            "SimHei",
            "MS Gothic",
        ],
    ),
    (
        "cjk-serif",
        &[
            "Noto Serif CJK SC",
            "Noto Serif CJK JP",
            "Source Han Serif",
            "SimSun",
            "MS Mincho",
        ],
    ),
];

/// Resolve a family name through the generic alias table.
///
/// If `name` matches a known generic (case-insensitive), returns the expanded
/// candidate list.  Otherwise returns `None`.
fn resolve_generic(name: &str) -> Option<&'static [&'static str]> {
    let lower = name.to_lowercase();
    GENERIC_ALIASES
        .iter()
        .find(|(k, _)| *k == lower.as_str())
        .map(|(_, v)| *v)
}

// ---------------------------------------------------------------------------
// CSS L4 weight selection
// ---------------------------------------------------------------------------

/// CSS Fonts Level 4 §4.5.5 weight ordering for a set of available weights.
///
/// Returns the candidates sorted in preference order for the given `requested`
/// weight.  The input `candidates` slice must contain `(weight, original_index)`
/// pairs.  Each index appears at most once in the output.
///
/// Uses `SmallVec` to avoid heap allocation for the common case of ≤16 faces.
fn css_weight_order(
    requested: u16,
    mut candidates: SmallVec<[(u16, usize); BAND_VEC_INLINE]>,
) -> SmallVec<[usize; FACE_VEC_INLINE]> {
    // Sort and deduplicate indices so bands can match without worrying about
    // indices appearing in multiple bands (e.g. exact-match weight appears in
    // both AboveOrEqual and BelowOrEqual bands).
    candidates.sort_by_key(|(w, _)| *w);

    match requested {
        400 => {
            // Prefer 400 → 500 → <400 descending → >500 ascending.
            ordered_by_weight_preference(
                &candidates,
                &[
                    WeightBand::Exact(400),
                    WeightBand::Exact(500),
                    WeightBand::Below(400),
                    WeightBand::Above(500),
                ],
            )
        }
        500 => {
            // Prefer 500 → 400 → <400 descending → >500 ascending.
            ordered_by_weight_preference(
                &candidates,
                &[
                    WeightBand::Exact(500),
                    WeightBand::Exact(400),
                    WeightBand::Below(400),
                    WeightBand::Above(500),
                ],
            )
        }
        w if w < 400 => {
            // Nearest below descending, then nearest above ascending.
            ordered_by_weight_preference(
                &candidates,
                &[WeightBand::BelowOrEqual(w), WeightBand::AboveOrEqual(w)],
            )
        }
        w => {
            // w > 500: nearest above ascending, then nearest below descending.
            ordered_by_weight_preference(
                &candidates,
                &[WeightBand::AboveOrEqual(w), WeightBand::BelowOrEqual(w)],
            )
        }
    }
}

/// Describes a band of weight values for the preference ordering.
#[derive(Clone, Copy)]
enum WeightBand {
    Exact(u16),
    Below(u16), // strictly below threshold, descending
    Above(u16), // strictly above threshold, ascending
    BelowOrEqual(u16),
    AboveOrEqual(u16),
}

/// Build the preference-ordered list of face indices from bands.
///
/// Each index appears at most once in the output: once an index has been added
/// it is skipped in subsequent bands.
///
/// # Deduplication strategy
///
/// The previous implementation used `result.contains(&idx)` which is O(n) per
/// insert, making the overall function O(n²) for n candidates across all bands.
/// This implementation instead maintains a `SmallVec<[bool; 64]>` seen-bitset
/// indexed by face index, providing O(1) deduplication at the cost of one extra
/// allocation only when there are >64 faces in a single family (extremely rare).
///
/// Internal band buffers also use `SmallVec` to avoid heap allocation.
fn ordered_by_weight_preference(
    candidates: &[(u16, usize)],
    bands: &[WeightBand],
) -> SmallVec<[usize; FACE_VEC_INLINE]> {
    // Find the maximum face index to size the seen buffer.
    let max_idx = candidates.iter().map(|(_, i)| *i).max().unwrap_or(0);

    // Seen bitset: index `i` in this vec is `true` once face index `i` has
    // been added to `result`.  Inline capacity 64 covers the vast majority of
    // real families without a heap allocation.
    let mut seen: SmallVec<[bool; 64]> = SmallVec::from_elem(false, max_idx + 1);
    let mut result: SmallVec<[usize; FACE_VEC_INLINE]> = SmallVec::new();

    for band in bands {
        match band {
            WeightBand::Exact(target) => {
                for (w, idx) in candidates {
                    if w == target && !seen[*idx] {
                        seen[*idx] = true;
                        result.push(*idx);
                    }
                }
            }
            WeightBand::Below(threshold) => {
                // Collect all weights strictly below threshold, sort descending.
                let mut below: SmallVec<[(u16, usize); BAND_VEC_INLINE]> = candidates
                    .iter()
                    .filter(|(w, _)| *w < *threshold)
                    .copied()
                    .collect();
                below.sort_by(|(a, _), (b, _)| b.cmp(a));
                for (_, idx) in below {
                    if !seen[idx] {
                        seen[idx] = true;
                        result.push(idx);
                    }
                }
            }
            WeightBand::Above(threshold) => {
                // Collect all weights strictly above threshold, sort ascending.
                let mut above: SmallVec<[(u16, usize); BAND_VEC_INLINE]> = candidates
                    .iter()
                    .filter(|(w, _)| *w > *threshold)
                    .copied()
                    .collect();
                above.sort_by_key(|(w, _)| *w);
                for (_, idx) in above {
                    if !seen[idx] {
                        seen[idx] = true;
                        result.push(idx);
                    }
                }
            }
            WeightBand::BelowOrEqual(threshold) => {
                let mut below: SmallVec<[(u16, usize); BAND_VEC_INLINE]> = candidates
                    .iter()
                    .filter(|(w, _)| *w <= *threshold)
                    .copied()
                    .collect();
                below.sort_by(|(a, _), (b, _)| b.cmp(a));
                for (_, idx) in below {
                    if !seen[idx] {
                        seen[idx] = true;
                        result.push(idx);
                    }
                }
            }
            WeightBand::AboveOrEqual(threshold) => {
                let mut above: SmallVec<[(u16, usize); BAND_VEC_INLINE]> = candidates
                    .iter()
                    .filter(|(w, _)| *w >= *threshold)
                    .copied()
                    .collect();
                above.sort_by_key(|(w, _)| *w);
                for (_, idx) in above {
                    if !seen[idx] {
                        seen[idx] = true;
                        result.push(idx);
                    }
                }
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Query builder
// ---------------------------------------------------------------------------

/// Builder-style CSS Level 4 font query.
///
/// Construct with [`Query::new`], chain setter methods, then call
/// [`Query::match_best`] to obtain the best-matching face or [`Query::match_all`]
/// to obtain all matching faces sorted by quality.
///
/// # Example
/// ```no_run
/// use oxifont_db::{FontDatabase, Query};
///
/// let mut db = FontDatabase::new();
/// // … load fonts …
/// let face = Query::new(&db)
///     .family("sans-serif")
///     .weight(400)
///     .italic(false)
///     .match_best();
/// ```
pub struct Query<'a> {
    db: &'a FontDatabase,
    families: Vec<String>,
    weight: u16,
    italic: bool,
    oblique: bool,
    stretch: u8,
    locale: Option<String>,
}

impl<'a> Query<'a> {
    /// Create a new query against `db` with default parameters (weight 400,
    /// non-italic, normal stretch, no locale preference).
    pub fn new(db: &'a FontDatabase) -> Self {
        Self {
            db,
            families: Vec::new(),
            weight: 400,
            italic: false,
            oblique: false,
            stretch: 5,
            locale: None,
        }
    }

    /// Add a font family to search for.
    ///
    /// Multiple calls accumulate into a **priority-ordered fallback list**: the
    /// query engine tries each family in the order they were added.  The first
    /// family for which at least one matching face exists in the database is
    /// used; subsequent families are only consulted when all earlier ones yield
    /// no match at the family-filter step.
    ///
    /// ```
    /// # use oxifont_db::{FontDatabase, Query};
    /// # let db = FontDatabase::new();
    /// let query = Query::new(&db)
    ///     .family("Helvetica Neue")
    ///     .family("Arial")
    ///     .family("sans-serif");
    /// ```
    ///
    /// The query engine tries families left-to-right:
    ///
    /// 1. `"Helvetica Neue"` — used if any face with that family name is loaded.
    /// 2. `"Arial"` — consulted next if Helvetica Neue is absent.
    /// 3. `"sans-serif"` — expanded via the fontconfig-inspired alias table into
    ///    concrete candidates (Arial, Helvetica Neue, Liberation Sans, DejaVu
    ///    Sans, Noto Sans, …) **in that alias order**.
    ///
    /// Note that the alias expansion happens per entry: `.family("sans-serif")`
    /// does not collapse to a single name but injects the entire ordered alias
    /// list at that position in the candidate chain.
    ///
    /// ## Generic family names
    ///
    /// The names `"sans-serif"`, `"serif"`, `"monospace"`, `"cursive"`,
    /// `"fantasy"`, `"cjk-sans-serif"`, and `"cjk-serif"` are **category
    /// aliases**, not literal family names.  They are expanded to their
    /// respective concrete candidate lists before any face lookup occurs.
    /// Comparison is case-insensitive (`"Sans-Serif"` is equivalent to
    /// `"sans-serif"`).
    pub fn family(mut self, name: impl Into<String>) -> Self {
        self.families.push(name.into());
        self
    }

    /// Set the desired CSS weight (100–900).
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }

    /// Set whether an italic face is desired.
    pub fn italic(mut self, i: bool) -> Self {
        self.italic = i;
        self
    }

    /// Set whether an oblique (mechanically slanted) face is preferred over
    /// a true italic.  When `true`, faces whose PostScript name contains
    /// `"Oblique"` (case-insensitive) are preferred among italic/oblique
    /// candidates.  This is a soft preference — non-oblique faces are still
    /// returned when no oblique candidate is available.
    pub fn oblique(mut self, o: bool) -> Self {
        self.oblique = o;
        self
    }

    /// Set the desired CSS stretch value (1 = ultra-condensed, 9 = ultra-expanded).
    pub fn stretch(mut self, s: u8) -> Self {
        self.stretch = s;
        self
    }

    /// Set a preferred BCP-47 locale tag (e.g. `"ja-JP"`, `"fr-FR"`).
    ///
    /// When set, faces whose [`FaceInfo::locale_families`] map contains an
    /// entry for the locale (or a matching language-only prefix) are softly
    /// preferred over faces that lack locale metadata.  This never eliminates
    /// candidates — it only breaks ties.
    pub fn locale(mut self, bcp47: impl Into<String>) -> Self {
        self.locale = Some(bcp47.into());
        self
    }

    /// Run the CSS Level 4 + fontconfig hybrid matching algorithm and return
    /// the best-matching face, or `None` if no faces match.
    ///
    /// The candidate set is built from the **accumulated family list** (see
    /// [`Query::family`]).  All families are expanded (generics are resolved to
    /// their alias lists) and merged into a single ordered candidate pool before
    /// the CSS stretch / style / weight narrowing phases run.  The family list
    /// order acts as a soft preference only at the family-filter step; within
    /// the surviving candidates the CSS tie-breaking rules apply.
    ///
    /// See the [module-level documentation](self) for a full description of the
    /// algorithm.
    pub fn match_best(&self) -> Option<&'a FaceInfo> {
        let ordered = self.ordered_indices()?;
        Some(&self.db.faces()[ordered[0]])
    }

    /// Run the matching algorithm and return **all** faces that pass the
    /// family / stretch / style filters, sorted in preference order (best first).
    ///
    /// The returned slice may be empty when no faces match the family filter.
    pub fn match_all(&self) -> Vec<&'a FaceInfo> {
        let all_faces = self.db.faces();
        match self.ordered_indices() {
            Some(indices) => indices.into_iter().map(|i| &all_faces[i]).collect(),
            None => Vec::new(),
        }
    }

    /// Build an approximate font fallback chain for `text`.
    ///
    /// Returns an ordered list of faces that together cover all Unicode
    /// codepoints in `text`.  The **primary match** comes from
    /// [`Query::match_best`] — which in turn uses the accumulated family list
    /// set via [`Query::family`] calls — and is placed first.  Additional
    /// faces are appended to cover codepoints the primary face does not claim
    /// to support (via [`FaceInfo::covers_char_approx`]).
    ///
    /// # Algorithm
    ///
    /// 1. Collect the unique set of characters in `text`.
    /// 2. Take the primary face from `match_best`.
    /// 3. Mark characters covered by the primary face as satisfied.
    /// 4. For each remaining character, scan all database faces (in insertion
    ///    order) for the first face that covers it, and append that face to the
    ///    chain (deduplicated).
    /// 5. Repeat until all characters are satisfied or no face covers the
    ///    remaining ones (prevents infinite loops for exotic codepoints with no
    ///    known face).
    ///
    /// Coverage is **approximate**: `covers_char_approx` checks OS/2 range bits,
    /// which may yield false positives.  Faces with `unicode_ranges == 0`
    /// (unknown) are treated as covering everything, which may lead to a shorter
    /// chain than expected but never to a longer one.
    ///
    /// Returns an empty `Vec` when `text` is empty or no face matches the query.
    pub fn match_with_fallback(&self, text: &str) -> Vec<&'a FaceInfo> {
        // Collect the unique character set from the text (ignore surrogates / NUL).
        let chars: Vec<char> = {
            let mut seen = std::collections::HashSet::new();
            text.chars().filter(|c| seen.insert(*c)).collect()
        };

        if chars.is_empty() {
            return Vec::new();
        }

        let all_faces = self.db.faces();
        let mut chain: Vec<&'a FaceInfo> = Vec::new();
        let mut uncovered: Vec<char> = chars;

        // Step 1: add the primary face.
        if let Some(primary) = self.match_best() {
            uncovered.retain(|c| !primary.covers_char_approx(*c));
            chain.push(primary);
        } else {
            // No primary match — fall back to a full scan below.
        }

        // Step 2: greedily cover remaining codepoints by scanning all faces.
        // We iterate until no progress is made (avoids infinite loop) or
        // all codepoints are covered.
        loop {
            if uncovered.is_empty() {
                break;
            }

            let prev_uncovered_len = uncovered.len();
            let mut made_progress = false;

            // For each remaining uncovered character, find the first face
            // (not already in chain) that covers it.
            let mut i = 0;
            while i < uncovered.len() {
                let c = uncovered[i];
                let found = all_faces.iter().find(|face| {
                    // Skip faces already in the chain.
                    if chain.iter().any(|f| f.id == face.id) {
                        return false;
                    }
                    face.covers_char_approx(c)
                });
                if let Some(face) = found {
                    // Add this face and immediately re-scan uncovered to remove
                    // all chars it covers, then restart the inner loop.
                    uncovered.retain(|ch| !face.covers_char_approx(*ch));
                    chain.push(face);
                    made_progress = true;
                    // uncovered has changed; restart from the beginning.
                    i = 0;
                } else {
                    i += 1;
                }
            }

            if !made_progress || uncovered.len() == prev_uncovered_len {
                // No face covers any remaining codepoint — stop.
                break;
            }
        }

        chain
    }

    // -----------------------------------------------------------------------
    // Internal: CSS L4 pipeline → preference-ordered face indices
    // -----------------------------------------------------------------------

    fn ordered_indices(&self) -> Option<SmallVec<[usize; FACE_VEC_INLINE]>> {
        // ---- Step 1: resolve generic aliases into concrete candidate families.
        //
        // Use a SmallVec for the family list; most queries have ≤ 16 families.
        let mut candidate_families: SmallVec<[String; 16]> = SmallVec::new();
        for name in &self.families {
            if let Some(aliases) = resolve_generic(name) {
                candidate_families.extend(aliases.iter().map(|s| s.to_lowercase()));
            } else {
                candidate_families.push(name.to_lowercase());
            }
        }

        // ---- Step 2: collect face indices matching any candidate family.
        //
        // Deduplication is O(1) via a seen-bitset keyed by face index.  This
        // replaces the previous O(n²) `face_indices.contains(&idx)` pattern.
        let total_faces = self.db.faces().len();
        let mut seen: SmallVec<[bool; 64]> = SmallVec::from_elem(false, total_faces.max(1));
        let mut face_indices: SmallVec<[usize; FACE_VEC_INLINE]> = SmallVec::new();
        for family in &candidate_families {
            for &idx in self.db.faces_by_family_lower(family) {
                if !seen[idx] {
                    seen[idx] = true;
                    face_indices.push(idx);
                }
            }
        }

        if face_indices.is_empty() {
            return None;
        }

        let all_faces = self.db.faces();

        // ---- Step 3: stretch narrowing (CSS §4.5.3).
        face_indices = narrow_by_stretch(face_indices, all_faces, self.stretch);
        if face_indices.is_empty() {
            return None;
        }

        // ---- Step 4: style narrowing (CSS §4.5.4).
        face_indices = narrow_by_style(face_indices, all_faces, self.italic);
        if face_indices.is_empty() {
            return None;
        }

        // ---- Step 5: weight ordering (CSS §4.5.5).
        let weight_pairs: SmallVec<[(u16, usize); BAND_VEC_INLINE]> = face_indices
            .iter()
            .map(|&idx| (all_faces[idx].weight, idx))
            .collect();

        let ordered = css_weight_order(self.weight, weight_pairs);

        // ---- Step 6: multi-dimensional tiebreaker (variable-font preference,
        //              locale, oblique).
        //
        // The key is a tuple (locale_miss, axis_miss, oblique_miss) where
        // lower = better.  Tuple comparison is lexicographic, so locale is
        // evaluated first, then axis coverage, then oblique preference.
        let ital_tag: [u8; 4] = [b'i', b't', b'a', b'l'];
        let wdth_tag: [u8; 4] = [b'w', b'd', b't', b'h'];
        let wdth_pct = stretch_to_wdth_percent(self.stretch);

        let locale_lower = self.locale.as_deref().map(|s| s.to_lowercase());

        let mut scored: SmallVec<[ScoredFace; FACE_VEC_INLINE]> = ordered
            .into_iter()
            .map(|idx| {
                let face = &all_faces[idx];

                // Locale preference: 0 = locale matched, 1 = no locale data
                let locale_miss: u8 = if let Some(ref loc) = locale_lower {
                    if face_has_locale(face, loc) {
                        0
                    } else {
                        1
                    }
                } else {
                    0 // locale not requested — no penalty
                };

                // Variable-axis coverage: check wght, ital (if italic),
                // wdth (if non-normal stretch).
                let axis_miss: u8 = {
                    let wght_ok = face.covers_weight(self.weight);
                    let ital_ok = if self.italic {
                        face.variable_axes
                            .iter()
                            .any(|ax| ax.tag == ital_tag && ax.max_value >= 1.0)
                    } else {
                        true // not requested, no penalty
                    };
                    let wdth_ok = if self.stretch != 5 {
                        face.variable_axes.iter().any(|ax| {
                            ax.tag == wdth_tag
                                && wdth_pct >= ax.min_value
                                && wdth_pct <= ax.max_value
                        })
                    } else {
                        true // normal stretch, no axis check needed
                    };

                    // Each un-covered axis that was requested adds 1.
                    let mut miss = 0u8;
                    if !wght_ok {
                        miss += 4;
                    }
                    if !ital_ok {
                        miss += 2;
                    }
                    if !wdth_ok {
                        miss += 1;
                    }
                    miss
                };

                // Oblique preference: 0 = is oblique, 1 = not oblique
                let oblique_miss: u8 = if self.oblique {
                    let is_oblique = face.post_script_name.to_lowercase().contains("oblique");
                    if is_oblique {
                        0
                    } else {
                        1
                    }
                } else {
                    0 // not requested
                };

                (idx, (locale_miss, axis_miss, oblique_miss))
            })
            .collect();

        // Stable sort preserves the CSS weight order within equal-score groups.
        scored.sort_by_key(|(_, score)| *score);

        let indices: SmallVec<[usize; FACE_VEC_INLINE]> =
            scored.into_iter().map(|(i, _)| i).collect();
        Some(indices)
    }
}

// ---------------------------------------------------------------------------
// Locale soft-match helper
// ---------------------------------------------------------------------------

/// Returns `true` when `face` has a locale-specific family name entry for
/// the given BCP-47 string (or any prefix obtained by progressively dropping
/// trailing subtags separated by `-`).
fn face_has_locale(face: &FaceInfo, bcp47_lower: &str) -> bool {
    if face.locale_families.is_empty() {
        return false;
    }

    // Try progressively shorter prefixes: "ja-jp", then "ja".
    let mut tag = bcp47_lower;
    loop {
        if let Some(lcid) = crate::locale::bcp47_to_lcid(tag) {
            if face.locale_families.iter().any(|(id, _)| *id == lcid) {
                return true;
            }
        }
        match tag.rfind('-') {
            Some(pos) => tag = &tag[..pos],
            None => break,
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Narrowing helpers
// ---------------------------------------------------------------------------

/// CSS §4.5.3 stretch narrowing.
///
/// - requested ≤ 5: prefer nearest at-or-below (descending), then nearest
///   above (ascending).
/// - requested > 5: prefer nearest at-or-above (ascending), then nearest
///   below (descending).
///
/// Uses `SmallVec` to avoid heap allocation for families with ≤16 faces.
fn narrow_by_stretch(
    indices: SmallVec<[usize; FACE_VEC_INLINE]>,
    faces: &[FaceInfo],
    requested: u8,
) -> SmallVec<[usize; FACE_VEC_INLINE]> {
    // Compute the best stretch value by scanning directly over the SmallVec.
    let best_stretch = if requested <= 5 {
        // Prefer at-or-below descending, then above ascending.
        indices
            .iter()
            .map(|&i| faces[i].stretch)
            .filter(|&s| s <= requested)
            .max()
            .or_else(|| {
                indices
                    .iter()
                    .map(|&i| faces[i].stretch)
                    .filter(|&s| s > requested)
                    .min()
            })
    } else {
        // Prefer at-or-above ascending, then below descending.
        indices
            .iter()
            .map(|&i| faces[i].stretch)
            .filter(|&s| s >= requested)
            .min()
            .or_else(|| {
                indices
                    .iter()
                    .map(|&i| faces[i].stretch)
                    .filter(|&s| s < requested)
                    .max()
            })
    };

    match best_stretch {
        Some(target) => indices
            .into_iter()
            .filter(|&i| faces[i].stretch == target)
            .collect(),
        None => indices,
    }
}

/// CSS §4.5.4 style narrowing.
///
/// - Italic requested: keep italic/oblique faces; if none, keep all.
/// - Normal requested: keep non-italic faces; if none, keep all.
///
/// Uses `SmallVec` to avoid heap allocation for families with ≤16 faces.
fn narrow_by_style(
    indices: SmallVec<[usize; FACE_VEC_INLINE]>,
    faces: &[FaceInfo],
    italic: bool,
) -> SmallVec<[usize; FACE_VEC_INLINE]> {
    let preferred: SmallVec<[usize; FACE_VEC_INLINE]> = if italic {
        indices
            .iter()
            .copied()
            .filter(|&i| faces[i].italic)
            .collect()
    } else {
        indices
            .iter()
            .copied()
            .filter(|&i| !faces[i].italic)
            .collect()
    };

    if preferred.is_empty() {
        indices
    } else {
        preferred
    }
}
