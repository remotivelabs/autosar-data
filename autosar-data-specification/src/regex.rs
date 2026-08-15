/// `x+` for the character class `class`: not empty, and every character is in the class
#[inline(always)]
fn all_of(s: &[u8], class: impl Fn(u8) -> bool) -> bool {
    !s.is_empty() && s.iter().all(|&c| class(c))
}

/// split at the first occurrence of `sep`, which is not part of either half
#[inline(always)]
fn split_first_at(s: &[u8], sep: impl Fn(u8) -> bool) -> Option<(&[u8], &[u8])> {
    s.iter().position(|&c| sep(c)).map(|pos| (&s[..pos], &s[pos + 1..]))
}

/// `[0-9]`
#[inline(always)]
fn is_digit(c: u8) -> bool {
    c.wrapping_sub(b'0') < 10
}

/// `[0-7]`
#[inline(always)]
fn is_octal_digit(c: u8) -> bool {
    c.wrapping_sub(b'0') < 8
}

/// `[0-1]`
#[inline(always)]
fn is_binary_digit(c: u8) -> bool {
    c.wrapping_sub(b'0') < 2
}

/// `[0-9a-fA-F]`
#[inline(always)]
fn is_hex_digit(c: u8) -> bool {
    is_digit(c) | ((c | 0x20).wrapping_sub(b'a') < 6)
}

/// validate ^(0[xX][0-9a-fA-F]+)$
pub(crate) fn validate_regex_1(s: &[u8]) -> bool {
    matches!(s, [b'0', b'x' | b'X', digits @ ..] if all_of(digits, is_hex_digit))
}

/// validate ^([1-9][0-9]*|0[xX][0-9a-fA-F]*|0[bB][0-1]+|0[0-7]*|UNSPECIFIED|UNKNOWN|BOOLEAN|PTR)$
pub(crate) fn validate_regex_2(s: &[u8]) -> bool {
    match s {
        [b'0', b'x' | b'X', digits @ ..] => digits.iter().all(|&c| is_hex_digit(c)),
        [b'0', b'b' | b'B', digits @ ..] => all_of(digits, is_binary_digit),
        [b'0', digits @ ..] => digits.iter().all(|&c| is_octal_digit(c)),
        [b'1'..=b'9', digits @ ..] => digits.iter().all(|&c| is_digit(c)),
        b"UNSPECIFIED" | b"UNKNOWN" | b"BOOLEAN" | b"PTR" => true,
        _ => false,
    }
}

/// validate ^([1-9][0-9]*|0[xX][0-9a-fA-F]+|0[0-7]*|0[bB][0-1]+|ANY|ALL)$
pub(crate) fn validate_regex_3(s: &[u8]) -> bool {
    match s {
        [b'0', b'x' | b'X', digits @ ..] => all_of(digits, is_hex_digit),
        [b'0', b'b' | b'B', digits @ ..] => all_of(digits, is_binary_digit),
        [b'0', digits @ ..] => digits.iter().all(|&c| is_octal_digit(c)),
        [b'1'..=b'9', digits @ ..] => digits.iter().all(|&c| is_digit(c)),
        b"ANY" | b"ALL" => true,
        _ => false,
    }
}

/// validate ^([0-9]+|ANY)$
pub(crate) fn validate_regex_4(s: &[u8]) -> bool {
    (!s.is_empty() && s.iter().all(u8::is_ascii_digit)) || s == b"ANY"
}

/// validate ^([0-9]+|STRING|ARRAY)$
pub(crate) fn validate_regex_5(s: &[u8]) -> bool {
    (!s.is_empty() && s.iter().all(u8::is_ascii_digit)) || s == b"STRING" || s == b"ARRAY"
}

/// validate ^(0|1|true|false)$
pub(crate) fn validate_regex_6(s: &[u8]) -> bool {
    s == b"0" || s == b"1" || s == b"true" || s == b"false"
}

/// `[a-zA-Z]`: fold the case bit, then a single unsigned range check
#[inline(always)]
fn is_ident_start(c: u8) -> bool {
    (c | 0x20).wrapping_sub(b'a') < 26
}

/// `[a-zA-Z0-9_]`, evaluated without short circuiting so that loops over it can be vectorized
#[inline(always)]
fn is_ident_char(c: u8) -> bool {
    is_ident_start(c) | (c.wrapping_sub(b'0') < 10) | (c == b'_')
}

/// validate ^([a-zA-Z_][a-zA-Z0-9_]*)$
pub(crate) fn validate_regex_7(s: &[u8]) -> bool {
    if s.is_empty() || !(is_ident_start(s[0]) | (s[0] == b'_')) {
        return false;
    }
    // deliberately no early exit and no short circuiting: this loop vectorizes, and validation
    // succeeds for nearly every input anyway, so there is nothing to exit early from
    let mut valid = true;
    for &c in s {
        valid &= is_ident_char(c);
    }
    valid
}

/// validate ^([a-zA-Z][a-zA-Z0-9_]*)$
pub(crate) fn validate_regex_8(s: &[u8]) -> bool {
    if s.is_empty() || !is_ident_start(s[0]) {
        return false;
    }
    // deliberately no early exit and no short circuiting: this loop vectorizes, and validation
    // succeeds for nearly every input anyway, so there is nothing to exit early from
    let mut valid = true;
    for &c in s {
        valid &= is_ident_char(c);
    }
    valid
}

/// validate ^(([0-9]{4}-[0-9]{2}-[0-9]{2})(T[0-9]{2}:[0-9]{2}:[0-9]{2}(Z|([+\-][0-9]{2}:[0-9]{2})))?)$
pub(crate) fn validate_regex_9(s: &[u8]) -> bool {
    // the pattern only permits three lengths: a date, a date and time with a 'Z', or with an offset
    if !matches!(s.len(), 10 | 20 | 25) {
        return false;
    }
    // date: [0-9]{4}-[0-9]{2}-[0-9]{2}
    if !(s[..4].iter().all(|&c| is_digit(c))
        && s[4] == b'-'
        && s[5..7].iter().all(|&c| is_digit(c))
        && s[7] == b'-'
        && s[8..10].iter().all(|&c| is_digit(c)))
    {
        return false;
    }
    if s.len() == 10 {
        return true;
    }
    // time: T[0-9]{2}:[0-9]{2}:[0-9]{2}
    if !(s[10] == b'T'
        && s[11..13].iter().all(|&c| is_digit(c))
        && s[13] == b':'
        && s[14..16].iter().all(|&c| is_digit(c))
        && s[16] == b':'
        && s[17..19].iter().all(|&c| is_digit(c)))
    {
        return false;
    }
    // time zone: either 'Z' or [+-][0-9]{2}:[0-9]{2}
    if s.len() == 20 {
        s[19] == b'Z'
    } else {
        matches!(s[19], b'+' | b'-')
            && s[20..22].iter().all(|&c| is_digit(c))
            && s[22] == b':'
            && s[23..25].iter().all(|&c| is_digit(c))
    }
}

/// validate ^([a-zA-Z][a-zA-Z0-9-]*)$
pub(crate) fn validate_regex_10(s: &[u8]) -> bool {
    !s.is_empty() && s[0].is_ascii_alphabetic() && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-')
}

/// validate ^([0-9a-zA-Z_\-]+)$
pub(crate) fn validate_regex_11(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

/// validate ^(%[ \-+#]?[0-9]*(\.[0-9]+)?[bBdiouxXfeEgGcs])$
pub(crate) fn validate_regex_12(s: &[u8]) -> bool {
    let [b'%', rest @ ..] = s else {
        return false;
    };
    // the conversion character at the end
    let Some((&conversion, rest)) = rest.split_last() else {
        return false;
    };
    if !matches!(
        conversion,
        b'b' | b'B' | b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'f' | b'e' | b'E' | b'g' | b'G' | b'c' | b's'
    ) {
        return false;
    }
    // an optional flag character, then a width and an optional precision
    let rest = match rest {
        [b' ' | b'-' | b'+' | b'#', tail @ ..] => tail,
        _ => rest,
    };
    match split_first_at(rest, |c| c == b'.') {
        Some((width, precision)) => width.iter().all(|&c| is_digit(c)) && all_of(precision, is_digit),
        None => rest.iter().all(|&c| is_digit(c)),
    }
}

/// validate ^(0|[\+\-]?[1-9][0-9]*|0[xX][0-9a-fA-F]+|0[bB][0-1]+|0[0-7]+)$
pub(crate) fn validate_regex_13(s: &[u8]) -> bool {
    match s {
        // only the decimal form may have a sign
        [b'+' | b'-', rest @ ..] => matches!(rest, [b'1'..=b'9', digits @ ..] if digits.iter().all(|&c| is_digit(c))),
        b"0" => true,
        [b'0', b'x' | b'X', digits @ ..] => all_of(digits, is_hex_digit),
        [b'0', b'b' | b'B', digits @ ..] => all_of(digits, is_binary_digit),
        [b'0', digits @ ..] => all_of(digits, is_octal_digit),
        [b'1'..=b'9', digits @ ..] => digits.iter().all(|&c| is_digit(c)),
        _ => false,
    }
}

/// validate ^((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)|ANY)$
pub(crate) fn validate_regex_14(s: &[u8]) -> bool {
    /// `25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?`: one or two digits, or three digits below 256
    fn is_octet(part: &[u8]) -> bool {
        match part {
            [a] => is_digit(*a),
            [a, b] => is_digit(*a) && is_digit(*b),
            [a, b, c] => {
                is_digit(*a)
                    && is_digit(*b)
                    && is_digit(*c)
                    && (u32::from(a - b'0') * 100 + u32::from(b - b'0') * 10 + u32::from(c - b'0')) <= 255
            }
            _ => false,
        }
    }

    if s == b"ANY" {
        return true;
    }
    let mut parts = s.split(|&c| c == b'.');
    match (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(c), Some(d), None) => is_octet(a) && is_octet(b) && is_octet(c) && is_octet(d),
        _ => false,
    }
}

/// validate ^([0-9A-Fa-f]{1,4}(:[0-9A-Fa-f]{1,4}){7,7}|ANY)$
pub(crate) fn validate_regex_15(s: &[u8]) -> bool {
    if s == b"ANY" {
        return true;
    }
    let mut groups = 0;
    for group in s.split(|&c| c == b':') {
        groups += 1;
        if groups > 8 || group.len() > 4 || !all_of(group, is_hex_digit) {
            return false;
        }
    }
    groups == 8
}

/// validate ^((0[xX][0-9a-fA-F]+)|(0[0-7]+)|(0[bB][0-1]+)|(([+\-]?[1-9][0-9]+(\.[0-9]+)?|[+\-]?[0-9](\.[0-9]+)?)([eE]([+\-]?)[0-9]+)?)|\.0|INF|-INF|NaN)$
pub(crate) fn validate_regex_16(s: &[u8]) -> bool {
    /// `([+\-]?[1-9][0-9]+(\.[0-9]+)?|[+\-]?[0-9](\.[0-9]+)?)([eE]([+\-]?)[0-9]+)?`, in one pass
    fn is_decimal(s: &[u8]) -> bool {
        let mut pos = 0;
        if pos < s.len() && matches!(s[pos], b'+' | b'-') {
            pos += 1;
        }
        // the integer part is either a single digit or several digits not starting with a zero
        let integer_start = pos;
        while pos < s.len() && is_digit(s[pos]) {
            pos += 1;
        }
        if pos == integer_start || (pos - integer_start > 1 && s[integer_start] == b'0') {
            return false;
        }
        // an optional fraction
        if pos < s.len() && s[pos] == b'.' {
            pos += 1;
            let fraction_start = pos;
            while pos < s.len() && is_digit(s[pos]) {
                pos += 1;
            }
            if pos == fraction_start {
                return false;
            }
        }
        if pos == s.len() {
            return true;
        }
        // an optional exponent, which is the only place where a second sign may appear
        if !matches!(s[pos], b'e' | b'E') {
            return false;
        }
        pos += 1;
        if pos < s.len() && matches!(s[pos], b'+' | b'-') {
            pos += 1;
        }
        let exponent_start = pos;
        while pos < s.len() && is_digit(s[pos]) {
            pos += 1;
        }
        pos > exponent_start && pos == s.len()
    }

    match s {
        b"INF" | b"-INF" | b"NaN" | b".0" => true,
        [b'0', b'x' | b'X', digits @ ..] => all_of(digits, is_hex_digit),
        [b'0', b'b' | b'B', digits @ ..] => all_of(digits, is_binary_digit),
        // 0[0-7]+ overlaps with the decimal form, which only accepts "0" itself
        [b'0', digits @ ..] if all_of(digits, is_octal_digit) => true,
        _ => is_decimal(s),
    }
}

/// validate ^(([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2})$
pub(crate) fn validate_regex_17(s: &[u8]) -> bool {
    s.len() == 17
        && s.split(|c| *c == b':')
            .all(|part| part.len() == 2 && part[0].is_ascii_hexdigit() && part[1].is_ascii_hexdigit())
}

/// validate ^([a-zA-Z_][a-zA-Z0-9_]*(\[([a-zA-Z_][a-zA-Z0-9_]*|[0-9]+)\])*(\.[a-zA-Z_][a-zA-Z0-9_]*(\[([a-zA-Z_][a-zA-Z0-9_]*|[0-9]+)\])*)*)$
pub(crate) fn validate_regex_18(s: &[u8]) -> bool {
    let mut pos = 0;
    // a '.'-separated list of names, each with any number of subscripts
    loop {
        // the name: [a-zA-Z_][a-zA-Z0-9_]*
        if pos == s.len() || !(is_ident_start(s[pos]) || s[pos] == b'_') {
            return false;
        }
        pos += 1;
        while pos < s.len() && is_ident_char(s[pos]) {
            pos += 1;
        }

        // the subscripts: [name] or [digits]
        while pos < s.len() && s[pos] == b'[' {
            pos += 1;
            let index_start = pos;
            while pos < s.len() && s[pos] != b']' {
                pos += 1;
            }
            if pos == s.len() {
                return false;
            }
            let index = &s[index_start..pos];
            if !(validate_regex_7(index) || all_of(index, is_digit)) {
                return false;
            }
            pos += 1;
        }

        if pos == s.len() {
            return true;
        }
        if s[pos] != b'.' {
            return false;
        }
        pos += 1;
    }
}

/// validate ^([A-Z][a-zA-Z0-9_]*)$
pub(crate) fn validate_regex_19(s: &[u8]) -> bool {
    !s.is_empty() && s[0].is_ascii_uppercase() && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// validate ^([1-9][0-9]*)$
pub(crate) fn validate_regex_20(s: &[u8]) -> bool {
    !s.is_empty() && s[0] != b'0' && s.iter().all(u8::is_ascii_digit)
}

/// validate ^(0|[\+]?[1-9][0-9]*|0[xX][0-9a-fA-F]+|0[bB][0-1]+|0[0-7]+)$
pub(crate) fn validate_regex_21(s: &[u8]) -> bool {
    match s {
        // only the decimal form may have a sign, and only a '+'
        [b'+', rest @ ..] => matches!(rest, [b'1'..=b'9', digits @ ..] if digits.iter().all(|&c| is_digit(c))),
        b"0" => true,
        [b'0', b'x' | b'X', digits @ ..] => all_of(digits, is_hex_digit),
        [b'0', b'b' | b'B', digits @ ..] => all_of(digits, is_binary_digit),
        [b'0', digits @ ..] => all_of(digits, is_octal_digit),
        [b'1'..=b'9', digits @ ..] => digits.iter().all(|&c| is_digit(c)),
        _ => false,
    }
}

/// validate ^([a-zA-Z]([a-zA-Z0-9]|_[a-zA-Z0-9])*_?)$
pub(crate) fn validate_regex_22(s: &[u8]) -> bool {
    let Some((&first, tail)) = s.split_first() else {
        return false;
    };
    if !is_ident_start(first) {
        return false;
    }
    // an underscore must be followed by an alphanumeric character, unless it is the last character
    // of the string; that is the same as saying that no two underscores may be adjacent. As in
    // validate_regex_24, each character is checked against its predecessor without an early exit,
    // which lets the loop vectorize.
    let mut valid = true;
    for (&prev, &c) in s.iter().zip(tail) {
        valid &= is_ident_char(c) & !((prev == b'_') & (c == b'_'));
    }
    valid
}

/// validate ^(-?([0-9]+|MAX-TEXT-SIZE|ARRAY-SIZE))$
pub(crate) fn validate_regex_23(s: &[u8]) -> bool {
    let mut txt = s;
    if !txt.is_empty() && txt[0] == b'-' {
        txt = &txt[1..];
    }
    !txt.is_empty() && { txt.iter().all(u8::is_ascii_digit) || txt == b"MAX-TEXT-SIZE" || txt == b"ARRAY-SIZE" }
}

/// validate ^(/?[a-zA-Z][a-zA-Z0-9_]{0,127}(/[a-zA-Z][a-zA-Z0-9_]{0,127})*)$
pub(crate) fn validate_regex_24(s: &[u8]) -> bool {
    // skip the optional leading '/'; the rest must be '/'-separated segments
    let path = match s.split_first() {
        Some((&b'/', tail)) => tail,
        Some(_) => s,
        None => return false,
    };
    let Some((&first, tail)) = path.split_first() else {
        return false;
    };
    // the first character starts a segment, and the last character may not be a separator
    if !is_ident_start(first) || path[path.len() - 1] == b'/' {
        return false;
    }
    // every remaining character is validated against its predecessor: a character following a '/'
    // starts a new segment and must be a letter, any other character may be a segment character or
    // the next separator. As in validate_regex_8 the loop avoids early exits so that it vectorizes.
    let mut valid = true;
    for (&prev, &c) in path.iter().zip(tail) {
        let starts_segment = prev == b'/';
        valid &= (starts_segment & is_ident_start(c)) | (!starts_segment & (is_ident_char(c) | (c == b'/')));
    }
    valid
}

/// validate ^([0-9]+\.[0-9]+\.[0-9]+([\._;].*)?)$
pub(crate) fn validate_regex_25(s: &[u8]) -> bool {
    let Some((major, rest)) = split_first_at(s, |c| c == b'.') else {
        return false;
    };
    let Some((minor, rest)) = split_first_at(rest, |c| c == b'.') else {
        return false;
    };
    if !all_of(major, is_digit) || !all_of(minor, is_digit) {
        return false;
    }
    // the patch level, optionally followed by a separator and arbitrary text
    match split_first_at(rest, |c| matches!(c, b'.' | b'_' | b';')) {
        // '.' in the regex matches anything except a line break
        Some((patch, text)) => all_of(patch, is_digit) && !text.contains(&b'\n'),
        None => all_of(rest, is_digit),
    }
}

/// validate ^((0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-((0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(\+([0-9a-zA-Z-]+(\.[0-9a-zA-Z-]+)*))?)$
pub(crate) fn validate_regex_26(s: &[u8]) -> bool {
    /// `0|[1-9]\d*`
    fn is_numeric_id(s: &[u8]) -> bool {
        match s {
            b"0" => true,
            [b'1'..=b'9', digits @ ..] => digits.iter().all(|&c| is_digit(c)),
            _ => false,
        }
    }

    /// `[0-9a-zA-Z-]`
    fn is_id_char(c: u8) -> bool {
        is_ident_start(c) | is_digit(c) | (c == b'-')
    }

    /// `0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*`: a numeric id, or one with at least one non-digit
    fn is_prerelease_id(s: &[u8]) -> bool {
        is_numeric_id(s) || (all_of(s, is_id_char) && !s.iter().all(|&c| is_digit(c)))
    }

    // the build metadata starts at the first '+', which cannot appear anywhere else
    let (s, build) = match split_first_at(s, |c| c == b'+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (s, None),
    };
    if let Some(build) = build
        && !build.split(|&c| c == b'.').all(|id| all_of(id, is_id_char))
    {
        return false;
    }

    // the version core contains no '-', so the pre-release starts at the first one
    let (core, prerelease) = match split_first_at(s, |c| c == b'-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (s, None),
    };
    if let Some(prerelease) = prerelease
        && !prerelease.split(|&c| c == b'.').all(is_prerelease_id)
    {
        return false;
    }

    let mut parts = core.split(|&c| c == b'.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(major), Some(minor), Some(patch), None) => {
            is_numeric_id(major) && is_numeric_id(minor) && is_numeric_id(patch)
        }
        _ => false,
    }
}

/// validate ^([0-1])$
pub(crate) fn validate_regex_27(s: &[u8]) -> bool {
    s.len() == 1 && (s[0] == b'0' || s[0] == b'1')
}

/// validate ^((-?[a-zA-Z_]+)(( )+-?[a-zA-Z_]+)*)$
pub(crate) fn validate_regex_28(s: &[u8]) -> bool {
    let mut pos = 0;
    loop {
        // a word: an optional '-', then at least one letter or underscore
        if pos < s.len() && s[pos] == b'-' {
            pos += 1;
        }
        let word_start = pos;
        while pos < s.len() && (is_ident_start(s[pos]) || s[pos] == b'_') {
            pos += 1;
        }
        if pos == word_start {
            return false;
        }
        if pos == s.len() {
            return true;
        }

        // the words are separated by one or more spaces
        if s[pos] != b' ' {
            return false;
        }
        while pos < s.len() && s[pos] == b' ' {
            pos += 1;
        }
        if pos == s.len() {
            return false;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_regex_1() {
        /* regex: ^(0x[0-9a-z]*)$ */
        assert!(!validate_regex_1(b""));
        assert!(validate_regex_1(b"0x1234567890"));
        assert!(validate_regex_1(b"0xaAbBcCdDeEfF"));
        assert!(!validate_regex_1(b"12345"));
        /* the pattern requires at least one hex digit after the prefix */
        assert!(!validate_regex_1(b"0x"));
        assert!(!validate_regex_1(b"0X"));
        assert!(validate_regex_1(b"0X1"));
        assert!(!validate_regex_1(b"00x1"));
        assert!(!validate_regex_1(b"0x1g"));
    }

    #[test]
    fn test_regex_2() {
        /* regex: ^([1-9][0-9]*|0[xX][0-9a-fA-F]*|0[bB][0-1]+|0[0-7]*|UNSPECIFIED|UNKNOWN|BOOLEAN|PTR)$ */
        /* empty string */
        assert!(!validate_regex_2(b""));
        /* decimal */
        assert!(validate_regex_2(b"1234567890"));
        /* hex */
        assert!(validate_regex_2(b"0x1234567890abcdefABCDEF"));
        /* octal */
        assert!(validate_regex_2(b"01234567"));
        /* other */
        assert!(validate_regex_2(b"UNSPECIFIED"));
        assert!(validate_regex_2(b"UNKNOWN"));
        assert!(validate_regex_2(b"BOOLEAN"));
        assert!(validate_regex_2(b"PTR"));
        /* invalid hex */
        assert!(!validate_regex_2(b"0xghij"));
        /* invalid octal */
        assert!(!validate_regex_2(b"08"));
        /* invalid other */
        assert!(!validate_regex_2(b"hello world"));
        /* the hex digits are optional in this pattern, unlike the binary and decimal digits */
        assert!(validate_regex_2(b"0x"));
        assert!(validate_regex_2(b"0"));
        assert!(!validate_regex_2(b"0b"));
        assert!(validate_regex_2(b"0b0101"));
        assert!(!validate_regex_2(b"0b2"));
        assert!(!validate_regex_2(b"-1"));
        assert!(!validate_regex_2(b"UNKNOWNX"));
    }

    #[test]
    fn test_regex_3() {
        /* regex: ^([1-9][0-9]*|0[xX][0-9a-fA-F]+|0[0-7]*|0[bB][0-1]+|ANY|ALL)$ */
        /* empty string */
        assert!(!validate_regex_3(b""));
        /* decimal */
        assert!(validate_regex_3(b"1234567890"));
        /* hex */
        assert!(validate_regex_3(b"0x1234567890abcdefABCDEF"));
        /* octal */
        assert!(validate_regex_3(b"01234567"));
        /* other */
        assert!(validate_regex_3(b"ANY"));
        assert!(validate_regex_3(b"ALL"));
        /* invalid hex */
        assert!(!validate_regex_3(b"0xghij"));
        /* invalid octal */
        assert!(!validate_regex_3(b"08"));
        /* invalid other */
        assert!(!validate_regex_3(b"hello world"));
        /* here the hex digits are not optional */
        assert!(!validate_regex_3(b"0x"));
        assert!(validate_regex_3(b"0"));
        assert!(!validate_regex_3(b"0b"));
        assert!(validate_regex_3(b"0b0101"));
        assert!(!validate_regex_3(b"AN"));
        assert!(!validate_regex_3(b"ANYY"));
    }

    #[test]
    fn test_regex_4() {
        /* regex: ^([0-9]+|ANY)$ */
        /* matching: */
        assert!(validate_regex_4(b"ANY"));
        assert!(validate_regex_4(b"1234567890"));
        assert!(validate_regex_4(b"0123456789"));

        /* non-matching */
        assert!(!validate_regex_4(b""));
        assert!(!validate_regex_4(b"hello world"));
    }

    #[test]
    fn test_regex_5() {
        /* regex ^([0-9]+|STRING|ARRAY)$ */
        /* matching: */
        assert!(validate_regex_5(b"STRING"));
        assert!(validate_regex_5(b"ARRAY"));
        assert!(validate_regex_5(b"1234567890"));
        assert!(validate_regex_5(b"0123456789"));

        /* non-matching */
        assert!(!validate_regex_5(b""));
        assert!(!validate_regex_5(b"hello world"));
    }

    #[test]
    fn test_regex_6() {
        /* regex ^(0|1|true|false)$ */
        /* matching */
        assert!(validate_regex_6(b"0"));
        assert!(validate_regex_6(b"1"));
        assert!(validate_regex_6(b"true"));
        assert!(validate_regex_6(b"false"));

        /* non-matching */
        assert!(!validate_regex_6(b""));
        assert!(!validate_regex_6(b"2"));
        assert!(!validate_regex_6(b"hello world"));
    }

    #[test]
    fn test_regex_7() {
        /* regex ^([a-zA-Z_][a-zA-Z0-9_]*)$ */
        /* matching */
        assert!(validate_regex_7(b"Text_0"));
        assert!(validate_regex_7(b"_Text_1"));
        assert!(validate_regex_7(b"TEXT_9"));
        assert!(validate_regex_7(b"_"));
        assert!(validate_regex_7(b"a"));
        /* longer than one simd register, to cover both the vectorized loop and its remainder */
        assert!(validate_regex_7(b"_Text_0123456789_Text_0123456789_abc"));

        /* non-matching */
        assert!(!validate_regex_7(b""));
        assert!(!validate_regex_7(b"0Text_0"));
        assert!(!validate_regex_7(b"Text Text"));
        assert!(!validate_regex_7(b"Text-Text"));
        assert!(!validate_regex_7(b"_Text_0123456789_Text_0123456789_ab."));
        /* the characters adjacent to the accepted ranges */
        assert!(!validate_regex_7(b"@"));
        assert!(!validate_regex_7(b"["));
        assert!(!validate_regex_7(b"`"));
        assert!(!validate_regex_7(b"{"));
        assert!(!validate_regex_7(b"A:"));
        assert!(!validate_regex_7(&[b'_', 0xff]));
    }

    #[test]
    fn test_regex_8() {
        /* regex ^([a-zA-Z][a-zA-Z0-9_]*)$ */
        /* matching */
        assert!(validate_regex_8(b"Text_0"));
        assert!(validate_regex_8(b"TEXT_9"));
        assert!(validate_regex_8(b"a"));
        assert!(validate_regex_8(b"Z"));
        assert!(validate_regex_8(b"Text_"));
        /* longer than one simd register, to cover both the vectorized loop and its remainder */
        assert!(validate_regex_8(b"Text_0123456789_Text_0123456789_abc"));

        /* non-matching */
        assert!(!validate_regex_8(b""));
        assert!(!validate_regex_8(b"_Text_1"));
        assert!(!validate_regex_8(b"0Text_0"));
        assert!(!validate_regex_8(b"Text Text"));
        assert!(!validate_regex_8(b"Text-Text"));
        assert!(!validate_regex_8(b"/Text"));
        assert!(!validate_regex_8(b"Text_0123456789_Text_0123456789_ab."));
        /* the characters adjacent to the accepted ranges */
        assert!(!validate_regex_8(b"A@"));
        assert!(!validate_regex_8(b"A["));
        assert!(!validate_regex_8(b"A`"));
        assert!(!validate_regex_8(b"A{"));
        assert!(!validate_regex_8(b"A/"));
        assert!(!validate_regex_8(b"A:"));
        assert!(!validate_regex_8(&[b'A', 0xff]));
    }

    #[test]
    fn test_regex_9() {
        /* regex ^(([0-9]{4}-[0-9]{2}-[0-9]{2})(T[0-9]{2}:[0-9]{2}:[0-9]{2}(Z|([+\-][0-9]{2}:[0-9]{2})))?)$ */
        /* matching */
        assert!(validate_regex_9(b"2022-01-01T12:00:00Z"));
        assert!(validate_regex_9(b"2022-01-01T12:00:00+01:30"));
        assert!(validate_regex_9(b"2022-01-01T12:00:00-01:30"));
        assert!(validate_regex_9(b"2022-01-01"));

        /* non-matching */
        assert!(!validate_regex_9(b""));
        assert!(!validate_regex_9(b"202-01-01T12:00:00Z"));
        assert!(!validate_regex_9(b"2022-01-01T12:00:00"));
        assert!(!validate_regex_9(b"2022-01-01T12:00:00z"));
        assert!(!validate_regex_9(b"2022-01-01T12:00:00Z "));
        assert!(!validate_regex_9(b"2022-01-01 12:00:00Z"));
        assert!(!validate_regex_9(b"2022:01:01"));
        assert!(!validate_regex_9(b"2022-01-01T12:00:00+0130"));
    }

    #[test]
    fn test_regex_10() {
        /* regex ^([a-zA-Z][a-zA-Z0-9-]*)$ */
        /* matching */
        assert!(validate_regex_10(b"Text-0"));
        assert!(validate_regex_10(b"TEXT-9"));

        /* non-matching */
        assert!(!validate_regex_10(b""));
        assert!(!validate_regex_10(b"-"));
        assert!(!validate_regex_10(b"-Text-1"));
        assert!(!validate_regex_10(b"0Text_0"));
        assert!(!validate_regex_10(b"Text Text"));
    }

    #[test]
    fn test_regex_11() {
        /* regex ^([0-9a-zA-Z_\-]+)$ */
        /* matching */
        assert!(validate_regex_11(b"Text-0"));
        assert!(validate_regex_11(b"TEXT-9"));
        assert!(validate_regex_11(b"-"));
        assert!(validate_regex_11(b"_"));

        /* non-matching */
        assert!(!validate_regex_11(b""));
        assert!(!validate_regex_11(b"hello world"));
    }

    #[test]
    fn test_regex_12() {
        /* regex ^(%[ \-+#]?[0-9]*(\.[0-9]+)?[diouxXfeEgGcs])$ */
        /* matching */
        assert!(validate_regex_12(b"%B"));
        assert!(validate_regex_12(b"%d"));
        assert!(validate_regex_12(b"% 9.9f"));
        assert!(validate_regex_12(b"%s"));
        assert!(validate_regex_12(b"%#.9X"));
        assert!(validate_regex_12(b"%12.34s"));
        assert!(validate_regex_12(b"%-d"));
        assert!(validate_regex_12(b"%+d"));

        /* non-matching */
        assert!(!validate_regex_12(b""));
        assert!(!validate_regex_12(b"d"));
        /* the flag character may only appear before the field width */
        assert!(!validate_regex_12(b"%0-d"));
        assert!(!validate_regex_12(b"%12#34s"));
        assert!(!validate_regex_12(b"%--d"));
        assert!(!validate_regex_12(b"%.d"));
        assert!(!validate_regex_12(b"%d "));
        assert!(!validate_regex_12(b"%a"));
        assert!(!validate_regex_12(b"%"));
    }

    #[test]
    fn test_regex_13() {
        /* regex ^(0|[\+\-]?[1-9][0-9]*|0[xX][0-9a-fA-F]+|0[bB][0-1]+|0[0-7]+)$ */
        /* matching */
        assert!(validate_regex_13(b"0"));
        assert!(validate_regex_13(b"-19"));
        assert!(validate_regex_13(b"0XDEADBEEF"));
        assert!(validate_regex_13(b"0b010101"));
        assert!(validate_regex_13(b"+19"));

        /* non-matching */
        assert!(!validate_regex_13(b""));
        assert!(!validate_regex_13(b"-019"));
        assert!(!validate_regex_13(b"0XDEADBEEG"));
        assert!(!validate_regex_13(b"0b010102"));
        assert!(!validate_regex_13(b"-0"));
        assert!(!validate_regex_13(b"+0x1"));
        assert!(!validate_regex_13(b"0x"));
        assert!(!validate_regex_13(b"01238"));
    }

    #[test]
    fn test_regex_14() {
        /* regex ^((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)|ANY)$ */
        /* matching */
        assert!(validate_regex_14(b"192.168.0.1"));
        assert!(validate_regex_14(b"255.255.255.0"));
        assert!(validate_regex_14(b"ANY"));
        assert!(validate_regex_14(b"0.0.0.0"));
        assert!(validate_regex_14(b"099.199.249.255"));

        /* non-matching */
        assert!(!validate_regex_14(b""));
        assert!(!validate_regex_14(b"255.255.255.255.255"));
        assert!(!validate_regex_14(b"256.1.1.1"));
        assert!(!validate_regex_14(b"260.1.1.1"));
        assert!(!validate_regex_14(b"1.2.3"));
        assert!(!validate_regex_14(b"1.2.3."));
        assert!(!validate_regex_14(b"1234.1.1.1"));
    }

    #[test]
    fn test_regex_15() {
        /* regex ^([0-9A-Fa-f]{1,4}(:[0-9A-Fa-f]{1,4}){7,7}|ANY)$ */
        /* matching */
        assert!(validate_regex_15(b"fe80:0:abcd:1234:0:0:0:1"));
        assert!(validate_regex_15(b"ANY"));
        assert!(validate_regex_15(b"1:2:3:4:5:6:7:8"));

        /* non-matching */
        assert!(!validate_regex_15(b""));
        assert!(!validate_regex_15(b"fe80::abcd:1234::::1"));
        assert!(!validate_regex_15(b"1:2:3:4:5:6:7"));
        assert!(!validate_regex_15(b"1:2:3:4:5:6:7:8:9"));
        assert!(!validate_regex_15(b"12345:2:3:4:5:6:7:8"));
        assert!(!validate_regex_15(b"1:2:3:4:5:6:7:8:"));
    }

    #[test]
    fn test_regex_16() {
        /* regex ^((0[xX][0-9a-fA-F]+)|(0[0-7]+)|(0[bB][0-1]+)|(([+\-]?[1-9][0-9]+(\.[0-9]+)?|[+\-]?[0-9](\.[0-9]+)?)([eE]([+\-]?)[0-9]+)?)|\.0|INF|-INF|NaN)$ */
        /* matching */
        assert!(validate_regex_16(b"0xC0"));
        assert!(validate_regex_16(b"0777"));
        assert!(validate_regex_16(b"+1234"));
        assert!(validate_regex_16(b"-3.1415e-42"));
        assert!(validate_regex_16(b"INF"));
        assert!(validate_regex_16(b"NaN"));
        assert!(validate_regex_16(b"0"));
        assert!(validate_regex_16(b".0"));
        assert!(validate_regex_16(b"-INF"));
        assert!(validate_regex_16(b"0b1010"));
        assert!(validate_regex_16(b"1.5e10"));
        assert!(validate_regex_16(b"12345.6789e+0"));

        /* non-matching */
        assert!(!validate_regex_16(b""));
        assert!(!validate_regex_16(b"text"));
        assert!(!validate_regex_16(b"09"));
        assert!(!validate_regex_16(b"01.5"));
        assert!(!validate_regex_16(b"1."));
        assert!(!validate_regex_16(b"1e"));
        assert!(!validate_regex_16(b"1e+"));
        assert!(!validate_regex_16(b"1.2.3"));
        assert!(!validate_regex_16(b"+INF"));
        assert!(!validate_regex_16(b".5"));
    }

    #[test]
    fn test_regex_17() {
        /* regex ^(([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2})$ */
        /* matching */
        assert!(validate_regex_17(b"0A:1B:2C:3D:4E:5F"));

        /* non-matching */
        assert!(!validate_regex_17(b""));
        assert!(!validate_regex_17(b"0A:1B:2C:3D:4E"));
    }

    #[test]
    fn test_regex_18() {
        /* regex ^([a-zA-Z_][a-zA-Z0-9_]*(\[([a-zA-Z_][a-zA-Z0-9_]*|[0-9]+)\])*(\.[a-zA-Z_][a-zA-Z0-9_]*(\[([a-zA-Z_][a-zA-Z0-9_]*|[0-9]+)\])*)*)$ */
        /* matching */
        assert!(validate_regex_18(b"aabb9_cd[x][y].cde"));
        assert!(validate_regex_18(b"a"));
        assert!(validate_regex_18(b"_x9"));
        assert!(validate_regex_18(b"a[0][b].c"));

        /* non-matching */
        assert!(!validate_regex_18(b""));
        assert!(!validate_regex_18(b"42"));
        /* a subscript must be followed by '.', another subscript, or the end of the string */
        assert!(!validate_regex_18(b"a[0]b"));
        assert!(!validate_regex_18(b"a[]"));
        assert!(!validate_regex_18(b"a[0"));
        assert!(!validate_regex_18(b"a[0b]"));
        assert!(!validate_regex_18(b"a."));
        assert!(!validate_regex_18(b".a"));
        assert!(!validate_regex_18(b"a..b"));
    }

    #[test]
    fn test_regex_19() {
        /* regex ^([A-Z][a-zA-Z0-9_]*)$ */
        /* matching */
        assert!(validate_regex_19(b"Text_Text"));

        /* non-matching */
        assert!(!validate_regex_19(b""));
        assert!(!validate_regex_19(b"text"));
        assert!(!validate_regex_19(b"Text Text"));
    }

    #[test]
    fn test_regex_20() {
        /* regex  ^([1-9][0-9]*)$ */
        /* matching */
        assert!(validate_regex_20(b"123"));

        /* non-matching */
        assert!(!validate_regex_20(b""));
        assert!(!validate_regex_20(b"abcd"));
        assert!(!validate_regex_20(b"0x123"));
    }

    #[test]
    fn test_regex_21() {
        /* regex ^(0|[\+]?[1-9][0-9]*|0[xX][0-9a-fA-F]+|0[bB][0-1]+|0[0-7]+)$ */
        /* matching */
        assert!(validate_regex_21(b"0"));
        assert!(validate_regex_21(b"+19"));
        assert!(validate_regex_21(b"0xbadcafe"));
        assert!(validate_regex_21(b"0b1010"));
        assert!(validate_regex_21(b"0777"));

        /* non-matching */
        assert!(!validate_regex_21(b""));
        assert!(!validate_regex_21(b"-19"));
        assert!(!validate_regex_21(b"1.23"));
        assert!(!validate_regex_21(b"text"));
        assert!(!validate_regex_21(b"+0"));
        assert!(!validate_regex_21(b"+0x1"));
        assert!(!validate_regex_21(b"0x"));
        assert!(!validate_regex_21(b"01238"));
    }

    #[test]
    fn test_regex_22() {
        /* regex ^([a-zA-Z]([a-zA-Z0-9]|_[a-zA-Z0-9])*_?)$ */
        /* matching */
        assert!(validate_regex_22(b"text"));
        assert!(validate_regex_22(b"text_text"));
        assert!(validate_regex_22(b"a"));
        assert!(validate_regex_22(b"text_"));
        assert!(validate_regex_22(b"aZ9_0_"));

        /* non-matching */
        assert!(!validate_regex_22(b""));
        assert!(!validate_regex_22(b"_text"));
        assert!(!validate_regex_22(b"text__text"));
        assert!(!validate_regex_22(b"text__"));
        assert!(!validate_regex_22(b"text-text"));
        assert!(!validate_regex_22(b"9text"));
    }

    #[test]
    fn test_regex_23() {
        /* regex ^(-?([0-9]+|MAX-TEXT-SIZE|ARRAY-SIZE))$ */
        /* matching */
        assert!(validate_regex_23(b"-000"));
        assert!(validate_regex_23(b"33"));
        assert!(validate_regex_23(b"MAX-TEXT-SIZE"));
        assert!(validate_regex_23(b"ARRAY-SIZE"));

        /* non-matching */
        assert!(!validate_regex_23(b""));
        assert!(!validate_regex_23(b"text"));
        assert!(!validate_regex_23(b"1.23"));
    }

    #[test]
    fn test_regex_24() {
        /* regex ^(/?[a-zA-Z][a-zA-Z0-9_]{0,127}(/[a-zA-Z][a-zA-Z0-9_]{0,127})*)$ */
        /* matching */
        assert!(validate_regex_24(b"/path/to/element"));
        assert!(validate_regex_24(b"element_name"));
        assert!(validate_regex_24(b"path/to/element"));
        assert!(validate_regex_24(b"/a"));
        assert!(validate_regex_24(b"/Pkg_0/Sub9/Element_name_0123456789"));

        /* non-matching */
        assert!(!validate_regex_24(b""));
        assert!(!validate_regex_24(b"1234"));
        assert!(!validate_regex_24(b"/"));
        assert!(!validate_regex_24(b"//"));
        assert!(!validate_regex_24(b"/path/"));
        assert!(!validate_regex_24(b"/path//to"));
        assert!(!validate_regex_24(b"/path/0to"));
        assert!(!validate_regex_24(b"/path/_to"));
        assert!(!validate_regex_24(b"/1path"));
        assert!(!validate_regex_24(b"/path/to element"));
        assert!(!validate_regex_24(b"/path/to/element."));
    }

    #[test]
    fn test_regex_25() {
        /* regex ^([0-9]+\.[0-9]+\.[0-9]+([\._;].*)?)$ */
        /* matching */
        assert!(validate_regex_25(b"0.1.2_text"));
        assert!(validate_regex_25(b"1.2.3"));
        assert!(validate_regex_25(b"1.2.3;anything at all"));
        assert!(validate_regex_25(b"1.2.3."));

        /* non-matching */
        assert!(!validate_regex_25(b""));
        assert!(!validate_regex_25(b"text"));
        assert!(!validate_regex_25(b"12"));
        assert!(!validate_regex_25(b"1.2.3x"));
        assert!(!validate_regex_25(b"1.2."));
        assert!(!validate_regex_25(b".2.3"));
        /* '.' in the pattern does not match a line break */
        assert!(!validate_regex_25(b"1.2.3.a\nb"));
    }

    #[test]
    fn test_regex_26() {
        /* regex ^((0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-((0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(\+([0-9a-zA-Z-]+(\.[0-9a-zA-Z-]+)*))?)$ */
        /* matching */
        assert!(validate_regex_26(b"0.0.0-ab-c.0.0+zz-Z"));
        assert!(validate_regex_26(b"1.2.3-alpha.1"));
        assert!(validate_regex_26(b"1.2.3-0.3.7"));
        assert!(validate_regex_26(b"1.2.3+build.1"));
        assert!(validate_regex_26(b"1.2.3-beta+exp.sha.5114f85"));
        assert!(validate_regex_26(b"1.2.3-0a"));

        /* non-matching */
        assert!(!validate_regex_26(b""));
        assert!(!validate_regex_26(b"0"));
        assert!(!validate_regex_26(b"text"));
        /* a numeric pre-release identifier may not have a leading zero */
        assert!(!validate_regex_26(b"1.2.3-03.7"));
        assert!(!validate_regex_26(b"1.2.3-alpha.01"));
        assert!(!validate_regex_26(b"01.2.3"));
        assert!(!validate_regex_26(b"1.2.3-"));
        assert!(!validate_regex_26(b"1.2.3+"));
        assert!(!validate_regex_26(b"1.2.3-a..b"));
        assert!(!validate_regex_26(b"1.2.3.4"));
    }

    #[test]
    fn test_regex_27() {
        /* regex ^([0-1])$ */
        /* matching */
        assert!(validate_regex_27(b"0"));
        assert!(validate_regex_27(b"1"));

        /* non-matching */
        assert!(!validate_regex_27(b""));
        assert!(!validate_regex_27(b"text"));
        assert!(!validate_regex_27(b"10"));
    }

    #[test]
    fn test_regex_28() {
        /* regex ^((-?[a-zA-Z_]+)(( )+-?[a-zA-Z_]+)*)$ */
        /* matching */
        assert!(validate_regex_28(b"abc"));
        assert!(validate_regex_28(b"-abc"));
        assert!(validate_regex_28(b"a  b"));
        assert!(validate_regex_28(b"-a -b  c"));
        assert!(validate_regex_28(b"_a b_"));

        /* non-matching */
        assert!(!validate_regex_28(b"-text_text-Z__Z"));
        assert!(!validate_regex_28(b""));
        assert!(!validate_regex_28(b"--"));
        assert!(!validate_regex_28(b"1"));
        assert!(!validate_regex_28(b" a"));
        assert!(!validate_regex_28(b"a "));
        assert!(!validate_regex_28(b"a - b"));
        assert!(!validate_regex_28(b"--a"));
        assert!(!validate_regex_28(b"a1"));
    }
}
