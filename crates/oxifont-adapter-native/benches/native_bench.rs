use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pure_database_system(c: &mut Criterion) {
    c.bench_function("pure_database_system", |b| {
        b.iter(|| {
            oxifont_adapter_pure::FontDatabase::system().expect("system font scan must succeed")
        });
    });
}

#[cfg(target_os = "macos")]
fn bench_native_catalog_system(c: &mut Criterion) {
    use oxifont_adapter_native::NativeCatalog;
    c.bench_function("native_catalog_system_macos", |b| {
        b.iter(|| NativeCatalog::system().expect("CoreText enumeration must succeed"));
    });
}

#[cfg(target_os = "macos")]
criterion_group!(native_benches, bench_native_catalog_system);
criterion_group!(pure_benches, bench_pure_database_system);

#[cfg(target_os = "macos")]
criterion_main!(native_benches, pure_benches);

#[cfg(not(target_os = "macos"))]
criterion_main!(pure_benches);
