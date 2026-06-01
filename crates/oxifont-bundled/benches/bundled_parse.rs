use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

#[cfg(feature = "bundled-noto")]
fn bench_bundled_parse(c: &mut Criterion) {
    use oxifont_bundled::{NOTO_SANS_REGULAR, SANS_REGULAR};
    use std::sync::Arc;

    // Pre-allocate Arc<[u8]> once — cloning is O(1) atomic increment
    let bytes: Arc<[u8]> = Arc::from(NOTO_SANS_REGULAR as &[u8]);

    // Cold-start: clone Arc (O(1)) then parse font from scratch
    c.bench_function("bundled_cold_parse", |b| {
        b.iter(|| {
            oxifont_parser::ParsedFace::parse(black_box(bytes.clone()), 0)
                .expect("fixture must parse")
        });
    });

    // Warm cache: OnceLock already populated, subsequent calls just clone Arc
    let _ = SANS_REGULAR
        .parsed_face()
        .expect("initial warm-up must succeed");
    c.bench_function("bundled_warm_parsed_face", |b| {
        b.iter(|| {
            black_box(&SANS_REGULAR)
                .parsed_face()
                .expect("cached face must succeed")
        });
    });
}

#[cfg(not(feature = "bundled-noto"))]
fn bench_bundled_parse(_c: &mut Criterion) {}

criterion_group!(benches, bench_bundled_parse);
criterion_main!(benches);
