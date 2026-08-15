//! Microbenchmarks for the hottest generated regex validators.
//!
//! `validate_regex_8` (`^([a-zA-Z][a-zA-Z0-9_]*)$`, Autosar Identifier) and `validate_regex_24`
//! (`^(/?[a-zA-Z][a-zA-Z0-9_]{0,127}(/[a-zA-Z][a-zA-Z0-9_]{0,127})*)$`, Autosar path reference)
//! together account for ~6% of the runtime of the `load_file` benchmark, so they are worth
//! optimizing individually. `validate_regex_7` (`^([a-zA-Z_][a-zA-Z0-9_]*)$`) is not hot, but it
//! validates the same character classes and is optimized the same way, so it is measured too.
//!
//! The validators are `pub(crate)`, so the module is included into the benchmark binary directly
//! instead of going through the crate's public API. The benchmark also contains alternative
//! implementations, which are compared against the ones in `src/regex.rs`.

extern crate alloc;

#[allow(dead_code, unused_imports)]
#[path = "../src/regex.rs"]
mod regex;

#[path = "common/candidates.rs"]
mod candidates;
#[path = "common/cases.rs"]
mod cases;
#[path = "common/corpus.rs"]
mod corpus;
#[path = "common/verify.rs"]
mod verify;

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// run every variant of one validator over one corpus
fn bench_variants(c: &mut Criterion, name: &str, corpus: &[String], variants: &[(&str, candidates::Validator)]) {
    let data: Vec<&[u8]> = corpus.iter().map(|s| s.as_bytes()).collect();
    let bytes: usize = data.iter().map(|s| s.len()).sum();

    let mut group = c.benchmark_group(name);
    group.throughput(criterion::Throughput::Bytes(bytes as u64));
    for (variant, func) in variants {
        group.bench_function(*variant, |b| {
            b.iter(|| {
                let mut count = 0usize;
                for s in &data {
                    count += usize::from(func(black_box(s)));
                }
                count
            });
        });
    }
    group.finish();
}

/// every validator over the samples of its pattern, so that a rewrite of any of them can be
/// compared against the previous version
fn bench_patterns(c: &mut Criterion) {
    if std::env::var_os("AUTOSAR_REGEX_SKIP_VERIFY").is_none() {
        verify::verify();
    }

    let mut group = c.benchmark_group("patterns");
    for case in cases::CASES {
        // the samples are short, so each one is validated 20 times per iteration to keep the
        // measurement well above the timer resolution
        let data: Vec<&[u8]> = case.samples.iter().map(|s| s.as_bytes()).collect();
        let bytes: usize = data.iter().map(|s| s.len()).sum::<usize>() * 20;
        group.throughput(criterion::Throughput::Bytes(bytes as u64));
        group.bench_function(case.name, |b| {
            b.iter(|| {
                let mut count = 0usize;
                for _ in 0..20 {
                    for s in &data {
                        count += usize::from((case.validate)(black_box(s)));
                    }
                }
                count
            });
        });
    }
    group.finish();
}

fn bench_regex_7(c: &mut Criterion) {
    bench_variants(c, "regex_7", &corpus::identifiers(), candidates::REGEX_7_VARIANTS);
}

fn bench_regex_22(c: &mut Criterion) {
    bench_variants(c, "regex_22", &corpus::identifiers(), candidates::REGEX_22_VARIANTS);
}

fn bench_regex_8(c: &mut Criterion) {
    bench_variants(c, "regex_8", &corpus::identifiers(), candidates::REGEX_8_VARIANTS);
}

fn bench_regex_24(c: &mut Criterion) {
    bench_variants(c, "regex_24", &corpus::paths(), candidates::REGEX_24_VARIANTS);
}

/// the mix of inputs that a real file load produces: mostly short identifiers, plus paths
fn bench_mixed(c: &mut Criterion) {
    let idents = corpus::identifiers();
    let paths = corpus::paths();
    let ident_data: Vec<&[u8]> = idents.iter().map(|s| s.as_bytes()).collect();
    let path_data: Vec<&[u8]> = paths.iter().map(|s| s.as_bytes()).collect();
    let bytes: usize =
        ident_data.iter().map(|s| s.len()).sum::<usize>() + path_data.iter().map(|s| s.len()).sum::<usize>();

    let mut group = c.benchmark_group("mixed");
    group.throughput(criterion::Throughput::Bytes(bytes as u64));
    for (name, f8, f24) in candidates::COMBINED_VARIANTS {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut count = 0usize;
                for s in &ident_data {
                    count += usize::from(f8(black_box(s)));
                }
                for s in &path_data {
                    count += usize::from(f24(black_box(s)));
                }
                count
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_patterns,
    bench_regex_7,
    bench_regex_8,
    bench_regex_22,
    bench_regex_24,
    bench_mixed
);
criterion_main!(benches);
