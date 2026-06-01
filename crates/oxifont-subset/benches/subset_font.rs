use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_subset::subset_font;
use std::collections::BTreeSet;
use std::hint::black_box;

static TTF_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn make_codepoints_100() -> BTreeSet<char> {
    (0x20u32..=0x7F).filter_map(char::from_u32).collect()
}

fn make_codepoints_500() -> BTreeSet<char> {
    (0x20u32..=0x27F).filter_map(char::from_u32).collect()
}

fn bench_subset(c: &mut Criterion) {
    let cps_100 = make_codepoints_100();
    let cps_500 = make_codepoints_500();

    c.bench_function("subset_font_100_codepoints", |b| {
        b.iter(|| {
            let _ = subset_font(black_box(TTF_BYTES), black_box(&cps_100));
        });
    });

    c.bench_function("subset_font_500_codepoints", |b| {
        b.iter(|| {
            let _ = subset_font(black_box(TTF_BYTES), black_box(&cps_500));
        });
    });
}

criterion_group!(benches, bench_subset);
criterion_main!(benches);
