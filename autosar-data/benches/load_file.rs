//! Benchmark the loading (parsing) of arxml files.
//!
//! The files to benchmark are given by the caller in environment variables; see `common` for the
//! description of these variables:
//!
//! ```sh
//! AUTOSAR_BENCH_FILES=/path/to/big.arxml cargo bench -p autosar-data --bench load_file
//! ```
//!
//! The file content is read into memory once, outside of the measurement, so that the benchmark
//! measures parsing and model building rather than disk IO. Each iteration parses the buffer into
//! a fresh `AutosarModel`; creation of the empty model and destruction of the loaded model both
//! happen outside of the measured section.

mod common;

use autosar_data::AutosarModel;
use common::{STRICT_VAR, benchmark_names, get_env_flag, get_input_files, get_sample_count, print_usage};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::path::Path;
use std::time::Duration;

/// load the file once to verify that it can be loaded, and to display some information about it
fn describe_file(buffer: &[u8], path: &Path, strict: bool) -> bool {
    let model = AutosarModel::new();
    match model.load_buffer(buffer, path, strict) {
        Ok((file, warnings)) => {
            let element_count = model.elements_dfs().count();
            let identifiable_count = model.identifiable_elements().count();
            println!(
                "{}: {} bytes, {}, {element_count} elements, {identifiable_count} identifiables, {} warnings",
                path.display(),
                buffer.len(),
                file.version(),
                warnings.len()
            );
            true
        }
        Err(error) => {
            eprintln!("{}: could not be loaded, skipping it: {error}", path.display());
            false
        }
    }
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let files = get_input_files();
    if files.is_empty() {
        print_usage("load_file");
        return;
    }
    let strict = get_env_flag(STRICT_VAR, false);
    let samples = get_sample_count();
    let names = benchmark_names(&files);

    let mut group = c.benchmark_group("load_file");
    group
        .sample_size(samples)
        // large files can take seconds per iteration; don't let criterion try to fit 5 seconds
        // of measurement into the configured number of samples
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));

    for (path, name) in files.iter().zip(names) {
        let buffer = match std::fs::read(path) {
            Ok(buffer) => buffer,
            Err(error) => {
                eprintln!("{}: could not be read, skipping it: {error}", path.display());
                continue;
            }
        };

        if !describe_file(&buffer, path, strict) {
            continue;
        }

        group.throughput(Throughput::Bytes(buffer.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(&name), |bencher| {
            bencher.iter_batched(
                AutosarModel::new,
                |model| {
                    let result = model.load_buffer(&buffer, path, strict);
                    // the model is returned so that it is dropped outside of the measured section
                    (model, result)
                },
                // one iteration per batch: each loaded model may use a lot of memory, so
                // collecting a whole batch of them before dropping them is not an option
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
