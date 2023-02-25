use crate::lexer::{units_to_tokens, Unit};

use crate::lexer::{Escapable, LexError, LexPosn, LexToken, TTToken};
use lexer_rs::{Lexer, LexerOfStr};

pub type TextStream<'stream> = LexerOfStr<'stream, LexPosn, LexToken, LexError>;

/// A type mimicking [TTToken] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestTTToken<'a> {
    Newline,
    Escaped(Escapable),
    Backslash,
    CodeOpen(usize),
    CodeClose(usize),
    CodeCloseOwningBlock(usize),
    CodeCloseOwningInline(usize),
    CodeCloseOwningRaw(usize, usize),
    InlineScopeOpen,
    BlockScopeOpen,
    RawScopeOpen(usize),
    RawScopeClose(usize),
    ScopeClose,
    Hashes(usize),
    OtherText(&'a str),
}
impl<'a> TestTTToken<'a> {
    pub fn from_str_tok(data: &'a str, t: TTToken) -> Self {
        match t {
            TTToken::Newline(_) => Self::Newline,
            TTToken::Escaped(_, escapable) => Self::Escaped(escapable),
            TTToken::Backslash(_) => Self::Backslash,
            TTToken::CodeOpen(_, n) => Self::CodeOpen(n),
            TTToken::CodeClose(_, n) => Self::CodeClose(n),
            TTToken::CodeCloseOwningBlock(_, n) => Self::CodeCloseOwningBlock(n),
            TTToken::CodeCloseOwningInline(_, n) => Self::CodeCloseOwningInline(n),
            TTToken::CodeCloseOwningRaw(_, n_code, n_hashes) => {
                Self::CodeCloseOwningRaw(n_code, n_hashes)
            }
            TTToken::InlineScopeOpen(_) => Self::InlineScopeOpen,
            TTToken::BlockScopeOpen(_) => Self::BlockScopeOpen,
            TTToken::RawScopeOpen(_, n) => Self::RawScopeOpen(n),
            TTToken::RawScopeClose(_, n) => Self::RawScopeClose(n),
            TTToken::ScopeClose(_) => Self::ScopeClose,
            TTToken::Hashes(_, n) => Self::Hashes(n),
            TTToken::OtherText(span) => Self::OtherText(data[span.byte_range()].into()),
        }
    }
}

/// Run the lexer on a given piece of text, convert the lexed tokens to our test versions, and compare with the expected result.
fn expect_lex<'a>(data: &str, expected_stok_types: Vec<TestTTToken<'a>>) {
    println!("{:?}", data);

    // First step: lex
    let l = TextStream::new(data);
    let units: Vec<Unit> = l
        .iter(&[Box::new(Unit::parse_special), Box::new(Unit::parse_other)])
        .scan((), |_, x| x.ok())
        .collect();
    let stoks = units_to_tokens(units);
    let stok_types: Vec<TestTTToken> = stoks
        .iter()
        .map(|stok| TestTTToken::from_str_tok(data, *stok))
        .collect();

    assert_eq!(stok_types, expected_stok_types);
}

use TestTTToken::*;

#[test]
pub fn test_escaped_cr() {
    // '\' + '\r'&
    expect_lex(
        "sentence start, \\\rrest of sentence",
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
    )
}
#[test]
pub fn test_escaped_lf() {
    // '\' + '\n'
    expect_lex(
        "sentence start, \\\nrest of sentence",
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
    )
}
#[test]
pub fn test_escaped_crlf() {
    // '\' + '\r' + '\n'
    expect_lex(
        "sentence start, \\\r\nrest of sentence",
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
    )
}

#[test]
pub fn test_cr() {
    // '\r'
    expect_lex("\rcontent", vec![Newline, OtherText("content")])
}
#[test]
pub fn test_lf() {
    // '\n'
    expect_lex("\ncontent", vec![Newline, OtherText("content")])
}
#[test]
pub fn test_crlf() {
    // '\r' + '\n'
    expect_lex("\r\ncontent", vec![Newline, OtherText("content")])
}
