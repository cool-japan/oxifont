use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_webfont::{decode_woff2, encode_woff2};
use std::hint::black_box;

static TTF_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn bench_woff2_decode(c: &mut Criterion) {
    // Encode once in setup so we only measure decode time.
    let woff2_bytes = encode_woff2(TTF_BYTES).expect("encode_woff2 failed in bench setup");

    c.bench_function("decode_woff2", |b| {
        b.iter(|| {
            let _ =
                decode_woff2(black_box(&woff2_bytes)).expect("decode_woff2 failed during bench");
        });
    });
}

criterion_group!(benches, bench_woff2_decode);
criterion_main!(benches);
