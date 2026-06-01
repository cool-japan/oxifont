use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_parser::ParsedFace;
use std::hint::black_box;
use std::sync::Arc;

static FIXTURE: &[u8] = include_bytes!("../tests/fixtures/test.ttf");

fn bench_parse_face(c: &mut Criterion) {
    // Pre-allocate Arc<[u8]> once outside the loop — cloning is O(1) atomic increment.
    // We're benchmarking the parse cost, not the Arc allocation.
    let bytes: Arc<[u8]> = Arc::from(FIXTURE);

    c.bench_function("parse_face_truetype", |b| {
        b.iter(|| {
            ParsedFace::parse(black_box(bytes.clone()), 0)
                .expect("test fixture must parse successfully")
        });
    });
}

criterion_group!(benches, bench_parse_face);
criterion_main!(benches);
