use criterion::{criterion_group, criterion_main, Criterion};
use std::io::Cursor;

static TTF_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn bench_woff2_streaming(c: &mut Criterion) {
    // Encode once; if brotli can't decode this fixture, skip the bench.
    let woff2_bytes = match oxifont_webfont::encode_woff2(TTF_BYTES) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP woff2_streaming bench: encode failed: {e:?}");
            return;
        }
    };

    // Verify decode works before benchmarking (known oxiarc-brotli limitation on large inputs).
    if oxifont_webfont::decode_woff2(&woff2_bytes).is_err() {
        eprintln!(
            "SKIP woff2_streaming bench: decode_woff2 returned Err (known brotli limitation)"
        );
        return;
    }

    let mut group = c.benchmark_group("woff2_decode");
    group.bench_function("one_shot", |b| {
        b.iter(|| {
            oxifont_webfont::decode_woff2(&woff2_bytes)
                .expect("decode_woff2 must succeed during bench")
        })
    });
    group.bench_function("streaming", |b| {
        b.iter(|| {
            let cursor = Cursor::new(&woff2_bytes);
            oxifont_webfont::decode_woff2_streaming(cursor)
                .expect("decode_woff2_streaming must succeed during bench")
        })
    });
    group.finish();
}

criterion_group!(benches, bench_woff2_streaming);
criterion_main!(benches);
