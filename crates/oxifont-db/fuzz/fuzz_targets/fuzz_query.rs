//! Fuzz target: generate random `Query` parameters and run `match_best`
//! against a fixed synthetic `FontDatabase` with a diverse face set.
//!
//! The database is constructed once as a thread-local to amortise setup cost.
//! Query parameters (weight, stretch, italic, oblique, family index) are
//! derived from the fuzz input bytes to cover the full CSS weight-ordering
//! combinatorics, including non-standard weights (350, 380, 620, 650).
//!
//! Invariants verified:
//!   - `Query::match_best` never panics for any weight in 1..=1000.
//!   - `Query::match_all` never panics.
//!   - When a match is returned, its weight is in 1..=1000.
//!   - When a match is returned, its stretch value is in 1..=9.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oxifont_db::{FaceInfo, FontDatabase, Query, Source};
use std::cell::RefCell;
use std::path::PathBuf;

// Fixed family names that the fuzzer can reference by index.
const FAMILIES: &[&str] = &[
    "Test Sans",
    "Test Serif",
    "Test Mono",
    "sans-serif",
    "serif",
    "monospace",
    "cursive",
    "fantasy",
];

thread_local! {
    static DB: RefCell<FontDatabase> = RefCell::new(build_synthetic_db());
}

fn make_face(family: &str, weight: u16, italic: bool, stretch: u8, idx: u32) -> FaceInfo {
    FaceInfo {
        id: 0, // assigned by database
        family: family.to_string(),
        post_script_name: format!("{}-w{weight}", family.replace(' ', "")),
        weight,
        italic,
        stretch,
        monospaced: family == "Test Mono" || family == "monospace",
        source: Source::File(PathBuf::from(format!("/fake/{family}_{weight}.ttf"))),
        face_index: idx,
        variable_axes: vec![],
        locale_families: vec![],
        unicode_ranges: 0,
    }
}

fn build_synthetic_db() -> FontDatabase {
    let mut db = FontDatabase::new();
    let weights: &[u16] = &[100, 200, 300, 350, 400, 450, 500, 600, 700, 800, 900];
    let stretches: &[u8] = &[1, 3, 5, 7, 9]; // ultra-condensed, condensed, normal, expanded, ultra-expanded

    for (fi, family) in FAMILIES.iter().enumerate() {
        for (wi, &weight) in weights.iter().enumerate() {
            for &italic in &[false, true] {
                for &stretch in stretches {
                    let face = make_face(family, weight, italic, stretch, (fi * 100 + wi) as u32);
                    db.add_face(face);
                }
            }
        }
    }
    db
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // Derive query parameters from fuzz bytes.
    let weight = {
        let raw = u16::from_le_bytes([data[0], data[1]]);
        // Map to 1..=1000 inclusive, covering non-standard values.
        (raw % 1000) + 1
    };
    let family_idx = (data[2] as usize) % FAMILIES.len();
    let flags = data[3];
    let italic = flags & 0x01 != 0;
    let stretch_raw = ((flags >> 2) & 0x0F) % 9 + 1; // 1..=9

    DB.with(|db| {
        let db = db.borrow();
        let family = FAMILIES[family_idx];

        // match_best must never panic for any combination.
        let result = Query::new(&db)
            .family(family)
            .weight(weight)
            .italic(italic)
            .stretch(stretch_raw)
            .match_best();

        if let Some(face) = result {
            assert!(
                (1..=1000).contains(&face.weight),
                "returned face has out-of-range weight: {}",
                face.weight
            );
            assert!(
                (1..=9).contains(&face.stretch),
                "returned face has out-of-range stretch: {}",
                face.stretch
            );
        }

        // match_all must never panic.
        let all = Query::new(&db)
            .family(family)
            .weight(weight)
            .italic(italic)
            .match_all();

        for face in &all {
            assert!(
                (1..=1000).contains(&face.weight),
                "returned face has out-of-range weight: {}",
                face.weight
            );
        }

        // Zero-family query (match all families at this weight) must not panic.
        let _ = Query::new(&db).weight(weight).italic(italic).match_best();

        // Edge-case: weight=1 and weight=1000.
        let _ = Query::new(&db).family(family).weight(1).match_best();
        let _ = Query::new(&db).family(family).weight(1000).match_best();
    });
});
