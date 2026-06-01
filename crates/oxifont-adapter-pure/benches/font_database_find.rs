use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FaceInfo, FontCatalog as _, FontQuery, FontStretch, FontStyle};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;

/// Build a synthetic Vec of 5000 FaceInfo entries with varying family names and weights.
fn make_face_infos() -> Vec<FaceInfo> {
    let weights = [100u16, 400, 700];
    (0..5000)
        .map(|i| {
            let weight = weights[i % weights.len()];
            FaceInfo {
                family: Arc::from(format!("Family{}", i / weights.len()).as_str()),
                post_script_name: format!("Family{}-W{}", i / weights.len(), weight),
                style: FontStyle::Normal,
                weight,
                stretch: FontStretch::Normal,
                path: PathBuf::from("/dev/null"),
                face_index: 0,
                localized_families: Vec::new(),
            }
        })
        .collect()
}

fn bench_find(c: &mut Criterion) {
    let faces = make_face_infos();
    let db = FontDatabase::from_faces(faces.clone());

    // Bench: exact-hit query (family "Family833", weight 400).
    // Family833 = index 2499 in the 5000-entry range (i=2499, i/3=833, weight=weights[0]=100; i=2500 → 100; family833 at i=2499,2500,2501)
    // Actually Family833 is at indices 2499..2501 (i / 3 == 833 when i in [2499..2502]).
    // We search for weight 400 which is the second of the three (i%3==1 => weight=400).
    let hit_query = FontQuery::new().family("Family833").weight(400);
    c.bench_function("find_exact_hit", |b| {
        b.iter(|| {
            let _ = db.find(black_box(&hit_query));
        });
    });

    // Bench: miss query (family not in database).
    let miss_query = FontQuery::new().family("NonExistent");
    c.bench_function("find_miss", |b| {
        b.iter(|| {
            let _ = db.find(black_box(&miss_query));
        });
    });

    // Bench: FontDatabase construction from 5000 FaceInfo records.
    c.bench_function("font_database_construction_5000", |b| {
        b.iter(|| {
            let _ = FontDatabase::from_faces(black_box(faces.clone()));
        });
    });
}

criterion_group!(benches, bench_find);
criterion_main!(benches);
