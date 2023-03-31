use criterion::{criterion_group, criterion_main, Criterion};

fn all_benchmarks(c: &mut Criterion) {
    c.bench_function("test", |b| {
        b.iter(|| print!("test"))
    });
}
criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(2000);
    targets = all_benchmarks
);
criterion_main!(benches);

