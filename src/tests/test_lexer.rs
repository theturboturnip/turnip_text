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
pub fn integration_test() {
    expect_lex(
        r#"[quote]{
According to all [paren]{known} laws of aviation, there is {no way} a bee should be able to fly. \
The bee, of course, [code]###{flies} anyway because bees [[[emph]]]{don't} care what humans think is [["impossible"]].
}"#,
        vec![
            CodeOpen(1),
            OtherText("quote"),
            CodeCloseOwningBlock(1),
            OtherText("According to all "),
            CodeOpen(1),
            OtherText("paren"),
            CodeCloseOwningInline(1),
            OtherText("known"),
            ScopeClose,
            OtherText(" laws of aviation, there is "),
            InlineScopeOpen,
            OtherText("no way"),
            ScopeClose,
            OtherText(" a bee should be able to fly. "),
            Escaped(Escapable::Newline),
            OtherText("The bee, of course, "),
            CodeOpen(1),
            OtherText("code"),
            CodeCloseOwningRaw(1, 3),
            OtherText("flies"),
            ScopeClose,
            OtherText(" anyway because bees "),
            CodeOpen(3),
            OtherText("emph"),
            CodeCloseOwningInline(3),
            OtherText("don't"),
            ScopeClose,
            OtherText(" care what humans think is "),
            CodeOpen(2),
            OtherText("\"impossible\""),
            CodeClose(2),
            OtherText("."),
            Newline,
            ScopeClose,
        ],
    )
}

/// Test \n, \r, \r\n
#[test]
pub fn test_newline() {
    expect_lex(
        "a\nb\rc\r\n",
        vec![
            OtherText("a"),
            Newline,
            OtherText("b"),
            Newline,
            OtherText("c"),
            Newline,
        ],
    )
}

/// Test escaped things
#[test]
pub fn test_escaped() {
    expect_lex(
        r#"
\

\\
\[
\]
\{
\}
\#
"#,
        vec![
            Newline,
            Escaped(Escapable::Newline),
            Newline,
            Escaped(Escapable::Backslash),
            Newline,
            Escaped(Escapable::SqrOpen),
            Newline,
            Escaped(Escapable::SqrClose),
            Newline,
            Escaped(Escapable::SqgOpen),
            Newline,
            Escaped(Escapable::SqgClose),
            Newline,
            Escaped(Escapable::Hash),
            Newline,
        ],
    )
}

/// Test backslashes when they don't escape anything
///
/// e.g. \a, \b, \c
#[test]
pub fn test_backslash() {
    expect_lex(
        r#"\a\b\c"#,
        vec![
            Backslash,
            OtherText("a"),
            Backslash,
            OtherText("b"),
            Backslash,
            OtherText("c"),
        ],
    )
}

#[test]
pub fn test_code_open() {
    expect_lex(
        r#"
[
[[
[[[
[[[[[[[
"#,
        vec![
            Newline,
            CodeOpen(1),
            Newline,
            CodeOpen(2),
            Newline,
            CodeOpen(3),
            Newline,
            CodeOpen(7),
            Newline,
        ],
    )
}

#[test]
pub fn test_code_close() {
    expect_lex(
        r#"
]
]]
]]]
]]]]]]]
"#,
        vec![
            Newline,
            CodeClose(1),
            Newline,
            CodeClose(2),
            Newline,
            CodeClose(3),
            Newline,
            CodeClose(7),
            Newline,
        ],
    )
}

#[test]
pub fn test_code_close_owning_inline() {
    expect_lex(
        r#"
]{}
]]{}
]]]{}
]]]]]]]{}
"#,
        vec![
            Newline,
            CodeCloseOwningInline(1),
            ScopeClose,
            Newline,
            CodeCloseOwningInline(2),
            ScopeClose,
            Newline,
            CodeCloseOwningInline(3),
            ScopeClose,
            Newline,
            CodeCloseOwningInline(7),
            ScopeClose,
            Newline,
        ],
    )
}

#[test]
pub fn test_code_close_owning_raw() {
    expect_lex(
        r#"
]#{
]]#{
]]]#{
]]]]]]]#{
]####{
]]####{
]]]####{
]]]]]]]####{
"#,
        vec![
            Newline,
            CodeCloseOwningRaw(1, 1),
            Newline,
            CodeCloseOwningRaw(2, 1),
            Newline,
            CodeCloseOwningRaw(3, 1),
            Newline,
            CodeCloseOwningRaw(7, 1),
            Newline,
            CodeCloseOwningRaw(1, 4),
            Newline,
            CodeCloseOwningRaw(2, 4),
            Newline,
            CodeCloseOwningRaw(3, 4),
            Newline,
            CodeCloseOwningRaw(7, 4),
            Newline,
        ],
    )
}

#[test]
pub fn test_code_close_owning_block() {
    expect_lex(
        r#"
]{

]]{

]]]{

]]]]]]]{

"#,
        vec![
            Newline,
            CodeCloseOwningBlock(1),
            Newline,
            CodeCloseOwningBlock(2),
            Newline,
            CodeCloseOwningBlock(3),
            Newline,
            CodeCloseOwningBlock(7),
            Newline,
        ],
    )
}

#[test]
pub fn test_inline_scope_open() {
    expect_lex(
        r#" { "#,
        vec![OtherText(" "), InlineScopeOpen, OtherText(" ")],
    )
}

#[test]
pub fn test_block_scope_open() {
    expect_lex(
        r#"
{

"#,
        vec![Newline, BlockScopeOpen, Newline],
    )
}

#[test]
pub fn test_scope_close() {
    expect_lex(
        r#"
}
"#,
        vec![Newline, ScopeClose, Newline],
    )
}

#[test]
pub fn test_raw_scope_open() {
    expect_lex(
        r#"
#{
##{
###{
#######{
"#,
        vec![
            Newline,
            RawScopeOpen(1),
            Newline,
            RawScopeOpen(2),
            Newline,
            RawScopeOpen(3),
            Newline,
            RawScopeOpen(7),
            Newline,
        ],
    )
}

#[test]
pub fn test_raw_scope_close() {
    expect_lex(
        r#"
}#
}##
}###
}#######
"#,
        vec![
            Newline,
            RawScopeClose(1),
            Newline,
            RawScopeClose(2),
            Newline,
            RawScopeClose(3),
            Newline,
            RawScopeClose(7),
            Newline,
        ],
    )
}

#[test]
pub fn test_hashes() {
    expect_lex(
        r#"
#
##
###
#######
"#,
        vec![
            Newline,
            Hashes(1),
            Newline,
            Hashes(2),
            Newline,
            Hashes(3),
            Newline,
            Hashes(7),
            Newline,
        ],
    )
}

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
