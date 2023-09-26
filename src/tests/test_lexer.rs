use crate::lexer::{units_to_tokens, Unit};

use crate::lexer::{Escapable, LexError, LexPosn, LexToken, TTToken};
use lexer_rs::{Lexer, LexerOfStr};

pub type TextStream<'stream> = LexerOfStr<'stream, LexPosn, LexToken, LexError>;

/// A type mimicking [TTToken] for test purposes
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
    Whitespace(&'a str),
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
            TTToken::Whitespace(span) => Self::Whitespace(data[span.byte_range()].into()),
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
    let sp = Whitespace(" ");
    let txt = OtherText;

    expect_lex(
        r#"[quote]{
According to all [paren]{known} laws of aviation, there is {no way} a bee should be able to fly. \
The bee, of course, [code]###{flies} anyway because bees [[[emph]]]{don't} care what humans think is [["impossible"]].
}"#,
        vec![
            CodeOpen(1),
            txt("quote"),
            CodeCloseOwningBlock(1),
            txt("According"),
            sp,
            txt("to"),
            sp,
            txt("all"),
            sp,
            CodeOpen(1),
            txt("paren"),
            CodeCloseOwningInline(1),
            txt("known"),
            ScopeClose,
            sp,
            txt("laws"),
            sp,
            txt("of"),
            sp,
            txt("aviation,"),
            sp,
            txt("there"),
            sp,
            txt("is"),
            sp,
            InlineScopeOpen,
            txt("no"),
            sp,
            txt("way"),
            ScopeClose,
            sp,
            txt("a"),
            sp,
            txt("bee"),
            sp,
            txt("should"),
            sp,
            txt("be"),
            sp,
            txt("able"),
            sp,
            txt("to"),
            sp,
            txt("fly."),
            sp,
            Escaped(Escapable::Newline),
            txt("The"),
            sp,
            txt("bee,"),
            sp,
            txt("of"),
            sp,
            txt("course,"),
            sp,
            CodeOpen(1),
            txt("code"),
            CodeCloseOwningRaw(1, 3),
            txt("flies"),
            ScopeClose,
            sp,
            txt("anyway"),
            sp,
            txt("because"),
            sp,
            txt("bees"),
            sp,
            CodeOpen(3),
            txt("emph"),
            CodeCloseOwningInline(3),
            txt("don't"),
            ScopeClose,
            sp,
            txt("care"),
            sp,
            txt("what"),
            sp,
            txt("humans"),
            sp,
            txt("think"),
            sp,
            txt("is"),
            sp,
            CodeOpen(2),
            txt("\"impossible\""),
            CodeClose(2),
            txt("."),
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

#[test]
pub fn test_whitespace_newline_chain() {
    expect_lex(
        "    \n    \r    \r\n    ",
        vec![
            Whitespace("    "),
            Newline,
            Whitespace("    "),
            Newline,
            Whitespace("    "),
            Newline,
            Whitespace("    "),
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
        vec![Whitespace(" "), InlineScopeOpen, Whitespace(" ")],
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
        "before\\\rafter",
        vec![
            OtherText("before"),
            Escaped(Escapable::Newline),
            OtherText("after"),
        ],
    )
}
#[test]
pub fn test_escaped_lf() {
    // '\' + '\n'
    expect_lex(
        "before\\\nafter",
        vec![
            OtherText("before"),
            Escaped(Escapable::Newline),
            OtherText("after"),
        ],
    )
}
#[test]
pub fn test_escaped_crlf() {
    // '\' + '\r' + '\n'
    expect_lex(
        "before\\\r\nafter",
        vec![
            OtherText("before"),
            Escaped(Escapable::Newline),
            OtherText("after"),
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

// TODO test error messages for multibyte?
