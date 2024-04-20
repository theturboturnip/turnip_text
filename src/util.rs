use std::ops::Range;

use lexer_rs::{PosnInCharStream, UserPosn};

use crate::lexer::LexPosn;

/// Helper struct representing the position of a character in a file, as both:
/// - Byte offset of the start of the UTF-8 code point
/// - (line, column) integers for display purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsePosn {
    pub byte_ofs: usize,
    pub char_ofs: usize,
    pub line: usize,
    pub column: usize,
}
impl From<LexPosn> for ParsePosn {
    fn from(p: LexPosn) -> Self {
        ParsePosn {
            byte_ofs: p.byte_ofs(),
            char_ofs: p.pos().char_ofs(),
            line: p.line(),
            column: p.column(),
        }
    }
}

/// Helper struct representing a span of characters between `start` (inclusive) and `end` (exclusive) in a file
/// TODO this is big and shouldn't be Copy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSpan {
    file_idx: usize,
    start: ParsePosn,
    end: ParsePosn,
}
impl ParseSpan {
    pub fn single_char(file_idx: usize, start: LexPosn, c: char) -> Self {
        Self {
            file_idx,
            start: start.clone().into(),
            end: start.advance_cols(c.len_utf8(), 1).into(),
        }
    }
    pub fn from_lex(file_idx: usize, start: LexPosn, end: LexPosn) -> Self {
        Self {
            file_idx,
            start: start.into(),
            end: end.into(),
        }
    }
    pub fn new(file_idx: usize, start: ParsePosn, end: ParsePosn) -> Self {
        Self {
            file_idx,
            start,
            end,
        }
    }
    pub fn byte_range(&self) -> Range<usize> {
        self.start.byte_ofs..self.end.byte_ofs
    }
    pub fn combine(&self, other: &ParseSpan) -> ParseSpan {
        assert_eq!(self.file_idx, other.file_idx);
        assert!(self.start.byte_ofs < other.end.byte_ofs);
        ParseSpan {
            file_idx: self.file_idx,
            start: self.start,
            end: other.end,
        }
    }
    pub fn annotate_snippets_range(&self) -> (usize, usize) {
        (self.start.char_ofs, self.end.char_ofs)
    }
    pub fn file_idx(&self) -> usize {
        self.file_idx
    }
    pub fn start(&self) -> ParsePosn {
        self.start
    }
    pub fn end(&self) -> ParsePosn {
        self.end
    }
}
