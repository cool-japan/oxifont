//! Comparative benchmark: oxifont-webfont vs woff2-patched (decode) and
//! ttf2woff2 (encode).
//!
//! Run with:
//!   cargo bench --bench woff2_compare --features woff2 -p oxifont-webfont

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

static TTF_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Decode comparison
// ---------------------------------------------------------------------------

fn bench_decode_compare(c: &mut Criterion) {
    // Pre-encode the fixture with oxifont-webfont so both decoders read the
    // same WOFF2 byte stream.  If encoding fails (e.g. brotli upstream
    // limitation on this build), skip gracefully.
    let woff2_bytes = match oxifont_webfont::encode_woff2(TTF_BYTES) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP woff2_compare decode bench: encode failed: {e:?}");
            return;
        }
    };

    // Verify both decoders can handle this input before timing them.
    if oxifont_webfont::decode_woff2(&woff2_bytes).is_err() {
        eprintln!("SKIP woff2_compare decode bench: oxifont decode_woff2 returned Err");
        return;
    }
    {
        let mut buf = Bytes::copy_from_slice(&woff2_bytes);
        if woff2_patched::decode::convert_woff2_to_ttf(&mut buf).is_err() {
            eprintln!("SKIP woff2_compare decode bench: woff2-patched returned Err");
            return;
        }
    }

    let mut group = c.benchmark_group("woff2_decode_compare");

    // oxifont-webfont one-shot decoder.
    group.bench_with_input(
        BenchmarkId::new("oxifont_webfont", "decode_woff2"),
        &woff2_bytes,
        |b, bytes| {
            b.iter(|| {
                oxifont_webfont::decode_woff2(black_box(bytes))
                    .expect("oxifont decode_woff2 must succeed during bench")
            })
        },
    );

    // oxifont-webfont streaming decoder (reader-based).
    group.bench_with_input(
        BenchmarkId::new("oxifont_webfont", "decode_woff2_streaming"),
        &woff2_bytes,
        |b, bytes| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(black_box(bytes.as_slice()));
                oxifont_webfont::decode_woff2_streaming(cursor)
                    .expect("oxifont decode_woff2_streaming must succeed during bench")
            })
        },
    );

    // woff2-patched decoder.
    group.bench_with_input(
        BenchmarkId::new("woff2_patched", "convert_woff2_to_ttf"),
        &woff2_bytes,
        |b, bytes| {
            b.iter(|| {
                let mut buf = Bytes::copy_from_slice(black_box(bytes.as_slice()));
                woff2_patched::decode::convert_woff2_to_ttf(&mut buf)
                    .expect("woff2-patched must succeed during bench")
            })
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Encode comparison
// ---------------------------------------------------------------------------

fn bench_encode_compare(c: &mut Criterion) {
    // Verify both encoders work before timing.
    if oxifont_webfont::encode_woff2(TTF_BYTES).is_err() {
        eprintln!("SKIP woff2_compare encode bench: oxifont encode_woff2 returned Err");
        return;
    }
    let quality = ttf2woff2::BrotliQuality::default();
    if ttf2woff2::encode(TTF_BYTES, quality).is_err() {
        eprintln!("SKIP woff2_compare encode bench: ttf2woff2::encode returned Err");
        return;
    }

    let mut group = c.benchmark_group("woff2_encode_compare");

    // oxifont-webfont encoder.
    group.bench_with_input(
        BenchmarkId::new("oxifont_webfont", "encode_woff2"),
        TTF_BYTES,
        |b, bytes| {
            b.iter(|| {
                oxifont_webfont::encode_woff2(black_box(bytes))
                    .expect("oxifont encode_woff2 must succeed during bench")
            })
        },
    );

    // ttf2woff2 encoder (pure Rust, quality-11 by default).
    group.bench_with_input(
        BenchmarkId::new("ttf2woff2", "encode"),
        TTF_BYTES,
        |b, bytes| {
            let q = ttf2woff2::BrotliQuality::default();
            b.iter(|| {
                ttf2woff2::encode(black_box(bytes), q)
                    .expect("ttf2woff2::encode must succeed during bench")
            })
        },
    );

    group.finish();
}

criterion_group!(benches, bench_decode_compare, bench_encode_compare);
criterion_main!(benches);
