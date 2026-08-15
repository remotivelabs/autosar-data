//! Input data for the regex microbenchmarks.
//!
//! The strings imitate what a real arxml file load feeds into the validators: Autosar identifiers
//! (`SHORT-NAME` and friends) for `validate_regex_8` and Autosar paths (`*-REF` character data)
//! for `validate_regex_24`. The length distribution follows the files produced by
//! `cargo run --example generate_files` (identifiers: p10 = 14, p50 = 28, p90 = 44 characters)
//! widened at the short end with hand written names of the kind found in real ecu extracts.
//!
//! Nearly all input in a real file is valid, so the corpus is valid too, apart from a small
//! fraction of rejected strings: the validators are called on every value, and a rejection
//! normally exits early, which would flatter an implementation that gives up quickly.

#![allow(dead_code)]

/// a deterministic pseudo random number generator, so that the corpus is identical in every run
pub struct Rand(u64);

impl Rand {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, limit: usize) -> usize {
        (self.next() % (limit as u64)) as usize
    }
}

const WORDS: [&str; 24] = [
    "Can",
    "Frame",
    "Triggering",
    "Ecu",
    "Instance",
    "Signal",
    "Group",
    "Pdu",
    "Mapping",
    "Cluster",
    "Controller",
    "Communication",
    "Connector",
    "Data",
    "Type",
    "Impl",
    "Base",
    "Sw",
    "Cs",
    "Isignal",
    "Port",
    "Interface",
    "Prototype",
    "Element",
];

/// build one identifier of roughly the requested length
fn identifier(rand: &mut Rand, target_len: usize) -> String {
    let mut name = String::new();
    while name.len() < target_len {
        if !name.is_empty() && rand.below(4) == 0 {
            name.push('_');
        }
        name.push_str(WORDS[rand.below(WORDS.len())]);
    }
    name.truncate(target_len.max(1));
    // a trailing '_' is legal, but make sure the first character is a letter
    if !name.as_bytes()[0].is_ascii_alphabetic() {
        name.insert(0, 'X');
    }
    if rand.below(3) == 0 {
        name.push('_');
        name.push_str(&rand.below(1000).to_string());
    }
    name
}

/// 2000 Autosar identifiers, ~2% of them invalid
pub fn identifiers() -> Vec<String> {
    let mut rand = Rand::new(0x1234_5678);
    let mut result = Vec::with_capacity(2000);
    for idx in 0..2000 {
        // length distribution: mostly 8..48, with a short and a long tail
        let len = match rand.below(10) {
            0 => 3 + rand.below(6),
            1..=7 => 8 + rand.below(32),
            _ => 40 + rand.below(40),
        };
        let mut name = identifier(&mut rand, len);
        if idx % 50 == 49 {
            // an invalid string: either a leading digit or an invalid character in the middle
            if idx % 100 == 49 {
                name.insert(0, '4');
            } else {
                let pos = name.len() / 2;
                name.insert(pos, '-');
            }
        }
        result.push(name);
    }
    result
}

/// 2000 Autosar reference paths, ~2% of them invalid
pub fn paths() -> Vec<String> {
    let mut rand = Rand::new(0x8765_4321);
    let mut result = Vec::with_capacity(2000);
    for idx in 0..2000 {
        let segments = 2 + rand.below(4);
        let mut path = String::new();
        for _ in 0..segments {
            path.push('/');
            let len = match rand.below(10) {
                0 => 3 + rand.below(6),
                1..=8 => 8 + rand.below(24),
                _ => 32 + rand.below(32),
            };
            path.push_str(&identifier(&mut rand, len));
        }
        if idx % 50 == 49 {
            if idx % 100 == 49 {
                // an empty path segment
                path.push('/');
            } else {
                // an invalid character in the last segment
                path.push('.');
                path.push('x');
            }
        }
        result.push(path);
    }
    result
}
