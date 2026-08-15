//! Checks the validators in `src/regex.rs` against the patterns they implement.
//!
//! The validators are hand written, so they need an independent reference: `check_against_patterns`
//! compiles the pattern of every validator with the `regex` crate and compares the two over a
//! generated corpus (see `common/cases.rs`). `check_candidates` additionally compares the
//! alternative implementations in `common/candidates.rs` against the shipped ones.
//!
//! The benchmark runs both before measuring anything.

#![allow(dead_code)]

use crate::candidates;
use crate::cases::{self, RegexCase};
use crate::corpus;

/// every string over `alphabet` with a length of up to `max_len`, plus every single character edit
/// of every sample: substitution, deletion and insertion at every position
fn inputs_for(case: &RegexCase) -> Vec<Vec<u8>> {
    let alphabet: Vec<u8> = case.alphabet.bytes().collect();
    let mut inputs: Vec<Vec<u8>> = vec![Vec::new()];

    let mut previous: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..case.exhaustive_len {
        let mut next = Vec::with_capacity(previous.len() * alphabet.len());
        for base in &previous {
            for &c in &alphabet {
                let mut extended = base.clone();
                extended.push(c);
                next.push(extended);
            }
        }
        inputs.extend(next.iter().cloned());
        previous = next;
    }

    for sample in case.samples {
        let sample = sample.as_bytes().to_vec();
        inputs.push(sample.clone());
        for pos in 0..sample.len() {
            let mut deleted = sample.clone();
            deleted.remove(pos);
            inputs.push(deleted);
            for &c in &alphabet {
                let mut substituted = sample.clone();
                substituted[pos] = c;
                inputs.push(substituted);
                let mut inserted = sample.clone();
                inserted.insert(pos, c);
                inputs.push(inserted);
            }
        }
        for &c in &alphabet {
            let mut appended = sample.clone();
            appended.push(c);
            inputs.push(appended);
        }
    }

    // random strings over the alphabet: they reach lengths that exhaustive generation cannot
    let mut rand = corpus::Rand::new(0xC0FFEE);
    for _ in 0..20000 {
        let len = rand.below(17);
        inputs.push((0..len).map(|_| alphabet[rand.below(alphabet.len())]).collect());
    }

    // random splices of two samples, cut at a random position: structurally plausible near misses
    if !case.samples.is_empty() {
        for _ in 0..20000 {
            let left = case.samples[rand.below(case.samples.len())].as_bytes();
            let right = case.samples[rand.below(case.samples.len())].as_bytes();
            let mut spliced = left[..rand.below(left.len() + 1)].to_vec();
            spliced.extend_from_slice(&right[rand.below(right.len() + 1)..]);
            if rand.below(4) == 0 && !spliced.is_empty() {
                // plus a random single character edit
                let pos = rand.below(spliced.len());
                spliced[pos] = alphabet[rand.below(alphabet.len())];
            }
            inputs.push(spliced);
        }
    }

    // bytes that no pattern contains, including a newline (which `.` does not match) and non-ascii
    for &c in b"\n\r\t\0\x7f\xff\xc3" {
        inputs.push(vec![c]);
        for sample in case.samples.iter().take(4) {
            let mut prefixed = sample.as_bytes().to_vec();
            prefixed.insert(0, c);
            inputs.push(prefixed);
            let mut suffixed = sample.as_bytes().to_vec();
            suffixed.push(c);
            inputs.push(suffixed);
            if !sample.is_empty() {
                let mut infixed = sample.as_bytes().to_vec();
                infixed.insert(sample.len() / 2, c);
                inputs.push(infixed);
            }
        }
    }

    inputs
}

/// compare every validator against the pattern it claims to implement
pub fn check_against_patterns() -> usize {
    let mut checked = 0;
    let mut failures = 0;

    for case in cases::CASES {
        let pattern = regex::bytes::RegexBuilder::new(case.pattern)
            .unicode(false)
            .build()
            .unwrap_or_else(|error| panic!("{}: pattern does not compile: {error}", case.name));

        let mut reported = 0;
        for input in inputs_for(case) {
            let expected = pattern.is_match(&input);
            let actual = (case.validate)(&input);
            checked += 1;
            if actual != expected {
                failures += 1;
                reported += 1;
                if reported <= 5 {
                    println!(
                        "  {} {:?}: validator says {actual}, pattern says {expected}",
                        case.name,
                        String::from_utf8_lossy(&input)
                    );
                }
            }
        }
        if reported > 5 {
            println!("  {}: ... and {} more", case.name, reported - 5);
        }
    }

    assert_eq!(
        failures, 0,
        "{failures} inputs are classified differently than by the pattern"
    );
    checked
}

/// compare the alternative implementations against the shipped ones
pub fn check_candidates() -> usize {
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    for case in cases::CASES {
        if matches!(case.name, "regex_7" | "regex_8" | "regex_22" | "regex_24") {
            inputs.extend(inputs_for(case));
        }
    }
    inputs.extend(corpus::identifiers().into_iter().map(String::into_bytes));
    inputs.extend(corpus::paths().into_iter().map(String::into_bytes));
    // one long segment: right at the {0,127} limit of regex 24 and beyond it
    for len in [127, 128, 129] {
        inputs.push(core::iter::repeat_n(b'a', len).collect());
        let mut path = b"/pkg/".to_vec();
        path.extend(core::iter::repeat_n(b'b', len));
        inputs.push(path);
    }
    // every single byte value, on its own and as the second character
    for byte in 0..=255u8 {
        inputs.push(vec![byte]);
        inputs.push(vec![b'a', byte]);
        inputs.push(vec![b'/', b'a', byte, b'b']);
    }

    for input in &inputs {
        let expected_7 = crate::regex::validate_regex_7(input);
        for (name, func) in candidates::REGEX_7_VARIANTS {
            assert_eq!(
                func(input),
                expected_7,
                "regex_7 candidate {name} disagrees on {:?}",
                String::from_utf8_lossy(input)
            );
        }

        let expected_8 = crate::regex::validate_regex_8(input);
        for (name, func) in candidates::REGEX_8_VARIANTS {
            assert_eq!(
                func(input),
                expected_8,
                "regex_8 candidate {name} disagrees on {:?}",
                String::from_utf8_lossy(input)
            );
        }

        let expected_22 = crate::regex::validate_regex_22(input);
        for (name, func) in candidates::REGEX_22_VARIANTS {
            assert_eq!(
                func(input),
                expected_22,
                "regex_22 candidate {name} disagrees on {:?}",
                String::from_utf8_lossy(input)
            );
        }

        let expected_24 = crate::regex::validate_regex_24(input);
        for (name, func) in candidates::REGEX_24_VARIANTS {
            if name.ends_with("len_limit") {
                // these enforce the segment length limit, which the shipped implementation ignores,
                // so they are compared against a literal reading of the regex instead
                assert_eq!(
                    func(input),
                    reference_24(input),
                    "regex_24 candidate {name} disagrees with the regex on {:?}",
                    String::from_utf8_lossy(input)
                );
                continue;
            }
            assert_eq!(
                func(input),
                expected_24,
                "regex_24 candidate {name} disagrees on {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    inputs.len()
}

/// a literal, deliberately slow reading of
/// `^(/?[a-zA-Z][a-zA-Z0-9_]{0,127}(/[a-zA-Z][a-zA-Z0-9_]{0,127})*)$`
fn reference_24(s: &[u8]) -> bool {
    let body = if s.first() == Some(&b'/') { &s[1..] } else { s };
    if body.is_empty() {
        return false;
    }
    body.split(|c| *c == b'/').all(|segment| {
        !segment.is_empty()
            && segment.len() <= 128
            && segment[0].is_ascii_alphabetic()
            && segment[1..].iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
    })
}

pub fn verify() {
    let by_pattern = check_against_patterns();
    let candidates = check_candidates();
    println!(
        "verified {} validators on {by_pattern} inputs, candidates on {candidates} inputs",
        cases::CASES.len()
    );
}
