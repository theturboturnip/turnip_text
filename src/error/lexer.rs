use lexer_rs::{LexerError, StreamCharPos};
use thiserror::Error;

use crate::{lexer::LineColumnChar, util::ParseSpan};

#[derive(Debug, Clone, Error)]
pub enum LexError {
    #[error("Too-long string (N={1}) of hyphen-minus characters - strings greater than three minuses must be escaped")]
    TooLongStringOfHyphenMinus(ParseSpan, usize),
}
// The lexer library forces us to implement LexerError - effectively a default failure for when none of the parser functions return true - but the turnip text lexer explicitly captures all non-special characters as normal text.
// The set of handled characters = union(special characters, other characters) = set of all characters
// Therefore we will never fail to parse.
impl LexerError<StreamCharPos<LineColumnChar>> for LexError {
    fn failed_to_parse(_state: StreamCharPos<LineColumnChar>, _ch: char) -> Self {
        unreachable!(
            "The turnip_text lexer is designed to accept all text - it should never fail to parse."
        )
    }
}
