//! Benchmark the loading (parsing) of arxml files.
//!
//! In order to allow the use of files that are not test fixtures in the benchmark, the files
//! to benchmark are given by the caller.
//! Criterion consumes the command line of the benchmark binary itself (a positional argument
//! is treated as a benchmark filter), so the input is passed in environment variables instead:
//!
//! ```sh
//! AUTOSAR_BENCH_FILES=/path/to/big.arxml cargo bench -p autosar-data --bench load_file
//! ```
//!
//! Environment variables:
//!
//!  - `AUTOSAR_BENCH_FILES`: one or more file names, separated by the platform path separator
//!    (`:` on unix, `;` on windows). If this is not set, the benchmark does nothing.
//!  - `AUTOSAR_BENCH_STRICT`: set to `1` / `true` / `yes` / `on` to load with strict = true.
//!    The default is non-strict loading, so that files with recoverable problems can be measured.
//!  - `AUTOSAR_BENCH_SAMPLES`: number of samples per file (minimum and default: 10). Loading a
//!    large file takes a long time, so criterion's default of 100 samples is not useful here.
//!
//! The file content is read into memory once, outside of the measurement, so that the benchmark
//! measures parsing and model building rather than disk IO. Each iteration parses the buffer into
//! a fresh `AutosarModel`; creation of the empty model and destruction of the loaded model both
//! happen outside of the measured section.

use autosar_data::AutosarModel;
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILES_VAR: &str = "AUTOSAR_BENCH_FILES";
const STRICT_VAR: &str = "AUTOSAR_BENCH_STRICT";
const SAMPLES_VAR: &str = "AUTOSAR_BENCH_SAMPLES";

const DEFAULT_SAMPLES: usize = 10;

/// get the list of files to benchmark from the environment
fn get_input_files() -> Vec<PathBuf> {
    let Some(value) = std::env::var_os(FILES_VAR) else {
        return Vec::new();
    };
    std::env::split_paths(&value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

/// get a boolean setting from the environment
fn get_env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            other => {
                eprintln!("{name}: can't interpret \"{other}\" as a boolean, using {default}");
                default
            }
        },
        Err(_) => default,
    }
}

/// get the number of samples per file from the environment
fn get_sample_count() -> usize {
    match std::env::var(SAMPLES_VAR) {
        // criterion requires at least 10 samples
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(count) => count.max(DEFAULT_SAMPLES),
            Err(error) => {
                eprintln!("{SAMPLES_VAR}: \"{value}\" is not a valid number ({error}), using {DEFAULT_SAMPLES}");
                DEFAULT_SAMPLES
            }
        },
        Err(_) => DEFAULT_SAMPLES,
    }
}

/// build a short, unique, human readable benchmark id for each input file
///
/// Normally the file name is used. If several input files have the same name in different
/// directories, then the name of the parent directory is prepended, and as a last resort a
/// counter is appended. Full paths are not used, because criterion truncates long ids for
/// display, which would make files in deeply nested directories indistinguishable.
fn benchmark_names(files: &[PathBuf]) -> Vec<String> {
    let file_names: Vec<String> = files
        .iter()
        .map(|file| {
            file.file_name()
                .unwrap_or(file.as_os_str())
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    let mut names = Vec::with_capacity(files.len());
    let mut used: HashMap<String, usize> = HashMap::new();
    for (file, file_name) in files.iter().zip(&file_names) {
        let is_ambiguous = file_names.iter().filter(|name| *name == file_name).count() > 1;
        let base = match file.parent().and_then(Path::file_name) {
            Some(dir) if is_ambiguous => format!("{}/{file_name}", dir.to_string_lossy()),
            _ => file_name.clone(),
        };
        // guarantee uniqueness, even if the parent directory names are identical as well
        let count = used.entry(base.clone()).or_default();
        *count += 1;
        names.push(if *count == 1 {
            base
        } else {
            format!("{base} ({count})")
        });
    }
    names
}

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
        println!(
            "load_file: no input files.\n\
             Set {FILES_VAR} to one or more arxml file names (separated by \"{}\") to run this benchmark, e.g.\n    \
             {FILES_VAR}=/path/to/big.arxml cargo bench -p autosar-data --bench load_file\n\
             Optional settings: {STRICT_VAR} (default: false), {SAMPLES_VAR} (default: {DEFAULT_SAMPLES})",
            if cfg!(windows) { ";" } else { ":" }
        );
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
