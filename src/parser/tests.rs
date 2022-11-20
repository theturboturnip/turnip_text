use crate::parser::parse_simple_tokens;

use super::{
    lexer::{Escapable, SimpleToken},
    ParseError, Token,
};
use lexer_rs::{Lexer, LexerOfStr, SimpleParseError, StreamCharPos};

type TextPos = StreamCharPos<usize>;
type LexToken = SimpleToken<TextPos>;
type LexError = SimpleParseError<TextPos>;
type TextStream<'stream> = LexerOfStr<'stream, TextPos, LexToken, LexError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleTokenType<'a> {
    Newline,
    Escaped(Escapable),
    Backslash,
    CodeOpen(usize),
    CodeClose(usize),
    ScopeOpen(usize),
    RawScopeOpen(usize),
    ScopeClose(usize),
    Hashes(usize),
    OtherText(&'a str),
}
impl<'a> SimpleTokenType<'a> {
    fn from_str_tok(data: &'a str, t: SimpleToken<StreamCharPos<usize>>) -> Self {
        match t {
            SimpleToken::Newline(_) => Self::Newline,
            SimpleToken::Escaped(_, escapable) => Self::Escaped(escapable),
            SimpleToken::Backslash(_) => Self::Backslash,
            SimpleToken::CodeOpen(_, n) => Self::CodeOpen(n),
            SimpleToken::CodeClose(_, n) => Self::CodeClose(n),
            SimpleToken::ScopeOpen(_, n) => Self::ScopeOpen(n),
            SimpleToken::RawScopeOpen(_, n) => Self::RawScopeOpen(n),
            SimpleToken::ScopeClose(_, n) => Self::ScopeClose(n),
            SimpleToken::Hashes(_, n) => Self::Hashes(n),
            SimpleToken::OtherText(span) => {
                Self::OtherText(data[span.start().pos()..span.end().pos()].into())
            }
        }
    }
}

pub fn expect_tokens<'a>(
    data: &str,
    expected_stok_types: Vec<SimpleTokenType<'a>>,
    expected_parse: Result<Vec<Token>, ParseError>,
) {
    println!("{:?}", data);

    // First step: lex
    let l = TextStream::new(data);
    let stoks: Vec<SimpleToken<_>> = l
        .iter(&[
            Box::new(SimpleToken::parse_special),
            Box::new(SimpleToken::parse_other),
        ])
        .scan((), |_, x| x.ok())
        .collect();
    let stok_types: Vec<SimpleTokenType> = stoks
        .iter()
        .map(|stok| SimpleTokenType::from_str_tok(data, *stok))
        .collect();

    assert_eq!(stok_types, expected_stok_types);

    // Second step: parse
    assert_eq!(
        parse_simple_tokens(data, Box::new(stoks.into_iter())),
        expected_parse
    );
}

use SimpleTokenType::*;
#[test]
pub fn test_basic_text() {
    expect_tokens(
        r#"Lorem Ipsum is simply dummy text of the printing and typesetting industry.
Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.
It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.
It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.
"#,
        vec![
            OtherText("Lorem Ipsum is simply dummy text of the printing and typesetting industry."),
            Newline,
            OtherText("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book."),
            Newline,
            OtherText("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged."),
            Newline,
            OtherText("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum."),
            Newline,
        ],
        Ok(vec![
            Token::Text("Lorem Ipsum is simply dummy text of the printing and typesetting industry.".into()),
            Token::Newline,
            Token::Text("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.".into()),
            Token::Newline,
            Token::Text("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.".into()),
            Token::Newline,
            Token::Text("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.".into()),
            Token::Newline,
        ])
    )
}

#[test]
pub fn test_inline_code() {
    expect_tokens(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(0),
            OtherText("len((1,2,3))"),
            CodeClose(0),
        ],
        Ok(vec![
            Token::Text("Number of values in (1,2,3): ".into()),
            Token::Code("len((1,2,3))".into()),
        ]),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [# len((1,2,3)) #]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(1),
            OtherText(" len((1,2,3)) "),
            CodeClose(1),
        ],
        Ok(vec![
            Token::Text("Number of values in (1,2,3): ".into()),
            Token::Code(" len((1,2,3)) ".into()),
        ]),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [#### len((1,2,3)) ####]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(4),
            OtherText(" len((1,2,3)) "),
            CodeClose(4),
        ],
        Ok(vec![
            Token::Text("Number of values in (1,2,3): ".into()),
            Token::Code(" len((1,2,3)) ".into()),
        ]),
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [\# len((1,2,3)) \#]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(0),
            Escaped(Escapable::Hash),
            OtherText(" len((1,2,3)) "),
            Escaped(Escapable::Hash),
            CodeClose(0),
        ],
        Ok(vec![
            Token::Text("Number of values in (1,2,3): ".into()),
            Token::Code(r#"\# len((1,2,3)) \#"#.into()),
        ]),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): \[\# len((1,2,3)) \#\]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            Escaped(Escapable::SqrOpen),
            Escaped(Escapable::Hash),
            OtherText(" len((1,2,3)) "),
            Escaped(Escapable::Hash),
            Escaped(Escapable::SqrClose),
        ],
        Ok(vec![Token::Text(
            "Number of values in (1,2,3): [# len((1,2,3)) #]".into(),
        )]),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [# len([1,2,3]) #]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(1),
            OtherText(" len("),
            CodeOpen(0),
            OtherText("1,2,3"),
            CodeClose(0),
            OtherText(") "),
            CodeClose(1),
        ],
        Ok(vec![
            Token::Text("Number of values in (1,2,3): ".into()),
            Token::Code(" len([1,2,3]) ".into()),
        ]),
    )
}

#[test]
pub fn test_inline_scope() {
    expect_tokens(
        r#"Outside the scope {inside the scope}"#,
        vec![
            OtherText("Outside the scope "),
            ScopeOpen(0),
            OtherText("inside the scope"),
            ScopeClose(0),
        ],
        Ok(vec![
            Token::Text("Outside the scope ".into()),
            Token::Scope(vec![Token::Text("inside the scope".into())]),
        ]),
    )
}

#[test]
pub fn test_inline_escaped_scope() {
    expect_tokens(
        r#"Outside the scope \{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope "),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(vec![Token::Text(
            "Outside the scope {not inside a scope}".into(),
        )]),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_tokens(
        r#"Outside the scope r{inside the raw scope}"#,
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(0),
            OtherText("inside the raw scope"),
            ScopeClose(0),
        ],
        Ok(vec![
            Token::Text("Outside the scope ".into()),
            Token::RawScope("inside the raw scope".into()),
        ]),
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_tokens(
        r#"Outside the scope r\{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope r"),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(vec![Token::Text(
            "Outside the scope r{not inside a scope}".into(),
        )]),
    )
}

#[test]
pub fn test_r_without_starting_raw_scope() {
    expect_tokens(
        r#" r doesn't always start a scope "#,
        vec![OtherText(" r doesn't always start a scope ")],
        Ok(vec![Token::Text(" r doesn't always start a scope ".into())]),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_tokens(
        r#"This has a string of ####### hashes in the middle"#,
        vec![
            OtherText("This has a string of "),
            Hashes(7),
            OtherText(" hashes in the middle"),
        ],
        Ok(vec![
            Token::Text("This has a string of ".into()),
            // The first hash in the chain starts a comment!
        ]),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash! \\[code]"#,
        vec![
            OtherText("About to see a backslash! "),
            Escaped(Escapable::Backslash),
            CodeOpen(0),
            OtherText("code"),
            CodeClose(0),
        ],
        Ok(vec![
            Token::Text(r#"About to see a backslash! \"#.into()),
            Token::Code("code".into()),
        ]),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        vec![
            OtherText("About to see a backslash and square brace! "),
            Escaped(Escapable::Backslash),
            Escaped(Escapable::SqrOpen),
            OtherText(" that didn't open code!"),
        ],
        Ok(vec![Token::Text(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#.into(),
        )]),
    )
}

#[test]
pub fn test_uneven_code() {
    expect_tokens(
        r#"code with no open]"#,
        vec![OtherText("code with no open"), CodeClose(0)],
        Err(ParseError::CodeCloseInText),
    )
}

#[test]
pub fn test_uneven_scope() {
    expect_tokens(
        r#"scope with no open}"#,
        vec![OtherText("scope with no open"), ScopeClose(0)],
        Err(ParseError::ScopeCloseOutsideScope),
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_tokens(
        r#"\a"#,
        vec![Backslash, OtherText("a")],
        Ok(vec![Token::Text(r#"\a"#.into())]),
    )
}

#[test]
pub fn test_escaped_cr() {
    // '\' + '\r'
    let s: String = ['\\', '\r'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Escaped(Escapable::Newline), OtherText("content")],
        Ok(vec![Token::Text(r#"content"#.into())]),
    )
}
#[test]
pub fn test_escaped_lf() {
    // '\' + '\n'
    let s: String = ['\\', '\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Escaped(Escapable::Newline), OtherText("content")],
        Ok(vec![Token::Text(r#"content"#.into())]),
    )
}
#[test]
pub fn test_escaped_crlf() {
    // '\' + '\r' + '\n'
    let s: String = ['\\', '\r', '\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Escaped(Escapable::Newline), OtherText("content")],
        Ok(vec![Token::Text(r#"content"#.into())]),
    )
}

#[test]
pub fn test_cr() {
    // '\r'
    let s: String = ['\r'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(vec![Token::Newline, Token::Text("content".into())]),
    )
}
#[test]
pub fn test_lf() {
    // '\n'
    let s: String = ['\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(vec![Token::Newline, Token::Text("content".into())]),
    )
}
#[test]
pub fn test_crlf() {
    // '\r' + '\n'
    let s: String = ['\r', '\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(vec![Token::Newline, Token::Text("content".into())]),
    )
}

#[test]
pub fn test_newline_in_code() {
    expect_tokens(
        r#"[code.do_something();
code.do_something_else()]"#,
        vec![
            CodeOpen(0),
            OtherText("code.do_something();"),
            Newline,
            OtherText("code.do_something_else()"),
            CodeClose(0),
        ],
        Err(ParseError::NewlineInCode),
    )
}
#[test]
pub fn test_code_close_in_text() {
    expect_tokens(
        "not code ] but closed code",
        vec![
            OtherText("not code "),
            CodeClose(0),
            OtherText(" but closed code"),
        ],
        Err(ParseError::CodeCloseInText),
    )
}
#[test]
pub fn test_scope_close_outside_scope() {
    expect_tokens(
        "not in a scope } but closed scope",
        vec![
            OtherText("not in a scope "),
            ScopeClose(0),
            OtherText(" but closed scope"),
        ],
        Err(ParseError::ScopeCloseOutsideScope),
    )
}
#[test]
pub fn test_mismatching_scope_close() {
    expect_tokens(
        "{## text in a scope with a #}",
        vec![
            ScopeOpen(2),
            OtherText(" text in a scope with a "),
            ScopeClose(1),
        ],
        Err(ParseError::MismatchingScopeClose(1)),
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_tokens(
        "text [code",
        vec![OtherText("text "), CodeOpen(0), OtherText("code")],
        Err(ParseError::EndedInsideCode),
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_tokens(
        "text r{#raw",
        vec![OtherText("text "), RawScopeOpen(1), OtherText("raw")],
        Err(ParseError::EndedInsideRawScope),
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_tokens(
        "text {##scope",
        vec![OtherText("text "), ScopeOpen(2), OtherText("scope")],
        Err(ParseError::EndedInsideScope),
    )
}
