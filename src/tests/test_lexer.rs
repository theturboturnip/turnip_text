use crate::lexer::lex;

use crate::lexer::{Escapable, TTToken};

/// A type mimicking [TTToken] for test purposes
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TestTTToken<'a> {
    Newline,
    Escaped(Escapable),
    Backslash,
    CodeOpen(usize),
    CodeClose(usize),
    ScopeOpen,
    RawScopeOpen(usize),
    RawScopeClose(usize),
    ScopeClose,
    Hashes(usize),
    OtherText(&'a str),
    Whitespace(&'a str),
    EOF,
}
impl<'a> TestTTToken<'a> {
    pub fn from_str_tok(data: &'a str, t: TTToken) -> Self {
        match t {
            TTToken::Newline(_) => Self::Newline,
            TTToken::Escaped(_, escapable) => Self::Escaped(escapable),
            TTToken::Backslash(_) => Self::Backslash,
            TTToken::CodeOpen(_, n) => Self::CodeOpen(n),
            TTToken::CodeClose(_, n) => Self::CodeClose(n),
            TTToken::ScopeOpen(_) => Self::ScopeOpen,
            TTToken::RawScopeOpen(_, n) => Self::RawScopeOpen(n),
            TTToken::RawScopeClose(_, n) => Self::RawScopeClose(n),
            TTToken::ScopeClose(_) => Self::ScopeClose,
            TTToken::Hashes(_, n) => Self::Hashes(n),
            TTToken::OtherText(span) => Self::OtherText(data[span.byte_range()].into()),
            TTToken::Whitespace(span) => Self::Whitespace(data[span.byte_range()].into()),
            TTToken::EOF(_) => Self::EOF,
        }
    }
}

/// Run the lexer on a given piece of text, convert the lexed tokens to our test versions, and compare with the expected result.
pub fn expect_lex<'a>(data: &str, expected_stok_types: Vec<TestTTToken<'a>>) {
    println!("{:?}", data);

    // First step: lex
    let stoks: Vec<TTToken> = lex(0, data).scan((), |_, x| x.ok()).collect();
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
            CodeClose(1),
            ScopeOpen,
            Newline,
            txt("According"),
            sp,
            txt("to"),
            sp,
            txt("all"),
            sp,
            CodeOpen(1),
            txt("paren"),
            CodeClose(1),
            ScopeOpen,
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
            ScopeOpen,
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
            CodeClose(1),
            RawScopeOpen(3),
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
            CodeClose(3),
            ScopeOpen,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            CodeClose(1),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(2),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(3),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(7),
            ScopeOpen,
            ScopeClose,
            Newline,
            EOF,
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
            CodeClose(1),
            RawScopeOpen(1),
            Newline,
            CodeClose(2),
            RawScopeOpen(1),
            Newline,
            CodeClose(3),
            RawScopeOpen(1),
            Newline,
            CodeClose(7),
            RawScopeOpen(1),
            Newline,
            CodeClose(1),
            RawScopeOpen(4),
            Newline,
            CodeClose(2),
            RawScopeOpen(4),
            Newline,
            CodeClose(3),
            RawScopeOpen(4),
            Newline,
            CodeClose(7),
            RawScopeOpen(4),
            Newline,
            EOF,
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
            CodeClose(1),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(2),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(3),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(7),
            ScopeOpen,
            Newline,
            Newline,
            EOF,
        ],
    )
}

#[test]
pub fn test_inline_scope_open() {
    expect_lex(
        r#" { "#,
        vec![Whitespace(" "), ScopeOpen, Whitespace(" "), EOF],
    )
}

#[test]
pub fn test_block_scope_open() {
    expect_lex(
        r#"
{

"#,
        vec![Newline, ScopeOpen, Newline, Newline, EOF],
    )
}

#[test]
pub fn test_scope_close() {
    expect_lex(
        r#"
}
"#,
        vec![Newline, ScopeClose, Newline, EOF],
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
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
            EOF,
        ],
    )
}

#[test]
pub fn test_cr() {
    // '\r'
    expect_lex("\rcontent", vec![Newline, OtherText("content"), EOF])
}
#[test]
pub fn test_lf() {
    // '\n'
    expect_lex("\ncontent", vec![Newline, OtherText("content"), EOF])
}
#[test]
pub fn test_crlf() {
    // '\r' + '\n'
    expect_lex("\r\ncontent", vec![Newline, OtherText("content"), EOF])
}

// TODO test error messages for multibyte?
