use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_db::{FaceInfo, FontDatabase, Query, Source};
use std::hint::black_box;
use std::path::PathBuf;

/// Build a synthetic database with `n` faces spread across multiple families.
///
/// Families are named `Family0`, `Family1`, … and each family contains three
/// faces at weights 100, 400, and 700.  Even-indexed faces (i % 4 == 0) are
/// italic so that italic matching is exercised realistically.
fn make_db(n: usize) -> FontDatabase {
    let mut db = FontDatabase::new();
    let weights = [100u16, 400, 700];
    for i in 0..n {
        let weight = weights[i % weights.len()];
        let group = i / weights.len();
        db.add_face(FaceInfo {
            id: 0, // overwritten by add_face
            family: format!("Family{group}"),
            post_script_name: format!("Family{group}-W{weight}"),
            weight,
            italic: i % 4 == 0,
            stretch: 5,
            monospaced: false,
            source: Source::File(PathBuf::from("/dev/null")),
            face_index: 0,
            variable_axes: Vec::new(),
            locale_families: Vec::new(),
            unicode_ranges: 0,
        });
    }
    db
}

fn bench_match_best(c: &mut Criterion) {
    let db = make_db(5000);

    // With weights.len() == 3 and 5000 faces:
    //   group 833 = i / 3 == 833  →  faces at indices 2499, 2500, 2501
    //   weight 400 is at i % 3 == 1  →  i == 2500, group == 833
    let hit_family = "Family833";

    c.bench_function("match_best_hit_5000", |b| {
        b.iter(|| {
            Query::new(black_box(&db))
                .family(hit_family)
                .weight(400)
                .italic(false)
                .match_best()
        });
    });

    c.bench_function("match_best_miss_5000", |b| {
        b.iter(|| {
            Query::new(black_box(&db))
                .family("NonExistentFamilyXYZ")
                .match_best()
        });
    });

    c.bench_function("db_construction_5000", |b| {
        b.iter(|| make_db(black_box(5000)));
    });
}

criterion_group!(benches, bench_match_best);
criterion_main!(benches);
