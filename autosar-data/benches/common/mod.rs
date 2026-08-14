//! Shared input handling for the benchmarks that operate on caller supplied arxml files.
//!
//! In order to allow the use of files that are not test fixtures in the benchmarks, the files
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
//!    (`:` on unix, `;` on windows). If this is not set, the benchmarks do nothing.
//!  - `AUTOSAR_BENCH_STRICT`: set to `1` / `true` / `yes` / `on` to load with strict = true.
//!    The default is non-strict loading, so that files with recoverable problems can be measured.
//!  - `AUTOSAR_BENCH_SAMPLES`: number of samples per file (minimum and default: 10). Working with
//!    a large file takes a long time, so criterion's default of 100 samples is not useful here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const FILES_VAR: &str = "AUTOSAR_BENCH_FILES";
pub const STRICT_VAR: &str = "AUTOSAR_BENCH_STRICT";
pub const SAMPLES_VAR: &str = "AUTOSAR_BENCH_SAMPLES";

pub const DEFAULT_SAMPLES: usize = 10;

/// get the list of files to benchmark from the environment
pub fn get_input_files() -> Vec<PathBuf> {
    let Some(value) = std::env::var_os(FILES_VAR) else {
        return Vec::new();
    };
    std::env::split_paths(&value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

/// get a boolean setting from the environment
pub fn get_env_flag(name: &str, default: bool) -> bool {
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
pub fn get_sample_count() -> usize {
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
pub fn benchmark_names(files: &[PathBuf]) -> Vec<String> {
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
        names.push(if *count == 1 { base } else { format!("{base} ({count})") });
    }
    names
}

/// print the message that explains how to give input files to the benchmark
pub fn print_usage(bench_name: &str) {
    println!(
        "{bench_name}: no input files.\n\
         Set {FILES_VAR} to one or more arxml file names (separated by \"{}\") to run this benchmark, e.g.\n    \
         {FILES_VAR}=/path/to/big.arxml cargo bench -p autosar-data --bench {bench_name}\n\
         Optional settings: {STRICT_VAR} (default: false), {SAMPLES_VAR} (default: {DEFAULT_SAMPLES})",
        if cfg!(windows) { ";" } else { ":" }
    );
}
