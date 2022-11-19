use lexer_rs::{Lexer, LexerOfStr, SimpleParseError, StreamCharPos};
use turnip_text::tokens::{SimpleToken, SpecialChar};

type TextPos = StreamCharPos<usize>;
type LexToken = SimpleToken<TextPos>;
type LexError = SimpleParseError<TextPos>;
type TextStream<'stream> = LexerOfStr<'stream, TextPos, LexToken, LexError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleTokenType {
    Newline,
    Escaped(SpecialChar),
    Backslash,
    CodeOpen { n: usize },
    CodeClose { n: usize },
    ScopeOpen { n: usize },
    ScopeClose { n: usize },
    Hashes(usize),
    Other(String),
}
impl SimpleTokenType {
    fn from_str_tok(data: &str, t: SimpleToken<StreamCharPos<usize>>) -> Self {
        match t {
            SimpleToken::Newline(_) => Self::Newline,
            SimpleToken::Escaped(_, char) => Self::Escaped(char),
            SimpleToken::Backslash(_) => Self::Backslash,
            SimpleToken::CodeOpen { n, .. } => Self::CodeOpen { n },
            SimpleToken::CodeClose { n, .. } => Self::CodeClose { n },
            SimpleToken::ScopeOpen { n, .. } => Self::ScopeOpen { n },
            SimpleToken::ScopeClose { n, .. } => Self::ScopeClose { n },
            SimpleToken::Hashes(_, n) => Self::Hashes(n),
            SimpleToken::Other(span) => {
                Self::Other(data[span.start().pos()..span.end().pos()].into())
            }
        }
    }
}

pub fn lex_test_string(data: &str) -> Vec<SimpleTokenType> {
    let l = TextStream::new(data);
    let tokens = l
        .iter(&[
            Box::new(SimpleToken::parse_special),
            Box::new(SimpleToken::parse_other),
        ])
        .scan((), |_, x| x.ok())
        .map(|tok| SimpleTokenType::from_str_tok(data, tok))
        .collect();
    tokens
}

pub fn expect_tokens(data: &str, tokens: Vec<SimpleTokenType>) {
    assert_eq!(lex_test_string(data), tokens);
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
            Other("Lorem Ipsum is simply dummy text of the printing and typesetting industry.".into()),
            Newline,
            Other("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.".into()),
            Newline,
            Other("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.".into()),
            Newline,
            Other("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.".into()),
            Newline,
        ],
    )
}

#[test]
pub fn test_inline_code() {
    expect_tokens(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            CodeOpen { n: 0 },
            Other("len((1,2,3))".into()),
            CodeClose { n: 0 },
        ],
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [# len((1,2,3)) #]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            CodeOpen { n: 1 },
            Other(" len((1,2,3)) ".into()),
            CodeClose { n: 1 },
        ],
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [#### len((1,2,3)) ####]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            CodeOpen { n: 4 },
            Other(" len((1,2,3)) ".into()),
            CodeClose { n: 4 },
        ],
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [\# len((1,2,3)) \#]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            CodeOpen { n: 0 },
            Escaped(SpecialChar::Hash),
            Other(" len((1,2,3)) ".into()),
            Escaped(SpecialChar::Hash),
            CodeClose { n: 0 },
        ],
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): \[\# len((1,2,3)) \#\]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            Escaped(SpecialChar::SqrOpen),
            Escaped(SpecialChar::Hash),
            Other(" len((1,2,3)) ".into()),
            Escaped(SpecialChar::Hash),
            Escaped(SpecialChar::SqrClose),
        ],
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [# len([1,2,3]) #]"#,
        vec![
            Other("Number of values in (1,2,3): ".into()),
            CodeOpen { n: 1 },
            Other(" len(".into()),
            CodeOpen { n: 0 },
            Other("1,2,3".into()),
            CodeClose { n: 0 },
            Other(") ".into()),
            CodeClose { n: 1 },
        ],
    )
}

#[test]
pub fn test_inline_scope() {
    expect_tokens(
        r#"Outside the scope {inside the scope}"#,
        vec![
            Other("Outside the scope ".into()),
            ScopeOpen { n: 0 },
            Other("inside the scope".into()),
            ScopeClose { n: 0 },
        ],
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_tokens(
        r#"This has a string of ####### hashes in the middle"#,
        vec![
            Other("This has a string of ".into()),
            Hashes(7),
            Other(" hashes in the middle".into()),
        ],
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash! \\[code]"#,
        vec![
            Other("About to see a backslash! ".into()),
            Escaped(SpecialChar::Backslash),
            CodeOpen { n: 0 },
            Other("code".into()),
            CodeClose { n: 0 },
        ],
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        vec![
            Other("About to see a backslash and square brace! ".into()),
            Escaped(SpecialChar::Backslash),
            Escaped(SpecialChar::SqrOpen),
            Other(" that didn't open code!".into()),
        ],
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_tokens(r#"\a"#, vec![Backslash, Other("a".into())])
}
