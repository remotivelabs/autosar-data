//! Alternative implementations of `validate_regex_7`, `validate_regex_8` and `validate_regex_24`.
//!
//! `current_*` are the implementations from `src/regex.rs` (included into the benchmark binary),
//! everything else is a candidate replacement. All candidates are checked against the current
//! implementation by `common/verify.rs`, which the benchmark runs before measuring.

#![allow(dead_code)]

pub type Validator = fn(&[u8]) -> bool;

pub static REGEX_7_VARIANTS: &[(&str, Validator)] = &[
    ("current", crate::regex::validate_regex_7),
    ("previous", validate_7_previous),
    ("lut", validate_7_lut),
];

pub static REGEX_8_VARIANTS: &[(&str, Validator)] = &[
    ("current", crate::regex::validate_regex_8),
    ("previous", validate_8_previous),
    ("branchless", validate_8_branchless),
    ("whole", validate_8_whole),
    ("chunked", validate_8_chunked),
    ("lut", validate_8_lut),
];

pub static REGEX_22_VARIANTS: &[(&str, Validator)] = &[
    ("current", crate::regex::validate_regex_22),
    ("state", validate_22_state),
];

pub static REGEX_24_VARIANTS: &[(&str, Validator)] = &[
    ("current", crate::regex::validate_regex_24),
    ("previous", validate_24_previous),
    ("state", validate_24_state),
    ("pairwise", validate_24_pairwise),
    ("lut", validate_24_lut),
    ("state_len_limit", validate_24_state_len_limit),
    ("pairwise_len_limit", validate_24_pairwise_len_limit),
];

pub static COMBINED_VARIANTS: &[(&str, Validator, Validator)] = &[
    (
        "current",
        crate::regex::validate_regex_8,
        crate::regex::validate_regex_24,
    ),
    ("previous", validate_8_previous, validate_24_previous),
    ("chunked+state", validate_8_chunked, validate_24_state),
    ("lut", validate_8_lut, validate_24_lut),
];

// ----------------------------------------------------------------------------------------------
// the implementations that the code generator produced before the hand written versions in
// src/regex.rs replaced them; they are kept here as the reference point of the comparison
// ----------------------------------------------------------------------------------------------

pub fn validate_7_previous(s: &[u8]) -> bool {
    !s.is_empty()
        && (s[0].is_ascii_alphabetic() || s[0] == b'_')
        && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

pub fn validate_8_previous(s: &[u8]) -> bool {
    !s.is_empty() && s[0].is_ascii_alphabetic() && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

pub fn validate_24_previous(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut path = s;
    if path[0] == b'/' {
        path = &path[1..];
    }
    path.split(|c| *c == b'/').all(validate_8_previous)
}

// ----------------------------------------------------------------------------------------------
// character classification without a lookup table
// ----------------------------------------------------------------------------------------------

/// `[a-zA-Z]`: fold the case bit, then a single unsigned range check
#[inline(always)]
fn is_alpha(c: u8) -> bool {
    (c | 0x20).wrapping_sub(b'a') < 26
}

/// `[a-zA-Z0-9_]`, evaluated without short circuiting so that the caller's loop can vectorize
#[inline(always)]
fn is_word(c: u8) -> bool {
    is_alpha(c) | (c.wrapping_sub(b'0') < 10) | (c == b'_')
}

// ----------------------------------------------------------------------------------------------
// character classification with a lookup table
// ----------------------------------------------------------------------------------------------

const CLASS_ALPHA: u8 = 1;
const CLASS_WORD: u8 = 2;
const CLASS_SLASH: u8 = 4;

const fn build_class_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut idx = 0;
    while idx < 256 {
        let c = idx as u8;
        let mut flags = 0;
        if c.is_ascii_alphabetic() {
            flags |= CLASS_ALPHA | CLASS_WORD;
        } else if c.is_ascii_digit() || c == b'_' {
            flags |= CLASS_WORD;
        } else if c == b'/' {
            flags |= CLASS_SLASH;
        }
        table[idx] = flags;
        idx += 1;
    }
    table
}

static CLASS: [u8; 256] = build_class_table();

// ----------------------------------------------------------------------------------------------
// candidates for validate_regex_7: ^([a-zA-Z_][a-zA-Z0-9_]*)$
// ----------------------------------------------------------------------------------------------

pub fn validate_7_lut(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    let mut bad = 0u8;
    for &c in rest {
        bad |= !CLASS[c as usize] & CLASS_WORD;
    }
    // the start class of regex 7 is [a-zA-Z_], which is CLASS_WORD without the digits
    (CLASS[first as usize] & CLASS_ALPHA != 0 || first == b'_') && bad == 0
}

// ----------------------------------------------------------------------------------------------
// candidates for validate_regex_8: ^([a-zA-Z][a-zA-Z0-9_]*)$
// ----------------------------------------------------------------------------------------------

pub fn validate_8_branchless(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    let mut ok = is_alpha(first);
    for &c in rest {
        ok &= is_word(c);
    }
    ok
}

/// like `validate_8_branchless`, but the vectorized loop runs over the whole string instead of
/// starting at an offset of one
pub fn validate_8_whole(s: &[u8]) -> bool {
    if s.is_empty() || !is_alpha(s[0]) {
        return false;
    }
    let mut ok = true;
    for &c in s {
        ok &= is_word(c);
    }
    ok
}

pub fn validate_8_chunked(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    if !is_alpha(first) {
        return false;
    }
    let mut ok = true;
    let mut chunks = rest.chunks_exact(16);
    for chunk in &mut chunks {
        let mut chunk_ok = true;
        for &c in chunk {
            chunk_ok &= is_word(c);
        }
        ok &= chunk_ok;
    }
    for &c in chunks.remainder() {
        ok &= is_word(c);
    }
    ok
}

pub fn validate_8_lut(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    // accumulate the "not a word character" bit of every character instead of branching per character
    let mut bad = 0u8;
    for &c in rest {
        bad |= !CLASS[c as usize] & CLASS_WORD;
    }
    (CLASS[first as usize] & CLASS_ALPHA != 0) && bad == 0
}

// ----------------------------------------------------------------------------------------------
// candidates for validate_regex_22: ^([a-zA-Z]([a-zA-Z0-9]|_[a-zA-Z0-9])*_?)$
// ----------------------------------------------------------------------------------------------

/// the shipped implementation checks each character against its predecessor, which vectorizes;
/// this one tracks the same condition in a state variable, which is shorter but does not
pub fn validate_22_state(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    if !is_alpha(first) {
        return false;
    }
    let mut after_underscore = false;
    for &c in rest {
        if c == b'_' {
            if after_underscore {
                return false;
            }
            after_underscore = true;
        } else if c.is_ascii_alphanumeric() {
            after_underscore = false;
        } else {
            return false;
        }
    }
    true
}

// ----------------------------------------------------------------------------------------------
// candidates for validate_regex_24: ^(/?[a-zA-Z][a-zA-Z0-9_]{0,127}(/[a-zA-Z][a-zA-Z0-9_]{0,127})*)$
// ----------------------------------------------------------------------------------------------

/// single pass over the string, tracking whether the next character starts a new path segment
pub fn validate_24_state(s: &[u8]) -> bool {
    let rest = match s.split_first() {
        Some((&b'/', tail)) => tail,
        Some(_) => s,
        None => return false,
    };
    let mut segment_start = true;
    for &c in rest {
        if segment_start {
            if !is_alpha(c) {
                return false;
            }
            segment_start = false;
        } else if c == b'/' {
            segment_start = true;
        } else if !is_word(c) {
            return false;
        }
    }
    !segment_start
}

/// like `validate_24_state`, but enforces the `{0,127}` segment length limit of the regex
pub fn validate_24_state_len_limit(s: &[u8]) -> bool {
    let rest = match s.split_first() {
        Some((&b'/', tail)) => tail,
        Some(_) => s,
        None => return false,
    };
    let mut segment_len = 0usize;
    for &c in rest {
        if segment_len == 0 {
            if !is_alpha(c) {
                return false;
            }
            segment_len = 1;
        } else if c == b'/' {
            segment_len = 0;
        } else if !is_word(c) || segment_len > 127 {
            return false;
        } else {
            segment_len += 1;
        }
    }
    segment_len != 0
}

/// branchless single pass: each character is validated against the class of its predecessor
pub fn validate_24_pairwise(s: &[u8]) -> bool {
    let body = match s.split_first() {
        Some((&b'/', tail)) => tail,
        Some(_) => s,
        None => return false,
    };
    let Some((&first, tail)) = body.split_first() else {
        return false;
    };
    if !is_alpha(first) || *body.last().unwrap() == b'/' {
        return false;
    }
    let mut ok = true;
    for (&prev, &cur) in body.iter().zip(tail) {
        let after_slash = prev == b'/';
        ok &= (after_slash & is_alpha(cur)) | (!after_slash & (is_word(cur) | (cur == b'/')));
    }
    ok
}

/// `validate_24_pairwise` with the `{0,127}` segment length limit of the regex.
///
/// No segment can be longer than the whole string, so the limit only needs to be checked at all if
/// the string is longer than the limit - which practically never happens.
pub fn validate_24_pairwise_len_limit(s: &[u8]) -> bool {
    if s.len() > 128 {
        return validate_24_state_len_limit(s);
    }
    validate_24_pairwise(s)
}

/// single pass over the string using the classification table
pub fn validate_24_lut(s: &[u8]) -> bool {
    let rest = match s.split_first() {
        Some((&b'/', tail)) => tail,
        Some(_) => s,
        None => return false,
    };
    let mut segment_start = true;
    for &c in rest {
        let class = CLASS[c as usize];
        if segment_start {
            if class & CLASS_ALPHA == 0 {
                return false;
            }
            segment_start = false;
        } else if class & CLASS_SLASH != 0 {
            segment_start = true;
        } else if class & CLASS_WORD == 0 {
            return false;
        }
    }
    !segment_start
}
