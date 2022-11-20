use crate::parser::parse_simple_tokens;

use super::{
    lexer::{Escapable, LexError, LexPosn, LexToken, SimpleToken},
    parser::ParserSpan,
    ParseError, Token,
};
use lexer_rs::{Lexer, LexerOfStr, PosnInCharStream};

type TextStream<'stream> = LexerOfStr<'stream, LexPosn, LexToken, LexError>;

/// A type mimicking [SimpleToken] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestSimpleToken<'a> {
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
impl<'a> TestSimpleToken<'a> {
    fn from_str_tok(data: &'a str, t: LexToken) -> Self {
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
                Self::OtherText(data[span.start().byte_ofs()..span.end().byte_ofs()].into())
            }
        }
    }
}

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestParserSpan {
    start: (usize, usize),
    end: (usize, usize),
}
impl From<ParserSpan> for TestParserSpan {
    fn from(p: ParserSpan) -> Self {
        Self {
            start: (p.start.line, p.start.column),
            end: (p.end.line, p.end.column),
        }
    }
}

/// A type mimicking [ParseError] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestParseError {
    NewlineInCode {
        code_start: TestParserSpan,
        newline: TestParserSpan,
    },
    CodeCloseInText(TestParserSpan),
    ScopeCloseOutsideScope(TestParserSpan),
    MismatchingScopeClose {
        n_hashes: usize,
        expected_closing_hashes: usize,
        scope_open_span: TestParserSpan,
        scope_close_span: TestParserSpan,
    },
    EndedInsideCode {
        code_start: TestParserSpan,
    },
    EndedInsideRawScope {
        raw_scope_start: TestParserSpan,
    },
    EndedInsideScope {
        scope_start: TestParserSpan,
    },
}
impl TestParseError {
    /// Convert [ParseError] to [TestParseError]
    ///
    /// This is a lossy transformation, ignoring byte offsets in spans, but is good enough for testing
    fn from_parse_error(p: ParseError) -> Self {
        match p {
            ParseError::NewlineInCode {
                code_start,
                newline,
            } => Self::NewlineInCode {
                code_start: code_start.into(),
                newline: newline.into(),
            },
            ParseError::CodeCloseInText(span) => Self::CodeCloseInText(span.into()),
            ParseError::ScopeCloseOutsideScope(span) => Self::ScopeCloseOutsideScope(span.into()),
            ParseError::MismatchingScopeClose {
                n_hashes,
                expected_closing_hashes,
                scope_open_span,
                scope_close_span,
            } => Self::MismatchingScopeClose {
                n_hashes,
                expected_closing_hashes,
                scope_open_span: scope_open_span.into(),
                scope_close_span: scope_close_span.into(),
            },
            ParseError::EndedInsideCode { code_start } => Self::EndedInsideCode {
                code_start: code_start.into(),
            },
            ParseError::EndedInsideRawScope { raw_scope_start } => Self::EndedInsideRawScope {
                raw_scope_start: raw_scope_start.into(),
            },
            ParseError::EndedInsideScope { scope_start } => Self::EndedInsideScope {
                scope_start: scope_start.into(),
            },
        }
    }
}

fn expect_tokens<'a>(
    data: &str,
    expected_stok_types: Vec<TestSimpleToken<'a>>,
    expected_parse: Result<Vec<Token>, TestParseError>,
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
    let stok_types: Vec<TestSimpleToken> = stoks
        .iter()
        .map(|stok| TestSimpleToken::from_str_tok(data, *stok))
        .collect();

    assert_eq!(stok_types, expected_stok_types);

    // Second step: parse
    assert_eq!(
        parse_simple_tokens(data, Box::new(stoks.into_iter()))
            .map_err(TestParseError::from_parse_error),
        expected_parse
    );
}

use TestSimpleToken::*;
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
        Err(TestParseError::CodeCloseInText(TestParserSpan {
            start: (1, 18),
            end: (1, 19),
        })),
    )
}

#[test]
pub fn test_uneven_scope() {
    expect_tokens(
        r#"scope with no open}"#,
        vec![OtherText("scope with no open"), ScopeClose(0)],
        Err(TestParseError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 19),
            end: (1, 20),
        })),
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
        "[code.do_something();\ncode.do_something_else()]",
        vec![
            CodeOpen(0),
            OtherText("code.do_something();"),
            Newline,
            OtherText("code.do_something_else()"),
            CodeClose(0),
        ],
        Err(TestParseError::NewlineInCode {
            code_start: TestParserSpan {
                start: (1, 1),
                end: (1, 2),
            },
            newline: TestParserSpan {
                start: (1, 22),
                end: (2, 1),
            },
        }),
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
        Err(TestParseError::CodeCloseInText(TestParserSpan {
            start: (1, 10),
            end: (1, 11),
        })),
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
        Err(TestParseError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 16),
            end: (1, 17),
        })),
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
        Err(TestParseError::MismatchingScopeClose {
            n_hashes: 1,
            expected_closing_hashes: 2,
            scope_open_span: TestParserSpan {
                start: (1, 1),
                end: (1, 4),
            },
            scope_close_span: TestParserSpan {
                start: (1, 28),
                end: (1, 30),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_tokens(
        "text [code",
        vec![OtherText("text "), CodeOpen(0), OtherText("code")],
        Err(TestParseError::EndedInsideCode {
            code_start: TestParserSpan {
                start: (1, 6),
                end: (1, 7),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_tokens(
        "text r{#raw",
        vec![OtherText("text "), RawScopeOpen(1), OtherText("raw")],
        Err(TestParseError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 9),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_tokens(
        "text {##scope",
        vec![OtherText("text "), ScopeOpen(2), OtherText("scope")],
        Err(TestParseError::EndedInsideScope {
            scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 9),
            },
        }),
    )
}
