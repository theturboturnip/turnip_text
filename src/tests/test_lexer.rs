use crate::interpreter::lexer::{lex, LexError};

use crate::interpreter::lexer::{Escapable, TTToken};

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
    HyphenMinuses(usize),
    EnDash,
    EmDash,
}
impl<'a> TestTTToken<'a> {
    fn from_str_tok(data: &'a str, t: TTToken) -> Self {
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
            TTToken::HyphenMinuses(_, n) => Self::HyphenMinuses(n),
            TTToken::EnDash(_) => Self::EnDash,
            TTToken::EmDash(_) => Self::EmDash,
        }
    }
}

/// Run the lexer on a given piece of text, convert the lexed tokens to our test versions, and compare with the expected result.
pub fn expect_lex<'a>(data: &str, expected_stok_types: Vec<TestTTToken<'a>>) {
    println!("{:?}", data);

    // First step: lex
    let stoks: Vec<TTToken> = lex(0, data).unwrap().collect();
    let stok_types: Vec<TestTTToken> = stoks
        .iter()
        .map(|stok| TestTTToken::from_str_tok(data, *stok))
        .collect();

    assert_eq!(stok_types, expected_stok_types);
}

pub fn expect_lex_nul_err(data: &str) {
    match lex(0, data) {
        Ok(_) => {
            dbg!(data);
            panic!("Expected lexer result to be Err(NullByteFound), got a valid lex");
        }
        Err(LexError::NullByteFound) => {}
    }
}

use TestTTToken::*;

#[test]
fn integration_test() {
    let sp = Whitespace(" ");
    let txt = OtherText;

    expect_lex(
        r#"[quote]{
According to all [paren]{known} laws of aviation, there is {no way} a bee should be able to fly. \
The bee, of course, [code]###{flies} anyway because bees [--emph--]{don't} care what humans think is [-"impossible"-].
}"#,
        vec![
            CodeOpen(0),
            txt("quote"),
            CodeClose(0),
            ScopeOpen,
            Newline,
            txt("According"),
            sp,
            txt("to"),
            sp,
            txt("all"),
            sp,
            CodeOpen(0),
            txt("paren"),
            CodeClose(0),
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
            CodeOpen(0),
            txt("code"),
            CodeClose(0),
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
            CodeOpen(2),
            txt("emph"),
            CodeClose(2),
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
            CodeOpen(1),
            txt("\"impossible\""),
            CodeClose(1),
            txt("."),
            Newline,
            ScopeClose,
            EOF,
        ],
    )
}

/// Test \n, \r, \r\n
#[test]
fn test_newline() {
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
fn test_whitespace_newline_chain() {
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
fn test_escaped() {
    expect_lex(
        r#"
\

\\
\[
\]
\{
\}
\#
\-
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
            Escaped(Escapable::HyphenMinus),
            Newline,
            EOF,
        ],
    )
}

/// Test backslashes when they don't escape anything
///
/// e.g. \a, \b, \c
#[test]
fn test_backslash() {
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
fn test_code_open() {
    expect_lex(
        r#"
[
[-
[--
[------
"#,
        vec![
            Newline,
            CodeOpen(0),
            Newline,
            CodeOpen(1),
            Newline,
            CodeOpen(2),
            Newline,
            CodeOpen(6),
            Newline,
            EOF,
        ],
    )
}

#[test]
fn test_code_close() {
    expect_lex(
        r#"
]
-]
--]
------]
"#,
        vec![
            Newline,
            CodeClose(0),
            Newline,
            CodeClose(1),
            Newline,
            CodeClose(2),
            Newline,
            CodeClose(6),
            Newline,
            EOF,
        ],
    )
}

#[test]
fn test_code_close_owning_inline() {
    expect_lex(
        r#"
]{}
-]{}
--]{}
------]{}
"#,
        vec![
            Newline,
            CodeClose(0),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(1),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(2),
            ScopeOpen,
            ScopeClose,
            Newline,
            CodeClose(6),
            ScopeOpen,
            ScopeClose,
            Newline,
            EOF,
        ],
    )
}

#[test]
fn test_code_close_owning_raw() {
    expect_lex(
        r#"
]#{
-]#{
--]#{
------]#{
]####{
-]####{
--]####{
------]####{
"#,
        vec![
            Newline,
            CodeClose(0),
            RawScopeOpen(1),
            Newline,
            CodeClose(1),
            RawScopeOpen(1),
            Newline,
            CodeClose(2),
            RawScopeOpen(1),
            Newline,
            CodeClose(6),
            RawScopeOpen(1),
            Newline,
            CodeClose(0),
            RawScopeOpen(4),
            Newline,
            CodeClose(1),
            RawScopeOpen(4),
            Newline,
            CodeClose(2),
            RawScopeOpen(4),
            Newline,
            CodeClose(6),
            RawScopeOpen(4),
            Newline,
            EOF,
        ],
    )
}

#[test]
fn test_code_close_owning_block() {
    expect_lex(
        r#"
]{

-]{

--]{

------]{

"#,
        vec![
            Newline,
            CodeClose(0),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(1),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(2),
            ScopeOpen,
            Newline,
            Newline,
            CodeClose(6),
            ScopeOpen,
            Newline,
            Newline,
            EOF,
        ],
    )
}

#[test]
fn test_inline_scope_open() {
    expect_lex(
        r#" { "#,
        vec![Whitespace(" "), ScopeOpen, Whitespace(" "), EOF],
    )
}

#[test]
fn test_block_scope_open() {
    expect_lex(
        r#"
{

"#,
        vec![Newline, ScopeOpen, Newline, Newline, EOF],
    )
}

#[test]
fn test_scope_close() {
    expect_lex(
        r#"
}
"#,
        vec![Newline, ScopeClose, Newline, EOF],
    )
}

#[test]
fn test_raw_scope_open() {
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
fn test_raw_scope_close() {
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
fn test_hashes() {
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
fn test_escaped_cr() {
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
fn test_escaped_lf() {
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
fn test_escaped_crlf() {
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
fn test_cr() {
    // '\r'
    expect_lex("\rcontent", vec![Newline, OtherText("content"), EOF])
}
#[test]
fn test_lf() {
    // '\n'
    expect_lex("\ncontent", vec![Newline, OtherText("content"), EOF])
}
#[test]
fn test_crlf() {
    // '\r' + '\n'
    expect_lex("\r\ncontent", vec![Newline, OtherText("content"), EOF])
}

// Test endash, hyphenminuses(1), emdash
#[test]
fn test_short_hyphen_strings() {
    expect_lex(
        r"
-
\-

--
\--
-\-

---
\---
-\--
--\-
\-\--
-\-\-
\-\-\-

\-\-\-\-\-\-\-\-",
        vec![
            Newline,
            HyphenMinuses(1),
            Newline,
            Escaped(Escapable::HyphenMinus),
            Newline,
            Newline,
            // 2-dash combinations
            EnDash,
            Newline,
            // \-- = escaped, single
            Escaped(Escapable::HyphenMinus),
            HyphenMinuses(1),
            Newline,
            // -\- = single, escaped
            HyphenMinuses(1),
            Escaped(Escapable::HyphenMinus),
            Newline,
            Newline,
            // 3-dash combinations
            EmDash,
            Newline,
            // \--- = escaped + double
            Escaped(Escapable::HyphenMinus),
            EnDash,
            Newline,
            // -\-- = single, escaped, single
            HyphenMinuses(1),
            Escaped(Escapable::HyphenMinus),
            HyphenMinuses(1),
            Newline,
            // --\- = double, escaped
            EnDash,
            Escaped(Escapable::HyphenMinus),
            Newline,
            // \-\-- = escaped, escaped, single
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            HyphenMinuses(1),
            Newline,
            // -\-\- = single, escaped, escaped
            HyphenMinuses(1),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Newline,
            // \-\-\- = three escaped
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Newline,
            Newline,
            // \-\-\-\-\-\-\-\- = seven escaped
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            Escaped(Escapable::HyphenMinus),
            EOF,
        ],
    );
}

#[test]
fn test_long_hyphen_strings() {
    expect_lex("----", vec![HyphenMinuses(4), EOF]);
    expect_lex("-----", vec![HyphenMinuses(5), EOF]);
    expect_lex("------", vec![HyphenMinuses(6), EOF]);
    expect_lex("-------", vec![HyphenMinuses(7), EOF]);
    expect_lex("--------", vec![HyphenMinuses(8), EOF]);
    expect_lex("---------", vec![HyphenMinuses(9), EOF]);
    expect_lex("----------", vec![HyphenMinuses(10), EOF]);
}

#[test]
fn test_nul_byte_not_allowed() {
    // Test the nul byte both next to special characters and non-special characters,
    // because there are two parsing functions that both need to reject it.
    expect_lex_nul_err(
        "diuwabdouwbdoqwbd\0qdwpiqbdjl wb
    
    [[-\0----- ]---- #{{{##} qdubwdqwd \n\t\r\r\n\t\0",
    );
}
