use super::AutosarDataError;
use memchr::memchr;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq, Clone, Copy)]
#[non_exhaustive]
/// `ArxmlLexerError` contains all errors that can occur while reading data
pub enum ArxmlLexerError {
    /// Incomplete data, closing '>' was not found
    #[error("Incomplete data, closing '>' was not found")]
    IncompleteData,

    /// Invalid element: '<>'
    #[error("Invalid element: '<>'")]
    InvalidElement,

    /// A processing instruction was started with '<?', but it did not end with '?>'
    #[error("A processing instruction was started with '<?', but it did not end with '?>'")]
    InvalidProcessingInstruction,

    /// Invalid arxml header: The xml header of an arxml file must specify version="1.0", and encoding="utf-8" if it declares one
    #[error(
        "Invalid arxml header: The xml header of an arxml file must specify version=\"1.0\", and encoding=\"utf-8\" if it declares one"
    )]
    InvalidXmlHeader,

    /// Invalid comment: Comments must start with '<!--' and end with '-->'
    #[error("Invalid comment")]
    InvalidComment,
}

#[derive(Debug)]
pub(crate) enum ArxmlEvent<'a> {
    ArxmlHeader(Option<bool>),
    BeginElement(&'a [u8], &'a [u8]),
    EndElement(&'a [u8]),
    Characters(&'a [u8]),
    Comment(&'a [u8]),
    EndOfFile,
}

pub(crate) struct ArxmlLexer<'a> {
    buffer: &'a [u8],
    bufpos: usize,
    line: usize,
    deferred_end: Option<(usize, usize)>,
    sourcefile: PathBuf,
}

impl<'a> ArxmlLexer<'a> {
    pub(crate) fn new(buffer: &'a [u8], name: PathBuf) -> Self {
        // skip the byte-order mark, if it is present
        let bufpos = if buffer.len() > 3 && buffer[0] == 239 && buffer[1] == 187 && buffer[2] == 191 {
            3
        } else {
            0
        };
        Self {
            buffer,
            bufpos,
            line: 1,
            deferred_end: None,
            sourcefile: name,
        }
    }

    fn read_characters(&mut self) -> (ArxmlEvent<'a>, bool) {
        debug_assert!(self.bufpos < self.buffer.len());

        // the start of the next element '<' is the end of this block of characters
        let mut endpos = self.bufpos;
        let mut all_whitespace = true;
        while endpos < self.buffer.len() && self.buffer[endpos] != b'<' {
            // count the lines directly in this loop; it's faster than calling count_lines and this loop is quite hot in the profile...
            if !self.buffer[endpos].is_ascii_whitespace() {
                all_whitespace = false;
            } else if self.buffer[endpos] == b'\n' {
                self.line += 1;
            }
            endpos += 1;
        }
        debug_assert!(endpos > self.bufpos);

        let text = &self.buffer[self.bufpos..endpos];
        self.bufpos = endpos;
        (ArxmlEvent::Characters(text), all_whitespace)
    }

    fn read_element_start(&mut self, mut endpos: usize) -> ArxmlEvent<'a> {
        debug_assert!(self.bufpos < self.buffer.len());
        debug_assert!(endpos > self.bufpos + 1);
        debug_assert!(self.buffer[self.bufpos] == b'<');

        // this loop runs at most twice: once with the '>' found by next(), and - if that '>'
        // turned out to be part of an attribute value - once with the real end of the element
        let (text, elemname, attributes, is_end) = loop {
            let (text, is_end) = if self.buffer[endpos - 1] == b'/' {
                (&self.buffer[self.bufpos + 1..endpos - 1], true)
            } else {
                (&self.buffer[self.bufpos + 1..endpos], false)
            };

            let (elemname, attributes) = if let Some(splitpos) = text.iter().position(u8::is_ascii_whitespace) {
                (&text[..splitpos], &text[splitpos + 1..])
            } else {
                (text, &text[0..0])
            };

            // xml permits an unescaped '>' inside a quoted attribute value, e.g. <FOO S="a > b">.
            // Such a '>' terminated the search in next() too early, which shows up here as attribute
            // text that ends in the middle of a quoted value.
            let Some(quotechar) = unterminated_quote(attributes) else {
                break (text, elemname, attributes, is_end);
            };
            let Some(real_endpos) = self.find_quoted_element_end(endpos, quotechar) else {
                // The quoted value is not terminated, i.e. the input is not valid xml after all.
                break (text, elemname, attributes, is_end);
            };
            endpos = real_endpos;
        };

        // this is a <element/>, so a EndElement event needs to be generated next
        if is_end {
            // calculate the position of the element name inside the text
            self.deferred_end = Some((self.bufpos + 1, self.bufpos + 1 + elemname.len()));
        }

        self.line += count_lines(text);
        self.bufpos = endpos + 1;
        ArxmlEvent::BeginElement(elemname, attributes)
    }

    /// continue the search for the end of an element from inside a quoted attribute value
    ///
    /// Returns the position of the next '>' that is not inside a quoted attribute value,
    /// or None if the element does not end in a way that is valid xml.
    fn find_quoted_element_end(&self, startpos: usize, quotechar: u8) -> Option<usize> {
        let mut quotechar = Some(quotechar);
        for (idx, c) in self.buffer[startpos..].iter().enumerate() {
            if *c == b'<' {
                // '<' must be escaped everywhere in xml, so it can't be part of an attribute value.
                // The element is simply malformed and the search should not run on into the following
                // elements looking for a closing quote that doesn't exist.
                return None;
            } else if let Some(qc) = quotechar {
                if *c == qc {
                    quotechar = None;
                }
            } else if *c == b'>' {
                return Some(startpos + idx);
            } else if *c == b'"' || *c == b'\'' {
                quotechar = Some(*c);
            }
        }
        None
    }

    fn read_element_end(&mut self, endpos: usize) -> ArxmlEvent<'a> {
        debug_assert!(self.bufpos < self.buffer.len());
        debug_assert!(endpos > self.bufpos + 1);
        debug_assert!(self.buffer[self.bufpos] == b'<');

        let text = &self.buffer[self.bufpos + 2..endpos].trim_ascii_end();
        self.bufpos = endpos + 1;

        ArxmlEvent::EndElement(text)
    }

    fn read_xml_header(&mut self, endpos: usize) -> Option<Result<ArxmlEvent<'a>, AutosarDataError>> {
        debug_assert!(self.bufpos < self.buffer.len());
        debug_assert!(endpos > self.bufpos + 1);
        debug_assert!(self.buffer[self.bufpos] == b'<');

        if self.buffer[endpos - 1] != b'?' {
            return Some(Err(self.error(ArxmlLexerError::InvalidProcessingInstruction)));
        }

        let text = &self.buffer[self.bufpos + 2..endpos - 1];
        self.bufpos = endpos + 1;

        let text_trimmed = text.trim_ascii();
        let (elemname, mut rest) = if let Some(ws_pos) = text_trimmed.iter().position(|c| c.is_ascii_whitespace()) {
            (&text_trimmed[..ws_pos], &text_trimmed[ws_pos..])
        } else {
            (text_trimmed, &text_trimmed[text_trimmed.len()..])
        };

        let result = if elemname == b"xml" {
            let mut ver = &text[0..0];
            let mut encoding = &text[0..0];
            let mut standalone: Option<bool> = None;

            let valid = loop {
                rest = rest.trim_ascii_start();
                if rest.is_empty() {
                    break true;
                }

                let Some(eq_pos) = rest.iter().position(|c| *c == b'=') else {
                    break false;
                };

                let attr_name = rest[..eq_pos].trim_ascii_end();
                if attr_name.is_empty() || attr_name.iter().any(|c| c.is_ascii_whitespace()) {
                    break false;
                }

                rest = rest[eq_pos + 1..].trim_ascii_start();
                if rest.is_empty() || (rest[0] != b'"' && rest[0] != b'\'') {
                    break false;
                }

                let quote = rest[0];
                rest = &rest[1..];
                let Some(end_quote_pos) = rest.iter().position(|c| *c == quote) else {
                    break false;
                };

                let attr_val = &rest[..end_quote_pos];
                rest = &rest[end_quote_pos + 1..];

                if attr_name == b"version" {
                    ver = attr_val;
                } else if attr_name == b"encoding" {
                    encoding = attr_val;
                } else if attr_name == b"standalone" {
                    standalone = Some(attr_val == b"yes");
                }
            };

            // XML 1.0 5th ed. section 4.3.3: the encoding declaration is optional, and an entity
            // without one is UTF-8.
            let encoding_ok = encoding.is_empty()
                || encoding == b"utf-8"
                || encoding == b"UTF-8"
                || encoding == b"utf8"
                || encoding == b"UTF8";
            if !valid || ver != b"1.0" || !encoding_ok {
                Some(Err(self.error(ArxmlLexerError::InvalidXmlHeader)))
            } else {
                Some(Ok(ArxmlEvent::ArxmlHeader(standalone)))
            }
        } else {
            None
        };

        self.line += count_lines(text);
        result
    }

    fn read_comment(&mut self, endpos: usize) -> Result<ArxmlEvent<'a>, AutosarDataError> {
        debug_assert!(self.bufpos < self.buffer.len());
        debug_assert!(endpos > self.bufpos + 1);

        let startpos = self.bufpos;
        let text = &self.buffer[startpos..endpos];
        self.bufpos = endpos + 1;

        if text.len() < 6 || !text.starts_with(b"<!--") || !text.ends_with(b"--") {
            return Err(AutosarDataError::LexerError {
                filename: self.sourcefile.clone(),
                line: self.line,
                source: ArxmlLexerError::InvalidComment,
            });
        }
        self.line += count_lines(text);
        let comment = &self.buffer[startpos + 4..endpos - 2];
        Ok(ArxmlEvent::Comment(comment))
    }
}

impl ArxmlLexer<'_> {
    pub(crate) fn next<'a>(&'a mut self) -> Result<(usize, ArxmlEvent<'a>), AutosarDataError> {
        // if an <element/> was found, then a BeginElement event is returned first, and the EndElement is deferred and must be returned next
        if let Some((startpos, endpos)) = self.deferred_end {
            self.deferred_end = None;
            Ok((self.line, ArxmlEvent::EndElement(&self.buffer[startpos..endpos])))
        } else {
            loop {
                if self.bufpos == self.buffer.len() {
                    break Ok((self.line, ArxmlEvent::EndOfFile));
                } else if self.buffer[self.bufpos] == b'<' {
                    // start of an <element> or </element> or <!--comment-->
                    // find a '>' character
                    let findpos = memchr(b'>', &self.buffer[self.bufpos + 1..])
                        .ok_or_else(|| self.error(ArxmlLexerError::IncompleteData))?;
                    // endpos may be the position of a '>' that is part of an attribute value, but the
                    // call to read_element_start() will check for that and continue the search if necessary.
                    let endpos = self.bufpos + findpos + 1;

                    if endpos == self.bufpos + 1 {
                        // string is "<>"
                        return Err(self.error(ArxmlLexerError::InvalidElement));
                    }

                    // got a non-empty sequence of characters that starts with '<' and ends with '>'
                    match self.buffer[self.bufpos + 1] {
                        b'/' => {
                            // second char is '/' -> EndElement
                            return Ok((self.line, self.read_element_end(endpos)));
                        }
                        b'?' => {
                            // second char is '?' -> xml header or processing instruction
                            // processing instructions are ignored, read_xml_header returns None
                            if let Some(result) = self.read_xml_header(endpos) {
                                let value = result?;
                                return Ok((self.line, value));
                            }
                        }
                        b'!' => {
                            // second char is '!' -> parse a comment
                            // we found a '>' character, but comments are allowed to contain unquoted '<' and '>'
                            // this means we need to make sure the end is actually '-->', not just '>'
                            // XML conformance note: the Autosar standard defines a subset of XML and forbids
                            // CDATA and DOCTYPE sections, so those don't need to be handled here.
                            let mut comment_endpos = endpos;
                            while comment_endpos < self.buffer.len()
                                && !self.buffer[comment_endpos - 2..].starts_with(b"-->")
                            {
                                comment_endpos += 1;
                            }
                            if comment_endpos < self.buffer.len() {
                                return self.read_comment(comment_endpos).map(|res| (self.line, res));
                            } else {
                                // hit the end of the input -> unclosed comment
                                return Err(self.error(ArxmlLexerError::InvalidComment));
                            }
                        }
                        _ => {
                            // any other second char -> BeginElement
                            return Ok((self.line, self.read_element_start(endpos)));
                        }
                    }
                } else {
                    // start of character sequence
                    if let (ArxmlEvent::Characters(text), false) = self.read_characters() {
                        // found a character sequence which is not all whitespace
                        return Ok((self.line, ArxmlEvent::Characters(text)));
                    }
                }
                // loop if:
                // - a processing instruction was ignored
                // - empty character data found (whitespace only)
            }
        }
    }

    fn error(&self, err: ArxmlLexerError) -> AutosarDataError {
        AutosarDataError::LexerError {
            filename: self.sourcefile.clone(),
            line: self.line,
            source: err,
        }
    }
}

/// check if the attribute text ends inside a quoted value
///
/// Returns the quote character of the unterminated value, or None if all quoted values are terminated.
fn unterminated_quote(text: &[u8]) -> Option<u8> {
    let mut quotechar = None;
    for c in text {
        if let Some(qc) = quotechar {
            if *c == qc {
                quotechar = None;
            }
        } else if *c == b'"' || *c == b'\'' {
            quotechar = Some(*c);
        }
    }
    quotechar
}

fn count_lines(text: &[u8]) -> usize {
    text.iter().filter(|c| **c == b'\n').count()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let data =
            b"<?xml version=\"1.0\" encoding=\"utf-8\"?><element attr=\"gggg\" attr3>contained characters</element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::ArxmlHeader(None)))));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs.len() == 17)
        );
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Characters(text))) if text == b"contained characters"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndElement(elem))) if elem == b"element"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndOfFile))));
    }

    #[test]
    fn skip_byte_order_mark() {
        let data =
            b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"utf-8\"?><element attr=\"gggg\" attr3>contained characters</element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::ArxmlHeader(None)))));
    }

    #[test]
    fn test_incomplete_data() {
        let data = b"<element";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError {source, ..}) if source == ArxmlLexerError::IncompleteData)
        );
    }

    #[test]
    fn test_invalid_element() {
        let data = b"<element><>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(lexer.next().is_ok());
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidElement)
        );
    }

    #[test]
    fn test_invalid_processing_instruction() {
        let data = b"<element><?what>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(lexer.next().is_ok());
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidProcessingInstruction)
        );
    }

    #[test]
    fn test_comment() {
        let data = b"<!-- foo--><element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Comment(_)))));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(_elem, _attrs)))));
    }

    #[test]
    fn test_invalid_comment() {
        let data = b"<element><!-- foo>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(lexer.next().is_ok());
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidComment)
        );
    }

    #[test]
    fn test_invalid_xml_header() {
        let data = br#"<?xml version="1.0" encoding="cp1252"?>"#;
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidXmlHeader)
        );

        let data = br#"<?xml ?>"#;
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidXmlHeader)
        );
    }

    #[test]
    fn test_malformed_xml_header_attributes() {
        // each of these breaks out of the attribute loop of read_xml_header in a different place
        let malformed: &[&[u8]] = &[
            // an attribute without a value: there is no '=' left in the remaining text
            br#"<?xml version="1.0" encoding="utf-8" standalone?>"#,
            // an empty attribute name
            br#"<?xml ="1.0"?>"#,
            // an attribute name containing whitespace
            br#"<?xml version number="1.0"?>"#,
            // an unquoted attribute value
            br#"<?xml version=1.0?>"#,
            // an attribute value whose closing quote is missing
            br#"<?xml version="1.0?>"#,
        ];
        for data in malformed {
            println!("checking: {}", String::from_utf8_lossy(data));
            let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
            assert!(
                matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidXmlHeader)
            );
        }
    }

    #[test]
    fn test_xml_header_without_encoding() {
        // the encoding declaration is optional; some tools write the header without it
        let data = br#"<?xml version="1.0" ?><element>"#;
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::ArxmlHeader(None)))));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, _))) if elem == b"element"));
    }

    #[test]
    fn test_xml_header_with_single_quotes_and_standalone() {
        let data = br#"<?xml version='1.0' encoding='UTF8' standalone='yes'?><element>"#;
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::ArxmlHeader(Some(true))))));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, _))) if elem == b"element"));
    }

    #[test]
    fn test_processing_instruction_is_ignored() {
        // a processing instruction which is not the xml header is skipped, and the lexer continues
        // with the next event
        let data = b"<?php echo 'hello'; ?><element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, _))) if elem == b"element"));
    }

    #[test]
    fn test_malformed_comment() {
        // '<!' is treated as the start of a comment, but this is not one, even though it ends with '-->'
        let data = b"<!DOCTYPE something -->";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidComment)
        );

        // a comment which is too short to contain both the opening and the closing marker
        let data = b"<!-->";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Err(AutosarDataError::LexerError{source, ..}) if source == ArxmlLexerError::InvalidComment)
        );
    }

    #[test]
    fn traits() {
        // ArxmlLexerError: Debug, Error, Eq, PartialEq, Clone
        let err = ArxmlLexerError::IncompleteData;
        let err2 = err;
        assert_eq!(err, err2);
        assert_eq!(format!("{err:#?}"), format!("{err2:#?}"));
        assert_eq!(format!("{err}"), format!("{err2}"));

        // ArxmlEvent: Debug
        let event = ArxmlEvent::ArxmlHeader(None);
        let _ = format!("{event:#?}");
    }

    /// github issue #15 - comments can contain '<' and '>'
    #[test]
    fn test_w3c_comment_example() {
        let data = b"<!-- declarations for <head> & <body> -->";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Comment(_)))));
    }

    /// xml allows an unescaped '>' inside a quoted attribute value
    #[test]
    fn test_gt_in_attribute_value() {
        // double quoted value containing '>'
        let data = b"<element attr=\"a>b\">text</element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"attr=\"a>b\"")
        );
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Characters(text))) if text == b"text"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndElement(elem))) if elem == b"element"));

        // single quoted value containing '>' and '"'
        let data = b"<element attr='a>\"b'>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"attr='a>\"b'")
        );

        // several attributes, only the last one contains '>'
        let data = b"<element a=\"1\" b='2' c=\"3>4\" d=\"5\">";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"a=\"1\" b='2' c=\"3>4\" d=\"5\"")
        );

        // empty element, whose attribute value contains "/>"
        let data = b"<element attr=\"a/>b\"/>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"attr=\"a/>b\"")
        );
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndElement(elem))) if elem == b"element"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndOfFile))));

        // a newline inside the attribute value is still counted
        let data = b"<element attr=\"a>\nb\"><x>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((1, ArxmlEvent::BeginElement(_, _)))));
        assert!(matches!(lexer.next(), Ok((2, ArxmlEvent::BeginElement(elem, _))) if elem == b"x"));
    }

    /// an attribute value that is never terminated: the element text is truncated at the first '>',
    /// so that the parser can report the error and the following elements are still parsed
    #[test]
    fn test_unterminated_attribute_value() {
        // the search for the end of the value stops at the '<' of the next element
        let data = b"<element attr=\"abc>text</element>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"attr=\"abc")
        );
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Characters(text))) if text == b"text"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndElement(elem))) if elem == b"element"));

        // the search for the end of the value runs to the end of the input
        let data = b"<element attr=\"abc>text";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(
            matches!(lexer.next(), Ok((_, ArxmlEvent::BeginElement(elem, attrs))) if elem == b"element" && attrs == b"attr=\"abc")
        );
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::Characters(text))) if text == b"text"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::EndOfFile))));
    }

    /// github issue #32 - extra spaces in the XML header should be tolerated
    #[test]
    fn test_xml_header_with_extra_spaces() {
        let data = b"<?xml   version =   \"1.0\"   encoding =   \"utf-8\"   standalone = \"yes\"  ?>";
        let mut lexer = ArxmlLexer::new(data, PathBuf::from("(buffer)"));
        assert!(matches!(lexer.next(), Ok((_, ArxmlEvent::ArxmlHeader(Some(true))))));
    }
}
