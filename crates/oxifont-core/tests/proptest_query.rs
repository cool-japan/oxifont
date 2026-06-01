//! Property-based tests for `FontQuery` builder and CSS matching invariants.
//!
//! Uses `proptest` to verify that invariants hold across all valid CSS input
//! combinations, complementing the hand-picked cases in `query_tests.rs`.

use oxifont_core::{FontQuery, FontStretch, FontStyle};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Any valid CSS font-weight value (100–900 inclusive, all integers).
fn weight_strategy() -> impl Strategy<Value = u16> {
    (1u16..=9).prop_map(|n| n * 100)
}

/// Any `FontStyle` variant.
fn style_strategy() -> impl Strategy<Value = FontStyle> {
    prop_oneof![
        Just(FontStyle::Normal),
        Just(FontStyle::Italic),
        Just(FontStyle::Oblique),
    ]
}

/// Any `FontStretch` variant, via its numeric width class (1–9).
fn stretch_strategy() -> impl Strategy<Value = FontStretch> {
    (1u8..=9).prop_map(FontStretch::from_width_class)
}

// ---------------------------------------------------------------------------
// FontQuery builder invariants
// ---------------------------------------------------------------------------

proptest! {
    /// Setting `.weight(w)` yields `Some(w)` in the query regardless of value.
    #[test]
    fn query_builder_weight_roundtrip(w in weight_strategy()) {
        let q = FontQuery::new().weight(w);
        prop_assert_eq!(q.weight, Some(w));
    }

    /// Setting `.style(s)` yields `Some(s)` in the query regardless of variant.
    #[test]
    fn query_builder_style_roundtrip(s in style_strategy()) {
        let q = FontQuery::new().style(s.clone());
        prop_assert_eq!(q.style, Some(s));
    }

    /// Setting `.stretch(st)` yields `Some(st)` in the query regardless of variant.
    #[test]
    fn query_builder_stretch_roundtrip(st in stretch_strategy()) {
        let q = FontQuery::new().stretch(st);
        prop_assert_eq!(q.stretch, Some(st));
    }

    /// Calling `.weight()` twice keeps the last value — builder is overwrite-style.
    #[test]
    fn query_builder_weight_overwrite(w1 in weight_strategy(), w2 in weight_strategy()) {
        let q = FontQuery::new().weight(w1).weight(w2);
        prop_assert_eq!(q.weight, Some(w2));
    }

    /// Calling `.style()` twice keeps the last value.
    #[test]
    fn query_builder_style_overwrite(s1 in style_strategy(), s2 in style_strategy()) {
        let q = FontQuery::new().style(s1).style(s2.clone());
        prop_assert_eq!(q.style, Some(s2));
    }

    /// Calling `.stretch()` twice keeps the last value.
    #[test]
    fn query_builder_stretch_overwrite(
        st1 in stretch_strategy(),
        st2 in stretch_strategy(),
    ) {
        let q = FontQuery::new().stretch(st1).stretch(st2);
        prop_assert_eq!(q.stretch, Some(st2));
    }

    /// A cloned query is independent: mutating the clone does not affect the original.
    #[test]
    fn query_clone_independence(
        w in weight_strategy(),
        s in style_strategy(),
        st in stretch_strategy(),
    ) {
        let q1 = FontQuery::new()
            .weight(w)
            .style(s.clone())
            .stretch(st);
        let mut q2 = q1.clone();
        q2.weight = Some(0xFFFF); // mutate clone
        // Original must still hold the original weight.
        prop_assert_eq!(q1.weight, Some(w));
    }

    /// Any combination of valid CSS values must not panic and must preserve
    /// each field independently.
    #[test]
    fn query_builder_all_fields_no_panic(
        w in weight_strategy(),
        s in style_strategy(),
        st in stretch_strategy(),
        family in "[A-Za-z ]{1,30}",
    ) {
        let q = FontQuery::new()
            .family(family.clone())
            .weight(w)
            .style(s.clone())
            .stretch(st);

        prop_assert_eq!(q.weight, Some(w));
        prop_assert_eq!(q.style, Some(s));
        prop_assert_eq!(q.stretch, Some(st));
        prop_assert_eq!(q.family.as_deref(), Some(family.as_str()));
    }
}

// ---------------------------------------------------------------------------
// FontStyle::css_preference_score invariants
// ---------------------------------------------------------------------------

proptest! {
    /// An exact match always scores strictly higher than any non-matching available style.
    ///
    /// This encodes the primary CSS invariant: the requested style is always
    /// preferred over every alternative.
    #[test]
    fn css_score_exact_match_beats_non_match(
        requested in style_strategy(),
        available in style_strategy(),
    ) {
        prop_assume!(requested != available);
        let exact   = FontStyle::css_preference_score(requested.clone(), requested.clone());
        let partial = FontStyle::css_preference_score(requested.clone(), available);
        prop_assert!(exact > partial,
            "exact match score ({exact}) must exceed non-match score ({partial})");
    }

    /// Exact match score is constant across all style variants.
    ///
    /// I.e. `score(R, R)` must be the same integer regardless of which R is chosen.
    #[test]
    fn css_score_exact_match_is_constant(
        r1 in style_strategy(),
        r2 in style_strategy(),
    ) {
        let s1 = FontStyle::css_preference_score(r1.clone(), r1);
        let s2 = FontStyle::css_preference_score(r2.clone(), r2);
        prop_assert_eq!(s1, s2,
            "score(R, R) must be identical for every R");
    }

    /// All three score values for a given requested style are distinct.
    ///
    /// The specification demands a total order among the three available choices;
    /// ties would make the ordering ambiguous.
    #[test]
    fn css_score_no_ties_for_any_requested(requested in style_strategy()) {
        let score_normal  = FontStyle::css_preference_score(requested.clone(), FontStyle::Normal);
        let score_italic  = FontStyle::css_preference_score(requested.clone(), FontStyle::Italic);
        let score_oblique = FontStyle::css_preference_score(requested.clone(), FontStyle::Oblique);

        prop_assert_ne!(score_normal,  score_italic);
        prop_assert_ne!(score_italic,  score_oblique);
        prop_assert_ne!(score_normal,  score_oblique);
    }

    /// All scores are within the bounded range [0, 2].
    ///
    /// The matching algorithm uses exactly three preference levels; values
    /// outside this range would indicate an implementation error.
    #[test]
    fn css_score_bounded(
        requested in style_strategy(),
        available in style_strategy(),
    ) {
        let score = FontStyle::css_preference_score(requested, available);
        prop_assert!((0..=2).contains(&score),
            "score must be in [0, 2], got {score}");
    }

    /// The three scores for any `requested` form a permutation of {0, 1, 2}.
    ///
    /// This guarantees that the CSS algorithm imposes a *total* preference
    /// order with no gaps.
    #[test]
    fn css_score_permutation_of_zero_one_two(requested in style_strategy()) {
        let score_normal  = FontStyle::css_preference_score(requested.clone(), FontStyle::Normal);
        let score_italic  = FontStyle::css_preference_score(requested.clone(), FontStyle::Italic);
        let score_oblique = FontStyle::css_preference_score(requested.clone(), FontStyle::Oblique);

        let mut scores = [score_normal, score_italic, score_oblique];
        scores.sort_unstable();
        prop_assert_eq!(scores, [0, 1, 2],
            "scores must be a permutation of {{0, 1, 2}}");
    }
}
