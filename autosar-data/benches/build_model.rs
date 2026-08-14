//! Benchmark building a model from scratch through the element creation API.
//!
//! The files to benchmark are given by the caller in environment variables; see `common` for the
//! description of these variables. They are the same variables that the `load_file` benchmark uses:
//!
//! ```sh
//! AUTOSAR_BENCH_FILES=/path/to/big.arxml cargo bench -p autosar-data --bench build_model
//! ```
//!
//! Each input file is loaded into a source model once, outside of the measurement. The measured
//! section then builds a second model that is a twin of the source model: it creates a new
//! `AutosarModel` and a new `ArxmlFile` in it, iterates over all elements of the source model with
//! `elements_dfs()`, and re-creates each of them one by one with `create_sub_element()` /
//! `create_named_sub_element()`, followed by the attributes and character data of the element.
//!
//! This deliberately does not use `AutosarModel::duplicate()` or `create_copied_sub_element()`,
//! which copy whole element trees in one step: the point of the benchmark is to show where the
//! time goes when a large model is assembled element by element, as application code would do.
//! Consequently the result is not required to be a bit-exact copy of the input - e.g. file
//! membership of a split model is not reproduced, since everything is created in a single file.

mod common;

use autosar_data::{ArxmlFile, AutosarModel, AutosarVersion, ContentType, Element, ElementContent, ElementName};
use common::{STRICT_VAR, benchmark_names, get_env_flag, get_input_files, get_sample_count, print_usage};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::path::Path;
use std::time::Duration;

/// the result of building a twin model: the model itself, and the number of operations that failed
struct BuildResult {
    model: AutosarModel,
    elements: usize,
    errors: usize,
}

/// get the item name of an element by looking up its SHORT-NAME sub element
///
/// This is only used on the error path, where `Element::item_name()` has already come up empty.
fn short_name_of(element: &Element) -> Option<String> {
    element
        .get_sub_element(ElementName::ShortName)?
        .character_data()?
        .string_value()
}

/// build a twin of `source` from scratch, using only the element creation API
///
/// The elements of the source model are visited in depth first order, so each element is visited
/// after its parent. `twins[depth]` is the newly created counterpart of the source element at that
/// depth, which makes the parent of each new element available as `twins[depth - 1]`.
fn build_twin_model(source: &AutosarModel, filename: &Path, version: AutosarVersion) -> BuildResult {
    let model = AutosarModel::new();
    let _file: ArxmlFile = model
        .create_file(filename, version)
        .expect("creating a file in an empty model can't fail");

    let mut twins: Vec<Element> = Vec::new();
    // elements with mixed content are handled after the main loop, because their character data
    // items can only be inserted at the correct position once all their sub elements exist
    let mut mixed: Vec<(Element, Element)> = Vec::new();
    let mut elements = 0;
    let mut errors = 0;

    for (depth, element) in source.elements_dfs() {
        // discard the twins of the previously visited subtree: after this, twins.len() == depth
        twins.truncate(depth);
        elements += 1;

        if depth == 0 {
            // the root element of the new model already exists, it was created by create_file()
            twins.push(model.root_element());
            continue;
        }
        let parent = &twins[depth - 1];
        let element_name = element.element_name();

        // create the twin of the current element
        let mut reused = false;
        let created = if element_name == ElementName::ShortName
            && let Some(short_name) = parent.get_sub_element(ElementName::ShortName)
        {
            // the SHORT-NAME was already created together with its identifiable parent element
            reused = true;
            Ok(short_name)
        } else if let Some(item_name) = element.item_name() {
            parent.create_named_sub_element(element_name, &item_name)
        } else {
            parent.create_sub_element(element_name)
        };
        let twin = match created.or_else(|error| {
            // item_name() only considers the first content item, so an identifiable element whose
            // SHORT-NAME is preceded by anything else looks unnamed; retry with its actual name
            short_name_of(&element).map_or(Err(error), |item_name| {
                parent.create_named_sub_element(element_name, &item_name)
            })
        }) {
            Ok(twin) => twin,
            Err(_) => {
                // keep the depth of the twins in sync with the depth of the source elements, so
                // that a single failure doesn't derail the rest of the run
                errors += 1;
                twins.push(parent.clone());
                continue;
            }
        };

        // copy the attributes and the character data of the element
        for attribute in element.attributes() {
            if twin.set_attribute(attribute.attrname, attribute.content).is_err() {
                errors += 1;
            }
        }
        match element.content_type() {
            ContentType::CharacterData if !reused => {
                if let Some(chardata) = element.character_data()
                    && twin.set_character_data(chardata).is_err()
                {
                    errors += 1;
                }
            }
            ContentType::Mixed => mixed.push((element.clone(), twin.clone())),
            _ => {}
        }
        if let Some(comment) = element.comment() {
            twin.set_comment(Some(comment));
        }

        twins.push(twin);
    }

    // now that all elements exist, the character data of mixed content elements can be inserted
    // between them, at the same positions as in the source model
    for (element, twin) in mixed {
        for (position, content) in element.content().enumerate() {
            if let ElementContent::CharacterData(chardata) = content
                && twin
                    .insert_character_content_item(&chardata.to_string(), position)
                    .is_err()
            {
                errors += 1;
            }
        }
    }

    BuildResult {
        model,
        elements,
        errors,
    }
}

/// load the input file and build the twin model once, to display some information about them
///
/// Returns the source model and the number of elements in it, or `None` if the file can't be used.
fn prepare(path: &Path, strict: bool) -> Option<(AutosarModel, u64)> {
    let source = AutosarModel::new();
    let file = match source.load_file(path, strict) {
        Ok((file, _warnings)) => file,
        Err(error) => {
            eprintln!("{}: could not be loaded, skipping it: {error}", path.display());
            return None;
        }
    };
    let version = file.version();

    let BuildResult {
        model,
        elements,
        errors,
    } = build_twin_model(&source, path, version);
    let created = model.elements_dfs().count();
    let identifiable_count = model.identifiable_elements().count();
    println!(
        "{}: {version}, {elements} elements in the source model, \
         {created} elements and {identifiable_count} identifiables in the twin model, {errors} errors",
        path.display()
    );
    if errors > 0 {
        eprintln!(
            "{}: {errors} operations failed while building the twin model; \
             the benchmark measures the remaining operations",
            path.display()
        );
    }

    Some((source, elements as u64))
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let files = get_input_files();
    if files.is_empty() {
        print_usage("build_model");
        return;
    }
    let strict = get_env_flag(STRICT_VAR, false);
    let samples = get_sample_count();
    let names = benchmark_names(&files);

    let mut group = c.benchmark_group("build_model");
    group
        .sample_size(samples)
        // large files can take seconds per iteration; don't let criterion try to fit 5 seconds
        // of measurement into the configured number of samples
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));

    for (path, name) in files.iter().zip(names) {
        let Some((source, element_count)) = prepare(path, strict) else {
            continue;
        };
        let version = source
            .files()
            .next()
            .map_or(AutosarVersion::LATEST, |file| file.version());

        group.throughput(Throughput::Elements(element_count));
        group.bench_function(BenchmarkId::from_parameter(&name), |bencher| {
            bencher.iter_batched(
                || (),
                // the result is returned so that the new model is dropped outside of the
                // measured section
                |()| build_twin_model(&source, path, version),
                // one iteration per batch: each model may use a lot of memory, so collecting a
                // whole batch of them before dropping them is not an option
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
