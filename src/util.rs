use std::ops::Range;

use lexer_rs::{PosnInCharStream, UserPosn};

use crate::lexer::LexPosn;

/// Helper struct representing the position of a character in a file, as both:
/// - Byte offset of the start of the UTF-8 code point
/// - (line, column) integers for display purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsePosn {
    pub byte_ofs: usize,
    pub line: usize,
    pub column: usize,
}
impl From<LexPosn> for ParsePosn {
    fn from(p: LexPosn) -> Self {
        ParsePosn {
            byte_ofs: p.byte_ofs(),
            line: p.line(),
            column: p.column(),
        }
    }
}

/// Helper struct representing a span of characters between `start` (inclusive) and `end` (exclusive) in a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSpan {
    pub start: ParsePosn,
    pub end: ParsePosn,
}
impl ParseSpan {
    pub fn from_lex(start: LexPosn, end: LexPosn) -> Self {
        Self {
            start: start.into(),
            end: end.into()
        }
    }
    pub fn new(start: ParsePosn, end: ParsePosn) -> Self {
        Self {
            start,
            end
        }
    }
    pub fn byte_range(&self) -> Range<usize> {
        self.start.byte_ofs..self.end.byte_ofs
    } 
}
