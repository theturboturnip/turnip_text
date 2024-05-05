use std::ops::Range;

use lexer_rs::{PosnInCharStream, UserPosn};

use crate::interpreter::lexer::LexPosn;

/// Helper struct representing the position of a character in a file, as both:
/// - Byte offset of the start of the UTF-8 code point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsePosn {
    pub byte_ofs: usize,
}
impl From<LexPosn> for ParsePosn {
    fn from(p: LexPosn) -> Self {
        ParsePosn {
            byte_ofs: p.byte_ofs(),
        }
    }
}

/// Helper struct representing a span of characters between `start` (inclusive) and `end` (exclusive) in a file
///
/// ## Considering shrinking
/// This is kinda big, but we can't shrink it without lowering the size of `start`.
/// Because it has `usize` fields, the align is `sizeof(usize)`, and the size has to be padded to the align
/// (at least it does in C.)
/// So any optimization has to remove `sizeof(usize)`, my only idea was to make `end` a byte-delta = u8
/// but even then it would still get padded out.
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

#[derive(Debug, Clone, Copy)]
pub struct ParseContext {
    first_tok: ParseSpan,
    last_tok: ParseSpan,
}
impl ParseContext {
    pub fn new(first_tok: ParseSpan, last_tok: ParseSpan) -> Self {
        assert!(
            first_tok.file_idx() == last_tok.file_idx(),
            "Can't have a BuilderContext span two files"
        );
        Self {
            first_tok,
            last_tok,
        }
    }
    pub fn try_extend(&mut self, new_tok: &ParseSpan) -> bool {
        if new_tok.file_idx() == self.last_tok.file_idx() {
            assert!(self.first_tok.start().byte_ofs <= new_tok.start().byte_ofs);
            self.last_tok = *new_tok;
            true
        } else {
            false
        }
    }
    pub fn try_combine(&mut self, new_builder: ParseContext) -> bool {
        if new_builder.first_tok.file_idx() == self.first_tok.file_idx() {
            assert!(self.first_tok.start().byte_ofs <= new_builder.first_tok.start().byte_ofs);
            self.last_tok = new_builder.last_tok;
            true
        } else {
            false
        }
    }

    pub fn first_tok(&self) -> ParseSpan {
        self.first_tok
    }
    pub fn last_tok(&self) -> ParseSpan {
        self.last_tok
    }
    pub fn full_span(&self) -> ParseSpan {
        self.first_tok.combine(&self.last_tok)
    }
}
