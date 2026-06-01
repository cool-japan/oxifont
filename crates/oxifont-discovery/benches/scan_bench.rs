use criterion::{criterion_group, criterion_main, Criterion};
use oxifont_discovery::{read_face_metadata_partial, scan_dirs, scan_dirs_metadata_only};
use std::hint::black_box;
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../oxifont-parser/tests/fixtures/test.ttf")
        .canonicalize()
        .expect("test fixture must exist at crates/oxifont-parser/tests/fixtures/test.ttf")
}

fn setup_temp_dir(fixture: &std::path::Path) -> PathBuf {
    let dir = std::env::temp_dir().join("oxifont_discovery_bench");
    std::fs::create_dir_all(&dir).expect("temp dir creation must succeed");
    std::fs::copy(fixture, dir.join("test.ttf")).expect("fixture copy must succeed");
    dir
}

fn bench_scan(c: &mut Criterion) {
    let fixture = fixture_path();
    let temp_dir = setup_temp_dir(&fixture);

    // Bench: read_face_metadata_partial (lazy — reads only 6 small tables)
    c.bench_function("read_face_metadata_partial", |b| {
        b.iter(|| {
            read_face_metadata_partial(black_box(&fixture)).expect("partial read must succeed")
        });
    });

    // Bench: scan_dirs_metadata_only (lazy scan of a single-file directory)
    let dirs = vec![temp_dir];
    c.bench_function("scan_dirs_metadata_only_single_file", |b| {
        b.iter(|| scan_dirs_metadata_only(black_box(&dirs)));
    });

    // Bench: scan_dirs full (eager parse — reads and parses all font data)
    c.bench_function("scan_dirs_full_single_file", |b| {
        b.iter(|| scan_dirs(black_box(dirs.as_slice())));
    });
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
