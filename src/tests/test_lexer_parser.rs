use std::panic;

use crate::lexer::{units_to_tokens, Unit};

use crate::lexer::Escapable;
use crate::python::interp_data;
use lexer_rs::Lexer;

use pyo3::prelude::*;

use super::test_lexer::*;
use super::test_parser::*;

/// Run the lexer AND parser on given data, checking the results of both against expected versions as specified in [super::test_lexer::expect_lex] and [super::test_parser::expect_parse]
fn expect_lex_parse<'a>(
    data: &str,
    expected_stok_types: Vec<TestTTToken<'a>>,
    expected_parse: Result<TestBlock, TestInterpError>,
) {
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

    // Second step: parse
    // Need to do this safely so that we don't panic while the TTPYTHON mutex is taken -
    // that would poison the mutex and break subsequent tests.
    let root: Result<Result<TestBlock, TestInterpError>, _> = {
        // Lock mutex
        let ttpython = TTPYTHON.lock().unwrap();
        // Catch all non-abort panics while running the interpreter
        // and handling the output
        panic::catch_unwind(|| {
            ttpython
                .with_gil(|py| {
                    let globals = generate_globals(py).expect("Couldn't generate globals dict");
                    let root = interp_data(py, globals, data, stoks.into_iter());
                    root.map(|bs| {
                        let bs_obj = bs.to_object(py);
                        let bs: &PyAny = bs_obj.as_ref(py);
                        (bs as &dyn PyToTest<TestBlock>).as_test(py)
                    })
                })
                .map_err(TestInterpError::from_interp_error)
        })
        // Unlock mutex
    };
    // If any of the python-related code tried to panic, re-panic here now the mutex is unlocked
    match root {
        Ok(root) => assert_eq!(root, expected_parse),
        Err(e) => panic!("{:?}", e),
    }
}

use TestTTToken::*;

// These tests are condensed versions of the tests in [test_parser] that also sanity-check the tokens generated.

#[test]
pub fn test_inline_code() {
    expect_lex_parse(
        r#"3=[len((1,2,3))]"#,
        vec![
            OtherText("3="),
            CodeOpen(1),
            OtherText("len((1,2,3))"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_lex_parse(
        r#"3=[[len((1,2,3))]]"#,
        vec![
            OtherText("3="),
            CodeOpen(2),
            OtherText("len((1,2,3))"),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_lex_parse(
        r#"3=[[[[[len((1,2,3))]]]]]"#,
        vec![
            OtherText("3="),
            CodeOpen(5),
            OtherText("len((1,2,3))"),
            CodeClose(5),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"3=\[[len((1,2,3))]\]"#,
        vec![
            OtherText("3="),
            Escaped(Escapable::SqrOpen),
            CodeOpen(1),
            OtherText("len((1,2,3))"),
            CodeClose(1),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3=["),
            test_text("3"),
            test_text("]"),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"3=\[\[ len((1,2,3)) \]\]"#,
        vec![
            OtherText("3="),
            Escaped(Escapable::SqrOpen),
            Escaped(Escapable::SqrOpen),
            Whitespace(" "),
            OtherText("len((1,2,3))"),
            Whitespace(" "),
            Escaped(Escapable::SqrClose),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"3=[[ len((1,2,3)) ]]"#,
        )])])),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_lex_parse(
        r#"3=[[len([1,2,3])]]"#,
        vec![
            OtherText("3="),
            CodeOpen(2),
            OtherText("len("),
            CodeOpen(1),
            OtherText("1,2,3"),
            CodeClose(1),
            OtherText(")"),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_block_scope() {
    expect_lex_parse(
        r#"outside

{
inside

inside 2
}"#,
        vec![
            OtherText("outside"),
            Newline,
            Newline,
            BlockScopeOpen,
            OtherText("inside"),
            Newline,
            Newline,
            OtherText("inside"),
            Whitespace(" "),
            OtherText("2"),
            Newline,
            ScopeClose,
        ],
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("outside")]),
            TestBlock::BlockScope {
                owner: None,
                contents: vec![
                    TestBlock::Paragraph(vec![test_sentence("inside")]),
                    TestBlock::Paragraph(vec![test_sentence("inside 2")]),
                ],
            },
        ])),
    )
}

#[test]
pub fn test_raw_scope() {
    expect_lex_parse(
        "#{It's f&%#ing raw}#",
        vec![
            RawScopeOpen(1),
            OtherText("It's"),
            Whitespace(" "),
            OtherText("f&%"),
            Hashes(1),
            OtherText("ing"),
            Whitespace(" "),
            OtherText("raw"),
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::RawText {
                owner: None,
                contents: "It's f&%#ing raw".into(),
            },
        ]])])),
    )
}

#[test]
pub fn test_inline_scope() {
    expect_lex_parse(
        r#"outside {inside}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            InlineScopeOpen,
            OtherText("inside"),
            ScopeClose,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inside")],
            },
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_scope() {
    expect_lex_parse(
        r#"outside \{not inside\}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            Escaped(Escapable::SqgOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("inside"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "outside {not inside}",
        )])])),
    )
}

#[test]
pub fn test_raw_scope_newlines() {
    expect_lex_parse(
        "outside #{\ninside\n}#",
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text(None, "\ninside\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
pub fn test_raw_scope_crlf_newlines() {
    expect_lex_parse(
        "outside #{\r\ninside\r\n}#",
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text(None, "\ninside\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_lex_parse(
        r#"outside #{inside}#"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            OtherText("inside"),
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text(None, "inside"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_lex_parse(
        r#"outside \#\{not inside\}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            Escaped(Escapable::Hash),
            Escaped(Escapable::SqgOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("inside"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "outside #{not inside}",
        )])])),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_lex_parse(
        r#"before ####### after"#,
        vec![
            OtherText("before"),
            Whitespace(" "),
            Hashes(7),
            Whitespace(" "),
            OtherText("after"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("before"), // The first hash in the chain starts a comment, and trailing whitespace is ignored
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"\\[None]"#,
        vec![
            Escaped(Escapable::Backslash),
            CodeOpen(1),
            OtherText("None"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("\\"),
            test_text("None"),
        ]])])),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"\\\[not code"#,
        vec![
            Escaped(Escapable::Backslash),
            Escaped(Escapable::SqrOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("code"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\[not code"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_lex_parse(
        r#"\a"#,
        vec![Backslash, OtherText("a")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_newline() {
    expect_lex_parse(
        r#"escaped \
newline"#,
        vec![
            OtherText("escaped"),
            Whitespace(" "),
            Escaped(Escapable::Newline),
            OtherText("newline"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "escaped newline",
        )])])),
    )
}

#[test]
pub fn test_newline_in_code() {
    expect_lex_parse(
        "[len((1,\r\n2))]",
        vec![
            CodeOpen(1),
            OtherText("len((1,"),
            Newline,
            OtherText("2))"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}

#[test]
pub fn test_block_scope_vs_inline_scope() {
    expect_lex_parse(
        r#"{
block
}{inline}"#,
        vec![
            BlockScopeOpen,
            OtherText("block"),
            Newline,
            ScopeClose,
            InlineScopeOpen,
            OtherText("inline"),
            ScopeClose,
        ],
        Ok(test_doc(vec![
            TestBlock::BlockScope {
                owner: None,
                contents: vec![TestBlock::Paragraph(vec![test_sentence("block")])],
            },
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inline")],
            }]]),
        ])),
    )
}
