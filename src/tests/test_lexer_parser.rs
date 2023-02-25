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
#[test]
pub fn test_basic_text() {
    expect_lex_parse(
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
        Ok(
            test_doc(vec![
                TestBlock::Paragraph(vec![
                    test_sentence("Lorem Ipsum is simply dummy text of the printing and typesetting industry."),
                    test_sentence("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book."),
                    test_sentence("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged."),
                    test_sentence("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum."),
                ])
            ])
        )
    )
}

#[test]
pub fn test_inline_code() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(1),
            OtherText("len((1,2,3))"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(2),
            OtherText(" len((1,2,3)) "),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): [[[[[ len((1,2,3)) ]]]]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(5),
            OtherText(" len((1,2,3)) "),
            CodeClose(5),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): \[[ len((1,2,3)) ]\]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            Escaped(Escapable::SqrOpen),
            CodeOpen(1),
            OtherText(" len((1,2,3)) "),
            CodeClose(1),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): ["),
            test_text("3"),
            test_text("]"),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): \[\[ len((1,2,3)) \]\]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            Escaped(Escapable::SqrOpen),
            Escaped(Escapable::SqrOpen),
            OtherText(" len((1,2,3)) "),
            Escaped(Escapable::SqrClose),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        )])])),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_lex_parse(
        r#"Number of values in (1,2,3): [[ len([1,2,3]) ]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(2),
            OtherText(" len("),
            CodeOpen(1),
            OtherText("1,2,3"),
            CodeClose(1),
            OtherText(") "),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_block_scope() {
    expect_lex_parse(
        r#"Outside the scope

{
Inside the scope

Second paragraph inside the scope
}"#,
        vec![
            OtherText("Outside the scope"),
            Newline,
            Newline,
            BlockScopeOpen,
            OtherText("Inside the scope"),
            Newline,
            Newline,
            OtherText("Second paragraph inside the scope"),
            Newline,
            ScopeClose,
        ],
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("Outside the scope")]),
            TestBlock::BlockScope {
                owner: None,
                contents: vec![
                    TestBlock::Paragraph(vec![test_sentence("Inside the scope")]),
                    TestBlock::Paragraph(vec![test_sentence("Second paragraph inside the scope")]),
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
            OtherText("It's f&%"),
            Hashes(1),
            OtherText("ing raw"),
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
        r#"Outside the scope {inside the scope}"#,
        vec![
            OtherText("Outside the scope "),
            InlineScopeOpen,
            OtherText("inside the scope"),
            ScopeClose,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inside the scope")],
            },
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_scope() {
    expect_lex_parse(
        r#"Outside the scope \{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope "),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope {not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_raw_scope_newlines() {
    expect_lex_parse(
        "Outside the scope #{\ninside the raw scope\n}#",
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside the raw scope"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
pub fn test_raw_scope_crlf_newlines() {
    expect_lex_parse(
        "Outside the scope #{\r\ninside the raw scope\r\n}#",
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside the raw scope"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_lex_parse(
        r#"Outside the scope #{inside the raw scope}#"#,
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            OtherText("inside the raw scope"),
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "inside the raw scope"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_lex_parse(
        r#"Outside the scope r\{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope r"),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope r{not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_r_without_starting_raw_scope() {
    expect_lex_parse(
        r#" r doesn't always start a scope "#,
        vec![OtherText(" r doesn't always start a scope ")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            " r doesn't always start a scope ",
        )])])),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_lex_parse(
        r#"This has a string of ####### hashes in the middle"#,
        vec![
            OtherText("This has a string of "),
            Hashes(7),
            OtherText(" hashes in the middle"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("This has a string of "), // The first hash in the chain starts a comment!
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"About to see a backslash! \\[None]"#,
        vec![
            OtherText("About to see a backslash! "),
            Escaped(Escapable::Backslash),
            CodeOpen(1),
            OtherText("None"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text(r#"About to see a backslash! \"#),
            test_text("None"),
        ]])])),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        vec![
            OtherText("About to see a backslash and square brace! "),
            Escaped(Escapable::Backslash),
            Escaped(Escapable::SqrOpen),
            OtherText(" that didn't open code!"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#,
        )])])),
    )
}

#[test]
pub fn test_uneven_code() {
    expect_lex_parse(
        r#"code with no open]"#,
        vec![OtherText("code with no open"), CodeClose(1)],
        Err(TestInterpError::CodeCloseOutsideCode(TestParserSpan {
            start: (1, 18),
            end: (1, 19),
        })),
    )
}

#[test]
pub fn test_uneven_scope() {
    expect_lex_parse(
        r#"scope with no open}"#,
        vec![OtherText("scope with no open"), ScopeClose],
        Err(TestInterpError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 19),
            end: (1, 20),
        })),
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
            OtherText("escaped "),
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
pub fn test_code_close_in_text() {
    expect_lex_parse(
        "not code ] but closed code",
        vec![
            OtherText("not code "),
            CodeClose(1),
            OtherText(" but closed code"),
        ],
        Err(TestInterpError::CodeCloseOutsideCode(TestParserSpan {
            start: (1, 10),
            end: (1, 11),
        })),
    )
}
#[test]
pub fn test_scope_close_outside_scope() {
    expect_lex_parse(
        "not in a scope } but closed scope",
        vec![
            OtherText("not in a scope "),
            ScopeClose,
            OtherText(" but closed scope"),
        ],
        Err(TestInterpError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 16),
            end: (1, 17),
        })),
    )
}
#[test]
pub fn test_mismatching_raw_scope_close() {
    expect_lex_parse(
        "##{ text in a scope with a }#",
        vec![
            RawScopeOpen(2),
            OtherText(" text in a scope with a "),
            RawScopeClose(1),
        ],
        Err(TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan {
                start: (1, 1),
                end: (1, 4),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_lex_parse(
        "text [code",
        vec![OtherText("text "), CodeOpen(1), OtherText("code")],
        Err(TestInterpError::EndedInsideCode {
            code_start: TestParserSpan {
                start: (1, 6),
                end: (1, 7),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_lex_parse(
        "text #{raw",
        vec![OtherText("text "), RawScopeOpen(1), OtherText("raw")],
        Err(TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 8),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_lex_parse(
        "text {scope",
        vec![OtherText("text "), InlineScopeOpen, OtherText("scope")],
        Err(TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 7),
            },
        }),
    )
}

#[test]
pub fn test_block_scope_vs_inline_scope() {
    expect_lex_parse(
        r#"{
block scope
}{inline scope}"#,
        vec![
            BlockScopeOpen,
            OtherText("block scope"),
            Newline,
            ScopeClose,
            InlineScopeOpen,
            OtherText("inline scope"),
            ScopeClose,
        ],
        Ok(test_doc(vec![
            TestBlock::BlockScope {
                owner: None,
                contents: vec![TestBlock::Paragraph(vec![test_sentence("block scope")])],
            },
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inline scope")],
            }]]),
        ])),
    )
}

#[test]
pub fn test_owned_block_scope() {
    expect_lex_parse(
        r#"[TEST_BLOCK_OWNER]{
It was the best of the times, it was the blurst of times
}
"#,
        vec![
            CodeOpen(1),
            OtherText("TEST_BLOCK_OWNER"),
            CodeCloseOwningBlock(1),
            OtherText("It was the best of the times, it was the blurst of times"),
            Newline,
            ScopeClose,
            Newline,
        ],
        Ok(test_doc(vec![TestBlock::BlockScope {
            owner: Some("TEST_BLOCK_OWNER".into()),
            contents: vec![TestBlock::Paragraph(vec![test_sentence(
                "It was the best of the times, it was the blurst of times",
            )])],
        }])),
    )
}

#[test]
pub fn test_owned_block_scope_with_non_block_owner() {
    expect_lex_parse(
        r#"[None]{
It was the best of the times, it was the blurst of times
}
"#,
        vec![
            CodeOpen(1),
            OtherText("None"),
            CodeCloseOwningBlock(1),
            OtherText("It was the best of the times, it was the blurst of times"),
            Newline,
            ScopeClose,
            Newline,
        ],
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass BlockScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (2, 1),
            },
        }),
    )
}

#[test]
pub fn test_owned_inline_scope() {
    expect_lex_parse(
        r"[TEST_INLINE_OWNER]{special text}",
        vec![
            CodeOpen(1),
            OtherText("TEST_INLINE_OWNER"),
            CodeCloseOwningInline(1),
            OtherText("special text"),
            ScopeClose,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope {
                owner: Some("TEST_INLINE_OWNER".into()),
                contents: vec![test_text("special text")],
            },
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_scope_with_non_inline_owner() {
    expect_lex_parse(
        r"[None]{special text}",
        vec![
            CodeOpen(1),
            OtherText("None"),
            CodeCloseOwningInline(1),
            OtherText("special text"),
            ScopeClose,
        ],
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass InlineScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 8),
            },
        }),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_newline() {
    expect_lex_parse(
        r#"[TEST_RAW_OWNER]#{
import os
}#"#,
        vec![
            CodeOpen(1),
            OtherText("TEST_RAW_OWNER"),
            CodeCloseOwningRaw(1, 1),
            Newline,
            OtherText("import os"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::RawText {
                owner: Some("TEST_RAW_OWNER".into()),
                contents: r#"
import os
"#
                .into(),
            },
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_non_raw_owner() {
    expect_lex_parse(
        r#"[None]#{
import os
}#"#,
        vec![
            CodeOpen(1),
            OtherText("None"),
            CodeCloseOwningRaw(1, 1),
            Newline,
            OtherText("import os"),
            Newline,
            RawScopeClose(1),
        ],
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass RawScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 9),
            },
        }),
    )
}
