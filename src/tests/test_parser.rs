use crate::error::{TurnipTextResult, UserPythonCompileMode};
use crate::interpreter::TurnipTextParser;
use regex::Regex;

use crate::python::prepare_freethreaded_turniptext_python;

use pyo3::prelude::*;

use std::ffi::CString;
use std::panic;
// We need to initialize Python the first time we test
use std::sync::Once;

use super::helpers::*;
static INIT_PYTHON: Once = Once::new();

/// Run the lexer and parser on a given piece of text, convert the parsed result to our test versions, and compare with the expected result.

fn expect_parse_err<'a, T: Into<TestTurnipError<'a>>>(data: &'a str, expected_err: T) {
    expect_parse(data, Err(expected_err.into()))
}

pub fn expect_parse(data: &str, expected_parse: Result<TestDocSegment, TestTurnipError>) {
    // Make sure Python has been set up
    INIT_PYTHON.call_once(prepare_freethreaded_turniptext_python);

    // Second step: parse
    // Need to do this safely so that we don't panic inside Python::with_gil.
    // I'm not 100% sure but I'm afraid it will poison the GIL and break subsequent tests.
    let root: Result<TurnipTextResult<TestDocSegment>, _> = {
        // Catch all non-abort panics while running the interpreter
        // and handling the output
        panic::catch_unwind(|| {
            Python::with_gil(|py| {
                let py_env = generate_globals(py).expect("Couldn't generate globals dict");
                let parser = TurnipTextParser::new(py, "<test>".into(), data.into())?;
                let root = parser.parse(py, py_env)?;
                let doc_obj = root.to_object(py);
                let doc: &PyAny = doc_obj.as_ref(py);
                Ok(doc.as_test(py))
            })
        })
        // Unlock mutex
    };
    // If any of the python-related code tried to panic, re-panic here now the mutex is unlocked
    match root {
        Ok(root) => match (&root, &expected_parse) {
            (Ok(doc), Ok(expected_doc)) => assert_eq!(doc, expected_doc),
            (Err(actual_err), Err(expected_err)) => {
                let matches = Python::with_gil(|py| expected_err.matches(py, &actual_err));
                if !matches {
                    panic!(
                        "assertion failed:\nexpected: {expected_err:?}\n  actual: {actual_err:?}"
                    );
                }
            }
            _ => panic!(
                "assertion failed, expected\n\t{expected_parse:?}\ngot\n\t{root:?}\n(mismatching \
                 success)"
            ),
        },
        Err(caught_panic) => panic!("{:?}", caught_panic),
    }
}

#[test]
pub fn test_basic_text() {
    expect_parse(
        r#"Lorem Ipsum is simply dummy text of the printing and typesetting industry.
Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.
It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.
It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.
"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence(
                "Lorem Ipsum is simply dummy text of the printing and typesetting industry.",
            ),
            test_sentence(
                "Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, \
                 when an unknown printer took a galley of type and scrambled it to make a type \
                 specimen book.",
            ),
            test_sentence(
                "It has survived not only five centuries, but also the leap into electronic \
                 typesetting, remaining essentially unchanged.",
            ),
            test_sentence(
                "It was popularised in the 1960s with the release of Letraset sheets containing \
                 Lorem Ipsum passages, and more recently with desktop publishing software like \
                 Aldus PageMaker including versions of Lorem Ipsum.",
            ),
        ])])),
    )
}

#[test]
pub fn test_inline_code() {
    expect_parse(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [- len((1,2,3)) -]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [---- len((1,2,3)) ----]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): \[\- len((1,2,3)) \-\]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"Number of values in (1,2,3): [- len((1,2,3)) -]"#,
        )])])),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [- len([1,2,3]) -]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_block_scope() {
    expect_parse(
        r#"Outside the scope

{
Inside the scope

Second paragraph inside the scope
}"#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("Outside the scope")]),
            TestBlock::BlockScope(vec![
                TestBlock::Paragraph(vec![test_sentence("Inside the scope")]),
                TestBlock::Paragraph(vec![test_sentence("Second paragraph inside the scope")]),
            ]),
        ])),
    )
}

#[test]
pub fn test_raw_scope() {
    expect_parse(
        "#{It's f&%#ing raw}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_raw_text("It's f&%#ing raw"),
        ]])])),
    )
}

#[test]
pub fn test_inline_scope() {
    expect_parse(
        r#"Outside the scope {inside the scope}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            TestInline::InlineScope(vec![test_text("inside the scope")]),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_scope() {
    expect_parse(
        r#"Outside the scope \{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope {not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_raw_scope_newlines() {
    expect_parse(
        "Outside the scope #{\ninside the raw scope\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("\ninside the raw scope\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
pub fn test_raw_scope_crlf_newlines() {
    expect_parse(
        "Outside the scope #{\r\ninside the raw scope\r\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_parse(
        r#"Outside the scope #{inside the raw scope}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("inside the raw scope"),
        ]])])),
    )
}

#[test]
pub fn test_owned_block_scope() {
    expect_parse(
        r#"[TEST_BLOCK_BUILDER]{
It was the best of the times, it was the blurst of times
}
"#,
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![
            TestBlock::Paragraph(vec![test_sentence(
                "It was the best of the times, it was the blurst of times",
            )]),
        ])])),
    )
}

#[test]
pub fn test_owned_block_scope_with_non_block_builder() {
    expect_parse_err(
        r#"[None]{
It was the best of the times, it was the blurst of times
}
"#,
        TestUserPythonExecError::CoercingBlockScopeBuilder {
            code_ctx: TestParseContext("[", "None", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*BlockScopeBuilder.*Got None.*").unwrap(),
        },
    )
}

#[test]
pub fn test_owned_inline_scope() {
    expect_parse(
        r"[TEST_INLINE_BUILDER]{special text}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedInline(vec![test_text("special text")]),
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_scope_with_non_inline_builder() {
    expect_parse_err(
        r"[None]{special text}",
        TestUserPythonExecError::CoercingInlineScopeBuilder {
            code_ctx: TestParseContext("[", "None", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*InlineScopeBuilder.*Got None.*").unwrap(),
        },
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_newline() {
    expect_parse(
        r#"[TEST_RAW_INLINE_BUILDER]#{
import os
}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw(
                r#"
import os
"#
                .into(),
            ),
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_non_raw_builder() {
    expect_parse_err(
        r#"[None]#{
import os
}#"#,
        TestUserPythonExecError::CoercingRawScopeBuilder {
            code_ctx: TestParseContext("[", "None", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*RawScopeBuilder.*Got None").unwrap(),
        },
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_parse(
        r#"Outside the scope \#\{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope #{not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_parse(
        r#"This has a string of ####### hashes in the middle"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("This has a string of"), // The first hash in the chain starts a comment, and trailing whitespace is ignored
        ])])),
    )
}

#[test]
pub fn test_comments() {
    expect_parse(
        r#"It was the best of times. # but...
It was the blurst of times."#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("It was the best of times."),
            test_sentence("It was the blurst of times."),
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash! \\#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            r#"About to see a backslash! \"#,
        )]])])),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_parse(
        r#"\a"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_newline() {
    expect_parse(
        r#"escaped \
newline"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "escaped newline",
        )])])),
    )
}

#[test]
pub fn test_newline_in_code() {
    expect_parse(
        "[len((1,\r\n2))]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}
#[test]
pub fn test_code_close_in_text() {
    expect_parse_err(
        "not code ] but closed code",
        TestInterpError::CodeCloseOutsideCode(TestParseSpan("]")),
    )
}
#[test]
pub fn test_inline_scope_close_outside_scope() {
    expect_parse_err(
        "not in a scope } but closed scope",
        TestInterpError::InlineScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
#[test]
pub fn test_block_scope_close_outside_scope() {
    expect_parse_err(
        "} # not in a scope",
        TestInterpError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
// Scope closes at the start of a line directly after a paragraph are treated differently
// We assume you couldn't possibly be closing an inline scope! There can't be any to close!
// So you must be trying to close a block-level scope...
#[test]
pub fn test_block_scope_close_outside_scope_after_para() {
    expect_parse_err(
        "wow some content\nthat could imply the next scope close is in a paragraph i.e. inline \
         mode\n} # not in a scope",
        TestInterpError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
#[test]
pub fn test_raw_scope_close_outside_scope() {
    expect_parse_err(
        "text in a scope with a mismatched }### # comment",
        TestInterpError::RawScopeCloseOutsideRawScope(TestParseSpan("}###")),
    )
}
#[test]
pub fn test_mismatching_raw_scope_close() {
    expect_parse_err(
        "##{ text in a scope with a }#",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParseSpan("##{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_parse_err(
        "text [code",
        TestInterpError::EndedInsideCode {
            code_start: TestParseSpan("["),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_parse_err(
        "text #{raw",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParseSpan("#{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_parse_err(
        "text {scope",
        TestInterpError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
pub fn test_newline_inside_inline_scope() {
    expect_parse_err(
        "text {scope\n",
        TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParseSpan("{"),
            sentence_break: TestParseSpan("\n"),
        },
    )
}
#[test]
pub fn test_block_scope_open_inline_para() {
    expect_parse_err(
        "text {\n",
        TestInterpError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext("text", "", " ")),
            block_scope_open: TestParseSpan("{"),
        },
    )
}
#[test]
pub fn test_block_scope_open_inline_multiline_para() {
    expect_parse_err(
        "1st line
        2nd line
        3rd {\n",
        TestInterpError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "1st",
                " line\n        2nd line\n        3rd",
                " ",
            )),
            block_scope_open: TestParseSpan("{"),
        },
    )
}
#[test]
pub fn test_block_scope_open_inline() {
    expect_parse_err(
        "{text {\n",
        TestInterpError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            block_scope_open: TestParseSpan("{"),
        },
    )
}
#[test]
pub fn test_eof_inside_block_scope() {
    expect_parse_err(
        "{\n",
        TestInterpError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
pub fn test_eof_inside_para_inside_block_scope() {
    // Under some broken parsers the EOF would implicitly end the paragraph but would be stopped there - it wouldn't be picked up by the block scope to emit an error.
    expect_parse_err(
        "{\n paragraph paragraph paragraph EOF",
        TestInterpError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}

#[test]
pub fn test_block_scope_vs_inline_scope() {
    expect_parse(
        r#"{
block scope
}

{inline scope}"#,
        Ok(test_doc(vec![
            TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "block scope",
            )])]),
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![test_text(
                "inline scope",
            )])]]),
        ])),
    )
}

#[test]
pub fn test_strip_leading_whitespace() {
    expect_parse(
        r#"
        Boy I sure hope this isn't indented!
        It would be bad!
        The test would be broken!"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Boy I sure hope this isn't indented!"),
            test_sentence("It would be bad!"),
            test_sentence("The test would be broken!"),
        ])])),
    )
}

#[test]
pub fn test_strip_trailing_whitespace() {
    expect_parse(
        concat!(
            "No whitespace allowed after this! \n",
            "I mean it!                        "
        ),
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("No whitespace allowed after this!"),
            test_sentence("I mean it!"),
        ])])),
    )
}

#[test]
pub fn test_strip_leading_scope_whitespace() {
    expect_parse(
        "{ no leading whitespace}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no leading whitespace")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_trailing_scope_whitespace() {
    expect_parse(
        "{no trailing whitespace }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no trailing whitespace")]),
        ]])])),
    )
}

#[test]
pub fn test_dont_strip_whitespace_between_scopes() {
    expect_parse(
        "{ stuff }     { other stuff }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
            test_text("     "),
            TestInline::InlineScope(vec![test_text("other stuff")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_whitespace_after_scope() {
    expect_parse(
        "{ stuff }     \n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_whitespace_between_scope_end_and_comment() {
    expect_parse(
        "{ stuff }     # stuff in a comment!\n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_trailing_whitespace_before_comment() {
    expect_parse(
        concat!(
            r#"No whitespace allowed after this!   # commented text doesn't prevent that 
"#,
            r#"I mean it!                          # it really doesn't! "#
        ),
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("No whitespace allowed after this!"),
            test_sentence("I mean it!"),
        ])])),
    )
}

#[test]
pub fn test_not_strip_trailing_whitespace_before_escaped_newline() {
    expect_parse(
        r#"
Whitespace is allowed after this \
because you may need it to split up words in sentences."#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Whitespace is allowed after this because you may need it to split up words in \
             sentences.",
        )])])),
    )
}

#[test]
pub fn test_emit_block_from_code() {
    expect_parse(
        "[TEST_BLOCK]",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
pub fn test_cant_emit_block_from_code_inside_paragraph() {
    expect_parse_err(
        "Lorem ipsum!
I'm in a [TEST_BLOCK]",
        TestInterpError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "Lorem",
                " ipsum!\nI'm in a",
                " ",
            )),
            code_span: TestParseSpan("[TEST_BLOCK]"),
        },
    )
}

// TODO test emitting things that can/can't get coerced to inline?

#[test]
pub fn test_raw_scope_emitting_block_from_block_level() {
    expect_parse(
        "[TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
pub fn test_raw_scope_emitting_inline_from_block_level() {
    expect_parse(
        "[TEST_RAW_INLINE_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw("some raw stuff that goes in a block!".into()),
        ]])])),
    )
}

#[test]
pub fn test_raw_scope_cant_emit_block_inside_paragraph() {
    expect_parse_err(
        "Inside a paragraph, you can't [TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a \
         block!}#",
        TestInterpError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "Inside",
                " a paragraph, you can't",
                " ",
            )),
            code_span: TestParseSpan(
                "[TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
            ),
        },
    )
}

#[test]
pub fn test_raw_scope_emitting_inline_inside_paragraph() {
    expect_parse(
        "Inside a paragraph, you can [TEST_RAW_INLINE_BUILDER]#{insert an inline raw!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Inside a paragraph, you can "),
            TestInline::TestOwnedRaw("insert an inline raw!".into()),
        ]])])),
    )
}

#[test]
pub fn test_emitting_none_at_block() {
    expect_parse(
        "
[None]
",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_emitting_none_inline() {
    expect_parse(
        "Check it out, there's [None]!",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Check it out, there's "),
            test_text("!"),
        ]])])),
    )
}

#[test]
pub fn test_assign_and_recall() {
    expect_parse(
        "[x = 5]

[x]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "5",
        )]])])),
    )
}

#[test]
pub fn test_emit_none() {
    expect_parse("[None]", Ok(test_doc(vec![])))
}

#[test]
pub fn test_cant_eval_none_for_block_builder() {
    expect_parse_err(
        "[None]{
    That doesn't make any sense! The owner can't be None
}",
        TestUserPythonExecError::CoercingBlockScopeBuilder {
            code_ctx: TestParseContext("[", "None", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*BlockScopeBuilder.*Got None").unwrap(),
        },
    )
}

#[test]
pub fn test_cant_assign_for_block_builder() {
    expect_parse_err(
        "[x = 5]{
    That doesn't make any sense! The owner can't be an abstract concept of x being something
}",
        TestUserPythonExecError::CoercingBlockScopeBuilder {
            code_ctx: TestParseContext("[", "x = 5", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*BlockScopeBuilder.*Got None").unwrap(),
        },
    )
}

#[test]
pub fn test_cant_assign_for_raw_builder() {
    expect_parse_err(
        "[x = 5]#{That doesn't make any sense! The owner can't be an abstract concept of x being \
         something}#",
        TestUserPythonExecError::CoercingRawScopeBuilder {
            code_ctx: TestParseContext("[", "x = 5", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*RawScopeBuilder.*Got None").unwrap(),
        },
    )
}

#[test]
pub fn test_cant_assign_for_inline_builder() {
    expect_parse_err(
        "[x = 5]{That doesn't make any sense! The owner can't be an abstract concept of x being \
         something}",
        TestUserPythonExecError::CoercingInlineScopeBuilder {
            code_ctx: TestParseContext("[", "x = 5", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*InlineScopeBuilder.*Got None").unwrap(),
        },
    )
}

#[test]
pub fn test_syntax_errs_passed_thru() {
    // The assignment support depends on trying to eval() the expression, that failing with a SyntaxError, and then trying to exec() it.
    // Make sure that something invalid as both still returns a SyntaxError
    expect_parse_err(
        "[1invalid]",
        TestUserPythonExecError::CompilingEvalBrackets {
            code_ctx: TestParseContext("[", "1invalid", "]"),
            code: CString::new("1invalid").unwrap(),
            mode: UserPythonCompileMode::ExecStmts,
            err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
        },
    )
}

#[test]
pub fn test_block_scope_builder_return_none() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed
}",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_block_scope_builder_return_none_with_end_inside_para() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed
}",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_property_calls_get() {
    expect_parse(
        "[TEST_PROPERTY]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "5",
        )]])])),
    )
}

// TODO do all these tests inside a block scope?? as well as at the top level??
mod block_spacing {
    use super::*;

    // These are tests for strict blank-line syntax checking - where the parser ensures that there is always a blank line between two blocks.

    // TODO check separation of headers - they're also block-level elements

    // There should always be a blank line between a paragraph ending and a paragraph starting
    // (otherwise they'd be the same paragraph)
    #[test]
    pub fn test_block_sep_para_para() {
        expect_parse(
            "Paragraph one\n has some content\n\nThis is paragraph two",
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![
                    test_sentence("Paragraph one"),
                    test_sentence("has some content"),
                ]),
                TestBlock::Paragraph(vec![test_sentence("This is paragraph two")]),
            ])),
        );
        expect_parse(
            "Paragraph one\nhas some content\nThis isn't paragraph two!",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![
                test_sentence("Paragraph one"),
                test_sentence("has some content"),
                test_sentence("This isn't paragraph two!"),
            ])])),
        )
    }

    /// There needs to be a blank line between a paragraph ending and a block scope starting.
    /// This seems inconsistent with block-scope-closing - see [test_block_sep_para_scope_close] - but it *is* consistent with code generating a block - see [test_block_sep_para_code] - and that's more important
    /// because in both the scope-open and code cases you're generating a new block.
    /// We want to avoid creating new blocks on adjacent lines to creating other blocks, because that implies they're "together" in some way.
    #[test]
    pub fn test_block_sep_para_scope_open() {
        expect_parse(
            r#"Paragraph one

            {
                New Block
            }"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
                TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence("New Block")])]),
            ])),
        );
        expect_parse_err(
            r#"Paragraph one
            {
                New Block
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(TestParseContext("Paragraph", " one", "\n")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
    }

    // There should always be a blank line between a paragraph ending and code-emitting-block
    // - this is picked up as trying to emit a block inside a paragraph
    #[test]
    pub fn test_block_sep_para_code() {
        expect_parse(
            r#"Paragraph one

            [TEST_BLOCK]"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
                TestBlock::TestOwnedBlock(vec![]),
            ])),
        );
        expect_parse_err(
            r#"Paragraph one
            [TEST_BLOCK]"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(TestParseContext("Paragraph", " one", "\n")),
                next_block_start: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
            },
        )
    }

    // There should always be a blank line between a code-emitting-block and a paragraph starting
    #[test]
    pub fn test_block_sep_code_para() {
        expect_parse(
            r#"[TEST_BLOCK]

            Paragraph one"#,
            Ok(test_doc(vec![
                TestBlock::TestOwnedBlock(vec![]),
                TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
            ])),
        );
        expect_parse_err(
            r#"[TEST_BLOCK] Paragraph one"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("Paragraph")),
            },
        );
        expect_parse_err(
            r#"[TEST_BLOCK]
            Paragraph one"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("Paragraph")),
            },
        )
    }

    // This should *not* trigger insufficient space - it's fine to close a block scope directly after a paragraph
    #[test]
    pub fn test_block_sep_para_scope_close() {
        expect_parse(
            r#"{
                Paragraph one
            }"#,
            Ok(test_doc(vec![TestBlock::BlockScope(vec![
                TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
            ])])),
        );
    }

    // There should need to be a blank line between a scope closing and another scope starting
    #[test]
    pub fn test_block_sep_scope_scope() {
        // Expect a full blank line separating two scops
        expect_parse(
            r#"{
            }

            {
            }"#,
            Ok(test_doc(vec![
                TestBlock::BlockScope(vec![]),
                TestBlock::BlockScope(vec![]),
            ])),
        );
        // Expect an error on no lines separating two scopes
        expect_parse_err(
            r#"{
            } {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockScope(TestParseContext(
                    "{",
                    "\n            ",
                    "}",
                )),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        expect_parse_err(
            r#"[TEST_BLOCK_BUILDER]{
            } {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan(
                    "[TEST_BLOCK_BUILDER]{
            }",
                )),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        // Expect an error on one newline separating scopes
        expect_parse_err(
            r#"{
            }
            {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockScope(TestParseContext(
                    "{",
                    "\n            ",
                    "}",
                )),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        expect_parse_err(
            r#"[TEST_BLOCK_BUILDER]{
            }
            {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan(
                    "[TEST_BLOCK_BUILDER]{
            }",
                )),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        )
    }

    // There needs to be a blank line between a scope closing and code-emitting-block
    #[test]
    pub fn test_block_sep_scope_code() {
        expect_parse(
            r#"{
            }

            [TEST_BLOCK]"#,
            Ok(test_doc(vec![
                TestBlock::BlockScope(vec![]),
                TestBlock::TestOwnedBlock(vec![]),
            ])),
        );
        expect_parse_err(
            r#"{
            }
            [TEST_BLOCK]"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockScope(TestParseContext(
                    "{",
                    "\n            ",
                    "}",
                )),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        );
    }

    // There should always be a blank line between a code-emitting-block and a scope opening
    #[test]
    pub fn test_block_sep_code_scope() {
        expect_parse(
            r#"
            [TEST_BLOCK]

            {
            }"#,
            Ok(test_doc(vec![
                TestBlock::TestOwnedBlock(vec![]),
                TestBlock::BlockScope(vec![]),
            ])),
        );
        expect_parse_err(
            r#"
            [TEST_BLOCK]
            {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        expect_parse_err(
            r#"
            [TEST_BLOCK]      {
            }"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        )
    }

    // There should always be a blank line between two code-emitting-blocks
    #[test]
    pub fn test_block_sep_code_code() {
        expect_parse(
            r#"
            [TEST_BLOCK]

            [TEST_BLOCK]"#,
            Ok(test_doc(vec![
                TestBlock::TestOwnedBlock(vec![]),
                TestBlock::TestOwnedBlock(vec![]),
            ])),
        );
        expect_parse_err(
            r#"
            [TEST_BLOCK]
            [TEST_BLOCK]"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        );
        expect_parse_err(
            r#"[TEST_BLOCK] [TEST_BLOCK_2]"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        )
    }

    #[test]
    pub fn test_inserted_file_newlines_dont_leak_out() {
        expect_parse_err(
            r#"
[-
f = test_src("""
Look a test paragraph

# These newlines should not count outside of this document
# i.e. if we follow this insertion with content on the same line, it should still be picked up as an error






""")
-]

[f] and some more content
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("and")),
            },
        )
    }

    #[test]
    pub fn test_block_sep_para_inserted_file() {
        // We should be able to insert a paragraph and then line break and then insert a file
        expect_parse(
            r#"
[-
f = test_src("""some more content""")
-]

content

[f]
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("content")]),
                TestBlock::Paragraph(vec![test_sentence("some more content")]),
            ])),
        );
        // We shouldn't be able to do that on adjacent lines - the paragraph "captures" any content on the line underneath
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

content
[f]
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(TestParseContext("content", "", "\n")),
                next_block_start: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
            },
        )
    }
    #[test]
    pub fn test_block_sep_inserted_file_para() {
        // There must be a blank line between inserted file and para
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

another paragraph of content
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::Paragraph(vec![test_sentence("another paragraph of content")]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
another paragraph of content
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("another")),
            },
        );
        // We shouldn't be able to both on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f] and some more content
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("and")),
            },
        )
    }
    #[test]
    pub fn test_block_sep_inserted_file_inserted_file() {
        // We must have a blank line between two files
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

[f]
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::Paragraph(vec![test_sentence("some content")]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
[f]
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        );
        // We shouldn't be able to two on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f] [f]
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        )
    }
    #[test]
    pub fn test_block_sep_inserted_file_block_code() {
        // We must have a blank line between a file and block code
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

[TEST_BLOCK_BUILDER]{
    some other content
}
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::TestOwnedBlock(vec![TestBlock::Paragraph(vec![test_sentence(
                    "some other content",
                )])]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
[TEST_BLOCK_BUILDER]{
    some other content
}
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        );
        // We shouldn't be able to two on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f] [TEST_BLOCK_BUILDER]{
    some other content
}
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        )
    }

    #[test]
    pub fn test_block_sep_inserted_file_inline_code() {
        // We must have a blank line between a file and inline code
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

[TEST_INLINE_BUILDER]{some other content}
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::Paragraph(vec![vec![TestInline::TestOwnedInline(vec![test_text(
                    "some other content",
                )])]]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
[TEST_INLINE_BUILDER]{some other content}
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        );
        // We shouldn't be able to two on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f] [TEST_INLINE_BUILDER]{some other content}
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
            },
        )
    }

    #[test]
    pub fn test_block_sep_inserted_file_block_scope() {
        // We must have a blank line between a file and block scopes
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

{
    some other content
}
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "some other content",
                )])]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
{
    some other content
}
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        // We shouldn't be able to two on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]    {
    some other content
}
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        )
    }
    #[test]
    pub fn test_block_sep_inserted_file_inline_scope() {
        // We must have a blank line between a file and inline scope
        expect_parse(
            r#"
[-
f = test_src("""some content""")
-]

[f]

{ some other content }
"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("some content")]),
                TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![test_text(
                    "some other content",
                )])]]),
            ])),
        );
        // We shouldn't be able to two on adjacent lines
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]
{ some other content }
"#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
        // We shouldn't be able to two on the same line
        expect_parse_err(
            r#"
[-
f = test_src("""some content""")
-]

[f]    { some other content }
        "#,
            TestInterpError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(TestParseSpan("[f]")),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        )
    }
}

/// Tests that the implicit structure mechanism is working correctly
mod doc_structure {
    use super::*;

    #[test]
    fn test_many_inner_levels() {
        expect_parse(
            "
        outside

        [TestDocSegmentHeader(weight=1)]

        light!

        [TestDocSegmentHeader(weight=2)]
        
        [TestDocSegmentHeader(weight=30)]

        [TestDocSegmentHeader(weight=54)]
        
        middling

        [TestDocSegmentHeader(weight=67)]

        [TestDocSegmentHeader(weight=100)]

        heAVEY

        ",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "outside",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: Some((1, None, None)),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("light!"),
                    ])]),
                    subsegments: vec![TestDocSegment {
                        header: Some((2, None, None)),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![TestDocSegment {
                            header: Some((30, None, None)),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![TestDocSegment {
                                header: Some((54, None, None)),
                                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                    test_sentence("middling"),
                                ])]),
                                subsegments: vec![TestDocSegment {
                                    header: Some((67, None, None)),
                                    contents: TestBlock::BlockScope(vec![]),
                                    subsegments: vec![TestDocSegment {
                                        header: Some((100, None, None)),
                                        contents: TestBlock::BlockScope(vec![
                                            TestBlock::Paragraph(vec![test_sentence("heAVEY")]),
                                        ]),
                                        subsegments: vec![],
                                    }],
                                }],
                            }],
                        }],
                    }],
                }],
            }),
        )
    }

    #[test]
    fn test_bouncing_in_and_out() {
        expect_parse(
            "
        outside

        [TestDocSegmentHeader(weight=1)]

        light!

        [TestDocSegmentHeader(weight=2)]
        
        [TestDocSegmentHeader(weight=30)]

        [TestDocSegmentHeader(weight=54)]
        
        middling

        [TestDocSegmentHeader(weight=2)]

        [TestDocSegmentHeader(weight=20)]

        middling again!

        ",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "outside",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: Some((1, None, None)),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("light!"),
                    ])]),
                    subsegments: vec![
                        TestDocSegment {
                            header: Some((2, None, None)),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![TestDocSegment {
                                header: Some((30, None, None)),
                                contents: TestBlock::BlockScope(vec![]),
                                subsegments: vec![TestDocSegment {
                                    header: Some((54, None, None)),
                                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(
                                        vec![test_sentence("middling")],
                                    )]),
                                    subsegments: vec![],
                                }],
                            }],
                        },
                        TestDocSegment {
                            header: Some((2, None, None)),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![TestDocSegment {
                                header: Some((20, None, None)),
                                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                    test_sentence("middling again!"),
                                ])]),
                                subsegments: vec![],
                            }],
                        },
                    ],
                }],
            }),
        )
    }

    #[test]
    fn test_bouncing_out_from_under() {
        expect_parse(
            "
        outside

        [TestDocSegmentHeader(weight=10)]

        1st level

        [TestDocSegmentHeader(weight=0)]
        
        1st level

        [TestDocSegmentHeader(weight=10)]

        2nd level

        ",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "outside",
                )])]),
                subsegments: vec![
                    TestDocSegment {
                        header: Some((10, None, None)),
                        contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                            test_sentence("1st level"),
                        ])]),
                        subsegments: vec![],
                    },
                    TestDocSegment {
                        header: Some((0, None, None)),
                        contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                            test_sentence("1st level"),
                        ])]),
                        subsegments: vec![TestDocSegment {
                            header: Some((10, None, None)),
                            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                test_sentence("2nd level"),
                            ])]),
                            subsegments: vec![],
                        }],
                    },
                ],
            }),
        )
    }

    #[test]
    fn test_negative_weight() {
        expect_parse(
            "
        outside

        [TestDocSegmentHeader(weight=1)]

        light!

        [TestDocSegmentHeader(weight=2)]
        
        [TestDocSegmentHeader(weight=-10)]

        [TestDocSegmentHeader(weight=54)]
        
        middling

        [TestDocSegmentHeader(weight=67)]

        [TestDocSegmentHeader(weight=100)]

        heAVEY

        ",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "outside",
                )])]),
                subsegments: vec![
                    TestDocSegment {
                        header: Some((1, None, None)),
                        contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                            test_sentence("light!"),
                        ])]),
                        subsegments: vec![TestDocSegment {
                            header: Some((2, None, None)),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![],
                        }],
                    },
                    TestDocSegment {
                        header: Some((-10, None, None)),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![TestDocSegment {
                            header: Some((54, None, None)),
                            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                test_sentence("middling"),
                            ])]),
                            subsegments: vec![TestDocSegment {
                                header: Some((67, None, None)),
                                contents: TestBlock::BlockScope(vec![]),
                                subsegments: vec![TestDocSegment {
                                    header: Some((100, None, None)),
                                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(
                                        vec![test_sentence("heAVEY")],
                                    )]),
                                    subsegments: vec![],
                                }],
                            }],
                        }],
                    },
                ],
            }),
        )
    }

    #[test]
    fn test_cant_create_header_block_scope() {
        expect_parse_err(
            "{
    [TestDocSegmentHeader()]
    }",
            TestInterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start: TestParseSpan("{"),
                code_span: TestParseSpan("[TestDocSegmentHeader()]"),
            },
        )
    }

    #[test]
    fn test_cant_build_header_block_scope() {
        expect_parse_err(
            "{
    [TestDocSegmentBuilder()]{
        Sometimes docsegmentheaders can be built, too!
        But if they're in a block scope it shouldn't be allowed :(
    }
    }",
            TestInterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start: TestParseSpan("{"),
                code_span: TestParseSpan(
                    "[TestDocSegmentBuilder()]{
        Sometimes docsegmentheaders can be built, too!
        But if they're in a block scope it shouldn't be allowed :(
    }",
                ),
            },
        )
    }

    #[test]
    fn test_cant_create_header_block_scope_argument() {
        expect_parse_err(
            "[TEST_BLOCK_BUILDER]{
    [TestDocSegmentHeader()]
    }",
            TestInterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start: TestParseSpan("{"),
                code_span: TestParseSpan("[TestDocSegmentHeader()]"),
            },
        )
    }

    #[test]
    fn test_can_create_header_toplevel_file() {
        expect_parse(
            "
        Toplevel content!

        [TestDocSegmentHeader(weight=123)]
        
        More content!",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "Toplevel content!",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: Some((123, None, None)),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("More content!"),
                    ])]),
                    subsegments: vec![],
                }],
            }),
        )
    }

    #[test]
    fn test_can_create_header_inner_file() {
        expect_parse(
            r#"
[-
header_in_file = test_src("""[TestDocSegmentHeader(weight=123)]

Content in file!
""")
-]
        Toplevel content!

        [header_in_file]
        
        Content outside file!"#,
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "Toplevel content!",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: Some((123, None, None)),
                    contents: TestBlock::BlockScope(vec![
                        TestBlock::Paragraph(vec![test_sentence("Content in file!")]),
                        TestBlock::Paragraph(vec![test_sentence("Content outside file!")]),
                    ]),
                    subsegments: vec![],
                }],
            }),
        )
    }

    #[test]
    fn test_cant_create_header_block_scope_in_inner_file() {
        expect_parse_err(
            r#"
[-
header_in_file = test_src("""
{
    [TestDocSegmentHeader(weight=123)]

    Content in file!
}""")
-]
        Toplevel content!

        [header_in_file]
        
        Content outside file!"#,
            TestInterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start: TestParseSpan("{"),
                code_span: TestParseSpan("[TestDocSegmentHeader(weight=123)]"),
            },
        )
    }

    #[test]
    fn test_cant_create_header_inner_file_in_block_scope() {
        expect_parse_err(
            r#"
[-
header_in_file = test_src("""
[TestDocSegmentHeader(weight=123)]

Content in file!
""")
-]
        Toplevel content!

        {
            [header_in_file]
            
            Content outside file!
        }"#,
            TestInterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start: TestParseSpan("{"),
                code_span: TestParseSpan("[TestDocSegmentHeader(weight=123)]"),
            },
        )
    }

    #[test]
    fn test_cant_create_header_in_paragraph() {
        expect_parse_err(
            "And as I was saying [TestDocSegmentHeader()]",
            TestInterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                    "And",
                    " as I was saying",
                    " ",
                )),
                code_span: TestParseSpan("[TestDocSegmentHeader()]"),
            },
        )
    }

    #[test]
    fn test_cant_create_header_inline() {
        expect_parse_err(
            "[TEST_BLOCK_BUILDER_FROM_INLINE]{ [TestDocSegmentHeader()] }",
            TestInterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan("[TestDocSegmentHeader()]"),
            },
        )
    }
}

mod inserted_files {
    use super::*;

    #[test]
    fn test_inserted_file_top_level() {
        expect_parse(
            r#"
        [test_src("paragraph 1")]
        
        [test_src("paragraph 2")]"#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("paragraph 1")]),
                TestBlock::Paragraph(vec![test_sentence("paragraph 2")]),
            ])),
        )
    }

    #[test]
    fn test_inserted_file_in_block_scope() {
        expect_parse(
            r#"
            paragraph 1

            {
                paragraph 2

                [test_src("paragraph 3")]
            }
            "#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("paragraph 1")]),
                TestBlock::BlockScope(vec![
                    TestBlock::Paragraph(vec![test_sentence("paragraph 2")]),
                    TestBlock::Paragraph(vec![test_sentence("paragraph 3")]),
                ]),
            ])),
        )
    }

    #[test]
    fn test_nested_inserted_file() {
        expect_parse(
            r#"
[-
f1 = test_src("""
    paragraph 1

    [f2]
""")
f2 = test_src("""
    paragraph 2

    [f3]
""")
f3 = test_src("""
    paragraph 3

    [f4]
""")
f4 = test_src("""
    paragraph 4
""")
-]
            # Include the first file, which will include the second, then the third etc.
            # None of them have block scopes so they'll all emit paragraphs to the top level
            [f1]
            "#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("paragraph 1")]),
                TestBlock::Paragraph(vec![test_sentence("paragraph 2")]),
                TestBlock::Paragraph(vec![test_sentence("paragraph 3")]),
                TestBlock::Paragraph(vec![test_sentence("paragraph 4")]),
            ])),
        )
    }

    #[test]
    fn test_combined_nested_inserted_file_block_scope() {
        expect_parse(
            r#"
[-
f1 = test_src("""
    paragraph 1

    {
        [f2]

        [f3]
    }
""")
f2 = test_src("""
    paragraph 2
""")
f3 = test_src("""
    paragraph 3

    {
        [f4]
    }
""")
f4 = test_src("""
    paragraph 4
""")
-]
            # Include the first file, which will include the second, then the third etc.
            [f1]
            "#,
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![test_sentence("paragraph 1")]),
                TestBlock::BlockScope(vec![
                    TestBlock::Paragraph(vec![test_sentence("paragraph 2")]),
                    TestBlock::Paragraph(vec![test_sentence("paragraph 3")]),
                    TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                        "paragraph 4",
                    )])]),
                ]),
            ])),
        )
    }

    #[test]
    fn test_inserted_file_in_builder_arg() {
        expect_parse(
            r#"
        [TEST_BLOCK_BUILDER]{
            [test_src("some valid stuff in an inner file")]
        }"#,
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![
                TestBlock::Paragraph(vec![test_sentence("some valid stuff in an inner file")]),
            ])])),
        )
    }

    // Can't insert a file in any inline mode
    #[test]
    fn test_no_inserted_file_in_paragraph() {
        expect_parse_err(
            r#"wow i'm inside a paragraph! [test_src("some more data O.O")]"#,
            TestInterpError::CodeEmittedSourceInInlineMode {
                inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                    "wow",
                    " i'm inside a paragraph!",
                    " ",
                )),
                code_span: TestParseSpan(r#"[test_src("some more data O.O")]"#),
            },
        )
    }

    #[test]
    fn test_no_inserted_file_in_inline_scope() {
        expect_parse_err(
            r#"{ wow i'm inside an inline scope! [test_src("some more data O.O")] }"#,
            TestInterpError::CodeEmittedSourceInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan(r#"[test_src("some more data O.O")]"#),
            },
        )
    }

    #[test]
    fn test_no_inserted_file_in_inline_builder() {
        expect_parse_err(
            r#"[TEST_INLINE_BUILDER]{wow i'm inside an inline scope builder! [test_src("some more data O.O")] }"#,
            TestInterpError::CodeEmittedSourceInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan(r#"[test_src("some more data O.O")]"#),
            },
        )
    }

    #[test]
    fn test_unbalanced_scope_inside_inserted_file() {
        // The close-scope inside the inserted file should not bubble out and close the scope in the top-level document.
        // If it did, the file would parse successfully. If it didn't, the file will return an unbalanced scope error.
        expect_parse_err(
            r#"
            {
                [test_src("}")]
            "#,
            TestInterpError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
        )
    }
}

mod flexibility {
    // All kinds of builder should be able to build inlines, even if their arguments aren't inline

    use super::*;

    #[test]
    fn test_inline_scope_builder_building_inline() {
        expect_parse(
            "building [TEST_INLINE_BUILDER]{something built} inline",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("building "),
                TestInline::TestOwnedInline(vec![test_text("something built")]),
                test_text(" inline"),
            ]])])),
        )
    }

    #[test]
    fn test_block_scope_builder_building_inline() {
        // We can enter Block mode when building something that consumes block, but if it returns inline
        // then we can continue in inline mode on the same line
        expect_parse(
            r#"
        building [TestDummyInlineBuilderFromBlock("dummy")]{
            lots of blocks!

            even more blocks!

            [TEST_BLOCK_BUILDER]{
                blocks inside blocks! [TEST_INLINE_BUILDER]{ with otehr stuff in them! }
            }
        } stuff # this is on the same line!
        "#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("building "),
                test_text("dummy"),
                test_text(" stuff"),
            ]])])),
        )
    }

    #[test]
    fn test_raw_scope_builder_building_inline() {
        expect_parse(
            "building [TEST_RAW_INLINE_BUILDER]#{some raw stuff}#",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("building "),
                TestInline::TestOwnedRaw("some raw stuff".to_string()),
            ]])])),
        )
    }

    // Make sure all the kinds of builder that emit inlines start an inline, and don't just create and close a paragraph

    #[test]
    fn test_inline_scope_builder_building_inline_creates_paragraph() {
        expect_parse(
            "[TEST_INLINE_BUILDER]{something built} inline",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![test_text("something built")]),
                test_text(" inline"),
            ]])])),
        )
    }

    #[test]
    fn test_block_scope_builder_building_inline_creates_paragraph() {
        // We can enter Block mode when building something that consumes block, but if it returns None
        // then we can continue in inline mode on the same line
        expect_parse(
            r#"
        [TestDummyInlineBuilderFromBlock("dummy")]{
            lots of blocks!

            even more blocks!

            [TEST_BLOCK_BUILDER]{
                blocks inside blocks! [TEST_INLINE_BUILDER]{ with otehr stuff in them! }
            }
        } stuff # this is on the same line!
        "#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("dummy"),
                test_text(" stuff"),
            ]])])),
        )
    }

    #[test]
    fn test_raw_scope_builder_building_inline_creates_paragraph() {
        expect_parse(
            "[TEST_RAW_INLINE_BUILDER]#{some raw stuff}# and this continues",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedRaw("some raw stuff".to_string()),
                test_text(" and this continues"),
            ]])])),
        )
    }

    // All kinds of builder should be able to build blocks, even if their arguments aren't blocks

    #[test]
    fn test_inline_scope_builder_building_block() {
        expect_parse(
            "[TEST_BLOCK_BUILDER_FROM_INLINE]{only inlines :)}",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![
                TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![TestInline::Text(
                    "only inlines :)".to_string(),
                )])]]),
            ])])),
        )
    }

    #[test]
    fn test_block_scope_builder_building_block() {
        expect_parse(
            r#"
            [TEST_BLOCK_BUILDER]{
                Stuff
            }
        "#,
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![
                TestBlock::Paragraph(vec![test_sentence("Stuff")]),
            ])])),
        )
    }

    #[test]
    fn test_raw_scope_builder_building_block() {
        expect_parse(
            "[TEST_RAW_BLOCK_BUILDER]#{ block! }#",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    // Even if each builder can build blocks, they shouldn't be able to emit a block in an inline context

    #[test]
    fn test_inline_scope_builder_building_block_in_inline() {
        expect_parse_err(
            "{wow i'm in an inline context [TEST_BLOCK_BUILDER_FROM_INLINE]{only inlines :)}}",
            TestInterpError::CodeEmittedBlockInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan("[TEST_BLOCK_BUILDER_FROM_INLINE]{only inlines :)}"),
            },
        )
    }

    #[test]
    fn test_block_scope_builder_building_block_in_inline() {
        expect_parse_err(
            r#"
            {wow i'm in an inline context [TEST_BLOCK_BUILDER]{
                Stuff
            } continuing the inline context}
        "#,
            TestInterpError::CodeEmittedBlockInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan(
                    "[TEST_BLOCK_BUILDER]{\n                Stuff\n            }",
                ),
            },
        )
    }

    #[test]
    fn test_raw_scope_builder_building_block_in_inline() {
        expect_parse_err(
            "{wow i'm in an inline context [TEST_RAW_BLOCK_BUILDER]#{ block! }# continuing the \
             inline context}",
            TestInterpError::CodeEmittedBlockInInlineMode {
                inl_mode: TestInlineModeContext::InlineScope {
                    scope_start: TestParseSpan("{"),
                },
                code_span: TestParseSpan("[TEST_RAW_BLOCK_BUILDER]#{ block! }#"),
            },
        )
    }

    // All kinds of builder should be able to build headers, even if their arguments aren't blocks
    #[test]
    fn test_inline_scope_builder_building_header() {
        expect_parse(
            "[TestDocSegmentBuilder()]{ Wowee i wish I had inline content }",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![TestDocSegment {
                    header: Some((
                        0,
                        None,
                        Some(TestInline::InlineScope(vec![test_text(
                            "Wowee i wish I had inline content",
                        )])),
                    )),
                    contents: TestBlock::BlockScope(vec![]),
                    subsegments: vec![],
                }],
            }),
        )
    }

    #[test]
    fn test_block_scope_builder_building_header() {
        expect_parse(
            "[TestDocSegmentBuilder()]{
            Wowee i wish I had block content
        }",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![TestDocSegment {
                    header: Some((
                        0,
                        Some(TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                            test_sentence("Wowee i wish I had block content"),
                        ])])),
                        None,
                    )),
                    contents: TestBlock::BlockScope(vec![]),
                    subsegments: vec![],
                }],
            }),
        )
    }

    #[test]
    fn test_raw_scope_builder_building_header() {
        expect_parse(
            "[TestDocSegmentBuilder()]#{ Wowee i wish I had inline content }#",
            Ok(TestDocSegment {
                header: None,
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![TestDocSegment {
                    header: Some((
                        0,
                        None,
                        Some(TestInline::InlineScope(vec![test_raw_text(
                            " Wowee i wish I had inline content ",
                        )])),
                    )),
                    contents: TestBlock::BlockScope(vec![]),
                    subsegments: vec![],
                }],
            }),
        )
    }

    // Even if each builder can build blocks, they shouldn't be able to emit a header in an inline context

    #[test]
    fn test_inline_scope_builder_building_header_in_inline_mode_para() {
        expect_parse_err(
            "And as I was saying [TestDocSegmentBuilder()]{ Wowee i wish I had inline content }",
            TestInterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                    "And",
                    " as I was saying",
                    " ",
                )),
                code_span: TestParseSpan(
                    "[TestDocSegmentBuilder()]{ Wowee i wish I had inline content }",
                ),
            },
        )
    }

    #[test]
    fn test_block_scope_builder_building_header_in_inline() {
        expect_parse_err(
            "And as I was saying [TestDocSegmentBuilder()]{
                Wowee i wish I had block content
            }",
            TestInterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                    "And",
                    " as I was saying",
                    " ",
                )),
                code_span: TestParseSpan(
                    "[TestDocSegmentBuilder()]{
                Wowee i wish I had block content
            }",
                ),
            },
        )
    }

    #[test]
    fn test_raw_scope_builder_building_header_in_inline() {
        expect_parse_err(
            "And as I was saying [TestDocSegmentBuilder()]#{ Wowee i wish 
                I had inline 
                and raw
                content }#",
            TestInterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                    "And",
                    " as I was saying",
                    " ",
                )),
                code_span: TestParseSpan(
                    "[TestDocSegmentBuilder()]#{ Wowee i wish 
                I had inline 
                and raw
                content }#",
                ),
            },
        )
    }

    // All kinds of builder should be able to build None

    // if an inline emits None inside a sentence with other content
    #[test]
    fn test_inline_scope_builder_building_none_inside_sentence() {
        expect_parse(
            "stuff at the start of a sentence [TEST_INLINE_SWALLOWER]{ this is gonna be swallowed \
             }
            [TEST_INLINE_SWALLOWER]{ this is gonna be swallowed } stuff at the end of a sentence",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![
                test_sentence("stuff at the start of a sentence "), // Note that this has the ending space - whitespace content is flushed to the text when the eval-brackets start
                test_sentence(" stuff at the end of a sentence"), // Note that this has the leading space - whitespace is counted after inline scopes and code, but not inside inline scopes
            ])])),
        )
    }

    // if an inline emits None and that's the whole sentence, it isn't added as a sentence inside the paragraph
    #[test]
    fn test_inline_scope_builder_building_none_sentence_inside_para() {
        expect_parse(
            "
            Wow what a lovely paragraph.
            [TEST_INLINE_SWALLOWER]{ this is gonna be swallowed }
            Yes, isn't it?",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![
                test_sentence("Wow what a lovely paragraph."),
                test_sentence("Yes, isn't it?"),
            ])])),
        )
    }

    // if all a paragraph has is None sentences i.e. nothing, it isn't emitted at all.
    // Actually, at this level emitting None emits it at the block level - you'd need at least enough content for a sentence to start a paragraph - so this happens even if the paragraph code doesn't handle it
    #[test]
    fn test_inline_scope_builder_building_none_para() {
        expect_parse(
            "[TEST_INLINE_SWALLOWER]{ this is gonna be swallowed }
            [TEST_INLINE_SWALLOWER]{ so is this }",
            Ok(test_doc(vec![])),
        )
    }

    #[test]
    fn test_block_scope_builder_building_none() {
        expect_parse(
            "[TEST_BLOCK_SWALLOWER]{
                this is gonna be swallowed
            
                so is this!
            }",
            Ok(test_doc(vec![])),
        )
    }

    #[test]
    fn test_raw_scope_builder_building_none() {
        expect_parse(
            "[TEST_RAW_SWALLOWER]#{ this is gonna be swallowed }#",
            Ok(test_doc(vec![])),
        )
    }
}

/// This contains tests for the code parser where it is initially ambiguous whether code is meant to be an owner or not.
mod code_ambiguity {
    use super::*;

    #[test]
    fn test_code_followed_by_newline_doesnt_build() {
        // Create a class which is a block scope + inline scope + raw scope builder all in one, and also a block in its own right! See what happens when we create it with no owning responsibilities
        expect_parse(
            r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]

"#,
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    #[test]
    fn test_code_followed_by_content_doesnt_build() {
        // Create a class which is a block scope + inline scope + raw scope builder all in one, and also a block in its own right! See what happens when we create it with no owning responsibilities
        expect_parse(
            r#"
[-
class Super:
    is_inline = True
    test_inline = InlineScope([])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()] and stuff

"#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![]),
                test_text(" and stuff"),
            ]])])),
        )
    }

    #[test]
    fn test_code_followed_by_block_scope_must_build() {
        // Test that things that can build do in fact build.
        // In this case make a big thing that would give a block of text if it was used directly, and None if it's used as a builder.
        // Assert it returns None.
        expect_parse(
            r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        return None
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]{
    stuff
}

"#,
            Ok(test_doc(vec![])),
        );
        // Test that things that can't build throw errors
        expect_parse_err(
            "[TEST_BLOCK]{
        }",
            TestUserPythonExecError::CoercingBlockScopeBuilder {
                code_ctx: TestParseContext("[", "TEST_BLOCK", "]"),
                err: Regex::new(r"TypeError\s*:\s*Expected.*BlockScopeBuilder.*Got <FauxBlock.*")
                    .unwrap(),
            },
        )
    }

    #[test]
    fn test_code_followed_by_inline_scope_must_build() {
        // Test that things that can build do in fact build.
        // In this case make a big thing that would give inlines if it was used directly, and a sentinel if it's used as a builder.
        // Assert it returns the sentinel.
        expect_parse(
            r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        return TEST_INLINE
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]{ stuff }

"#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![]),
            ]])])),
        );
        // Test that things that can't build throw errors
        expect_parse_err(
            "[TEST_INLINE]{}",
            TestUserPythonExecError::CoercingInlineScopeBuilder {
                code_ctx: TestParseContext("[", "TEST_INLINE", "]"),
                err: Regex::new(r"TypeError\s*:\s*Expected.*InlineScopeBuilder.*Got <FauxInline.*")
                    .unwrap(),
            },
        )
    }

    #[test]
    fn test_code_followed_by_raw_scope_must_build() {
        // Test that things that can build do in fact build.
        // In this case make a big thing that would give inlines if it was used directly, and a sentinel if it's used as a builder.
        // Assert it returns the sentinel.
        expect_parse(
            r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        return TEST_INLINE_RAW
-]

[Super()]#{ stuff }#

"#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedRaw("".to_string()),
            ]])])),
        );
        // Test that things that can't build throw errors
        expect_parse_err(
            "[TEST_INLINE]#{}#",
            TestUserPythonExecError::CoercingRawScopeBuilder {
                code_ctx: TestParseContext("[", "TEST_INLINE", "]"),
                err: Regex::new(r"TypeError\s*:\s*Expected.*RawScopeBuilder.*Got <FauxInline.*")
                    .unwrap(),
            },
        )
    }
}

/// This contains tests for the scope parser where it is initially ambiguous whether a scope is inline or block.
mod scope_ambiguity {
    use super::*;

    // Test that block scopes can be opened with various whitespace elements between them and the newline

    #[test]
    fn block_scope_opened_with_direct_newline() {
        expect_parse("{\n}", Ok(test_doc(vec![TestBlock::BlockScope(vec![])])))
    }

    #[test]
    fn block_scope_opened_with_whitespaces_then_newline() {
        expect_parse(
            "{       \n}",
            Ok(test_doc(vec![TestBlock::BlockScope(vec![])])),
        )
    }

    #[test]
    fn block_scope_opened_with_whitespaces_then_comment_then_newline() {
        expect_parse(
            "{       # wowie a comment!\n}",
            Ok(test_doc(vec![TestBlock::BlockScope(vec![])])),
        )
    }

    #[test]
    fn block_scope_opened_with_comment() {
        expect_parse(
            "{# wowie a comment\n}",
            Ok(test_doc(vec![TestBlock::BlockScope(vec![])])),
        )
    }

    // Test the same thing but with code owners on the front

    #[test]
    fn code_block_scope_opened_with_direct_newline() {
        expect_parse(
            "[TEST_BLOCK_BUILDER]{\n}",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    #[test]
    fn code_block_scope_opened_with_whitespaces_then_newline() {
        expect_parse(
            "[TEST_BLOCK_BUILDER]{       \n}",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    #[test]
    fn code_block_scope_opened_with_whitespaces_then_comment_then_newline() {
        expect_parse(
            "[TEST_BLOCK_BUILDER]{       # wowie a comment!\n}",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    #[test]
    fn code_block_scope_opened_with_comment() {
        expect_parse(
            "[TEST_BLOCK_BUILDER]{# wowie a comment\n}",
            Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
        )
    }

    // Test that inline scopes can be opened with and without whitespace between them and their first contents
    #[test]
    fn inline_scope_opened_with_direct_content() {
        expect_parse(
            "{inline}",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::InlineScope(vec![test_text("inline")]),
            ]])])),
        )
    }

    #[test]
    fn inline_scope_opened_with_whitespaces_then_content() {
        expect_parse(
            "{       inline      }",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::InlineScope(vec![test_text("inline")]),
            ]])])),
        )
    }

    // Test the same thing but with code owners on the front
    #[test]
    fn code_inline_scope_opened_with_direct_content() {
        expect_parse(
            "[TEST_INLINE_BUILDER]{inline}",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![test_text("inline")]),
            ]])])),
        )
    }

    #[test]
    fn code_inline_scope_opened_with_whitespaces_then_content() {
        expect_parse(
            "[TEST_INLINE_BUILDER]{       inline      }",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![test_text("inline")]),
            ]])])),
        )
    }

    // Empty scopes should count as inline because there are no newlines inside
    #[test]
    fn empty_scopes_are_inline() {
        expect_parse(
            "{}",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::InlineScope(vec![]),
            ]])])),
        )
    }

    #[test]
    fn scopes_with_escaped_newlines_are_inline() {
        expect_parse(
            r#"{\
\
}"#,
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::InlineScope(vec![]),
            ]])])),
        )
    }

    #[test]
    fn code_empty_scopes_are_inline() {
        expect_parse(
            "[TEST_INLINE_BUILDER]{}",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                TestInline::TestOwnedInline(vec![]),
            ]])])),
        )
    }

    // EOFs inside block and inline scopes should both fail equally

    #[test]
    fn eof_in_inline_scope() {
        expect_parse_err(
            "{ wow some data and then EOF",
            TestInterpError::EndedInsideScope {
                scope_start: TestParseSpan("{"),
                eof_span: TestParseSpan(""),
            },
        )
    }

    #[test]
    fn eof_in_block_scope() {
        expect_parse_err(
            "{   \n wow some data and then EOF",
            TestInterpError::EndedInsideScope {
                scope_start: TestParseSpan("{"),
                eof_span: TestParseSpan(""),
            },
        )
    }
}

/// This contains tests for situations that are currently allowed but probably shouldn't be.
mod overflexibility {
    use super::*;

    // todo
    /*

    {
        In a block scope

        [TEST_BLOCK_BUILDER_FROM_INLINE]{some inline stuff}}

    The last } might close the block scope despite being on the same line?

    I really think forcing Correct block spacing is the wrong way to go.
    It's a useful error message when trying to do stupid shit, and certainly putting content directly after a block closes
    */

    /// Code automatic-indent support can sometimes allow code with inconsistent indents, which could lead to issues.
    /// Prepending `if True` only works for exactly one level of indent.
    ///
    /// e.g.
    /// ```python
    /// # (1) Supported
    /// [--
    ///    code_with_indent
    /// --]
    /// # (2) Also supported, because Python doesn't complain about dedenting to zero
    /// [--
    ///    code_with_indent
    /// code_without_indent
    /// --]
    /// # (3) Not supported, because the zero-indent resets expectations
    /// [--
    ///     code_with_indent
    /// code_without_indent
    ///     code_with_indent
    /// --]
    /// # (4) Not supported, because Python picks up two non-zero indent levels
    /// [--
    ///     code_with_indent
    ///   code_less_indented
    /// --]
    /// ```
    ///
    /// Specifically, case (2) is weird.
    #[test]
    fn code_incorrect_indent() {
        expect_parse(
            r"
[--
    indented_x = 3

not_indented_y = 5
--]

        [indented_x] [not_indented_y]
        ",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("3"),
                test_text(" "),
                test_text("5"),
            ]])])),
        )
    }

    // A downside of using inner dashes is that negative literals require a space
    #[test]
    fn test_code_negative_literal() {
        expect_parse_err(
            "[-1]",
            TestInterpError::EndedInsideCode {
                code_start: TestParseSpan("[-"),
                eof_span: TestParseSpan("]"),
            },
        )
    }
}

/// This module checks two kinds of substitution - the only ones performed on text content.
/// - Newlines, whether they be \r\n or \r or \n, are all translated to \n - even in raw scopes
/// - Strings of hyphens
mod substitution {
    use super::*;

    #[test]
    fn test_raw_newlines() {
        expect_parse(
            "#{\r}# #{\n}# #{\r\n}#",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_raw_text("\n"),
                test_text(" "),
                test_raw_text("\n"),
                test_text(" "),
                test_raw_text("\n"),
            ]])])),
        )
    }

    #[test]
    fn test_short_hyphen_strings() {
        expect_parse(
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

    \----
    ---\-
    \---\-
    \----\-

    iubdqiouwbdw---iubwduibqwdb
    uqwbduibdqiudw--iqwbdiuqwbud

    \-\-\-\-\-\-\-\-",
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![
                    test_sentence("-"),
                    // Escaped dash = ASCII dash
                    test_sentence("-"),
                ]),
                TestBlock::Paragraph(vec![
                    // two dashes = en-dash literal
                    test_sentence("\u{2013}"),
                    // If one is escaped, it becomes a normal ASCII dash and the other is just a normal ASCII dash too
                    test_sentence("--"),
                    test_sentence("--"),
                ]),
                TestBlock::Paragraph(vec![
                    // three dashes = em-dash literal
                    test_sentence("\u{2014}"),
                    // \--- = \- + -- = escaped + en-dash
                    test_sentence("-\u{2013}"),
                    // -\-- = - + \- + - = three ASCII
                    test_sentence("---"),
                    // --\- = en-dash + escaped
                    test_sentence("\u{2013}-"),
                    // \-\-- = \- + \- + - = three ASCII
                    test_sentence("---"),
                    // -\-\- = - + \- + \- = three ASCII
                    test_sentence("---"),
                    // \-\-\- = three escaped = three ASCII
                    test_sentence("---"),
                ]),
                TestBlock::Paragraph(vec![
                    test_sentence("-\u{2014}"),
                    test_sentence("\u{2014}-"),
                    test_sentence("-\u{2013}-"),
                    test_sentence("-\u{2014}-"),
                ]),
                TestBlock::Paragraph(vec![
                    test_sentence("iubdqiouwbdw\u{2014}iubwduibqwdb"),
                    test_sentence("uqwbduibdqiudw\u{2013}iqwbdiuqwbud"),
                ]),
                TestBlock::Paragraph(vec![test_sentence("--------")]),
            ])),
        );
    }

    #[test]
    fn test_long_hyphen_strings() {
        expect_parse(
            "----",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "----",
            )])])),
        );
        expect_parse(
            "-----",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "-----",
            )])])),
        );
        expect_parse(
            "------",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "------",
            )])])),
        );
        expect_parse(
            "-------",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "-------",
            )])])),
        );
        expect_parse(
            "--------",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "--------",
            )])])),
        );
        expect_parse(
            "---------",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "---------",
            )])])),
        );
        expect_parse(
            "----------",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "----------",
            )])])),
        );
    }

    // See notes/syntax.md - the reason we use dashes *inside* eval-brackets for disambiguation, instead of outside, is so that we can have all the nice hyphen constructs directly next to code.
    #[test]
    fn test_hyphens_outside_code() {
        expect_parse(
            r"-[1]--[2]---[3]---[4]-\--[5]\-[6]\-[7]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("-"),
                test_text("1"),
                test_text("\u{2013}"),
                test_text("2"),
                test_text("\u{2014}"),
                test_text("3"),
                test_text("\u{2014}"),
                test_text("4"),
                test_text("---"),
                test_text("5"),
                test_text("-"),
                test_text("6"),
                test_text("-"),
                test_text("7"),
            ]])])),
        )
    }
}

mod code {
    use std::ffi::CString;

    use crate::error::UserPythonCompileMode;

    use super::*;

    // TODO test dashes work, errors reported use the correct open/close?
    // TODO what happens with empty code

    #[test]
    fn simple_eval_works() {
        expect_parse(
            "[5] ['string']
            
            [TEST_BLOCK]",
            Ok(test_doc(vec![
                TestBlock::Paragraph(vec![vec![
                    test_text("5"),
                    test_text(" "),
                    test_text("string"),
                ]]),
                TestBlock::TestOwnedBlock(vec![]),
            ])),
        )
    }
    #[test]
    fn multiline_eval_works() {
        expect_parse(
            "There are [len((
            1,
            2,
            3,
            4,
            5,
            6
        ))] elements",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("There are "),
                test_text("6"),
                test_text(" elements"),
            ]])])),
        )
    }
    #[test]
    fn raise_in_eval_mode_works() {
        // Raise a SyntaxError, which a broken compiler might pick up as a compile error, and make sure it's thrown as a runtime error.
        expect_parse_err(
            "
[
def test():
    raise SyntaxError()
]
        
        [test()]",
            TestUserPythonExecError::RunningEvalBrackets {
                code_ctx: TestParseContext("[", "test()", "]"),
                code: CString::new("test()").unwrap(),
                mode: UserPythonCompileMode::EvalExpr,
                err: Regex::new(r"^SyntaxError\s*:\s*None$").unwrap(),
            },
        )
    }
    // Code should be trimmed of whitespace on both sides
    #[test]
    fn code_trimmed_in_eval_mode() {
        expect_parse(
            "[len((1,2,3))      ]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[     len((1,2,3))]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[     len((1,2,3))      ]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );

        // And with the dashes
        expect_parse(
            "[---len((1,2,3))      ---]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[---     len((1,2,3))---]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[---     len((1,2,3))      ---]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse_err(
            "[     1invalid     ]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext("[", "     1invalid     ", "]"),
                // The actual code shouldn't be indented
                code: CString::new("1invalid").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        )
    }
    #[test]
    fn syntax_error_in_eval_mode_passes_through() {
        // The assignment support depends on trying to eval() the expression, that failing with a SyntaxError, and then trying to exec() it (and then doing more things if there are indent errors!)
        // Make sure that something invalid as both still returns a SyntaxError
        expect_parse_err(
            "[1invalid]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext("[", "1invalid", "]"),
                code: CString::new("1invalid").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        )
    }

    #[test]
    fn simple_exec_works() {
        expect_parse(
            "[x = 5]
        
        [x]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "5",
            )])])),
        )
    }
    #[test]
    fn multiline_exec_works() {
        expect_parse(
            "
            [l = len((
                1,
                2,
                3,
                4,
                5,
                6
            ))]
            
            There are [l] elements",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("There are "),
                test_text("6"),
                test_text(" elements"),
            ]])])),
        );
        // Even this counts as a multiline non-indented exec because whitespace is trimmed.
        // A second statement would be necessary to raise IndentationError when compiled for exec mode,
        // and thus to pick up the indent guard.
        // However, comments would force compiling in indented-exec mode because the trim algorithm doesn't account for them.
        expect_parse(
            "
            [
                l = len((
                    1,
                    2,
                    3,
                    4,
                    5,
                    6
                ))
            ]
            
            There are [l] elements",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("There are "),
                test_text("6"),
                test_text(" elements"),
            ]])])),
        );
    }
    #[test]
    fn raise_in_exec_mode_works() {
        // Raise a SyntaxError, which a broken compiler might pick up as a compile error, and make sure it's thrown as a runtime error.
        expect_parse_err(
            "
[
def test():
    raise SyntaxError()
]
        
        [x = test()]",
            TestUserPythonExecError::RunningEvalBrackets {
                code_ctx: TestParseContext("[", "x = test()", "]"),
                code: CString::new("x = test()").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*None$").unwrap(),
            },
        )
    }
    #[test]
    fn code_trimmed_in_exec_mode() {
        expect_parse(
            "[l = len((1,2,3))      ][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[     l = len((1,2,3))][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[     l = len((1,2,3))      ][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );

        // And with the dashes
        expect_parse(
            "[---l = len((1,2,3))      ---][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[---     l = len((1,2,3))---][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse(
            "[---     l = len((1,2,3))      ---][l]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
                "3",
            )])])),
        );
        expect_parse_err(
            "[     l = 1invalid     ][l]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext("[", "     l = 1invalid     ", "]"),
                // The actual code shouldn't be indented
                code: CString::new("l = 1invalid").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        )
    }
    #[test]
    fn syntax_error_in_exec_mode_passes_through() {
        // The assignment support depends on trying to eval() the expression, that failing with a SyntaxError, and then trying to exec() it (and then doing more things if there are indent errors!)
        // Make sure that something invalid as both still returns a SyntaxError
        expect_parse_err(
            "[x = 1invalid]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext("[", "x = 1invalid", "]"),
                code: CString::new("x = 1invalid").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        );
        // Even this counts as a multiline non-indented exec because whitespace is trimmed.
        // A second statement would be necessary to raise IndentationError when compiled for exec mode,
        // and thus to pick up the indent guard.
        // However, comments would force compiling in indented-exec mode because the trim algorithm doesn't account for them.
        expect_parse_err(
            "[
                 x = 1invalid
            ]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext(
                    "[",
                    "\n                 x = 1invalid\n            ",
                    "]",
                ),
                code: CString::new("x = 1invalid").unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        );
    }

    #[test]
    fn indented_exec_works() {
        expect_parse(
            "
        
        [
            indented_x = 5
            indented_y = 10
        ]
        
        [indented_x] [indented_y]",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("5"),
                test_text(" "),
                test_text("10"),
            ]])])),
        )
    }
    #[test]
    fn indented_multiline_exec_works() {
        expect_parse(
            "
            [
              # wow stuff that means there's still an indent after trimming
                l = len((
                    1,
                    2,
                    3,
                    4,
                    5,
                    6
                ))
                second_statement_at_nonzero_indent_level_to_force_indent_compile = 0
            ]
            
            There are [l] elements",
            Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
                test_text("There are "),
                test_text("6"),
                test_text(" elements"),
            ]])])),
        );
    }
    #[test]
    fn raise_in_indented_exec_mode_works() {
        // Raise a SyntaxError, which a broken compiler might pick up as a compile error, and make sure it's thrown as a runtime error.
        expect_parse_err(
            "
[
def test():
    raise SyntaxError()
]
        
        [
            # force indent-mode
            x = test()
        ]",
            TestUserPythonExecError::RunningEvalBrackets {
                code_ctx: TestParseContext(
                    "[",
                    "\n            # force indent-mode\n            x = test()\n        ",
                    "]",
                ),
                // Code has had if True: appended, and was not trimmed
                code: CString::new(
                    "if True:\n\n            # force indent-mode\n            x = test()\n        ",
                )
                .unwrap(),
                mode: UserPythonCompileMode::ExecIndentedStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*None$").unwrap(),
            },
        )
    }
    // If indentation is truly broken, we have to expose it to the user. Best to expose the
    #[test]
    fn indent_errors_in_indented_exec_mode_pass_through() {
        expect_parse_err(
            "[
            good_indent = 0
        unindent = 1
            suddenly_bad_indent = 2
        ]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext(
                    "[",
                    "\n            good_indent = 0\n        unindent = 1\n            \
                     suddenly_bad_indent = 2\n        ",
                    "]",
                ),
                code: CString::new(
                    "if True:\n\n            good_indent = 0\n        unindent = 1\n            \
                     suddenly_bad_indent = 2\n        ",
                )
                .unwrap(),
                mode: UserPythonCompileMode::ExecIndentedStmts,
                err: Regex::new(r"^IndentationError").unwrap(),
            },
        )
    }
    // Indented exec mode should only be triggered by indentation.
    #[test]
    fn non_indent_errors_dont_trigger_indented_exec_mode() {
        // This example actually does trigger indented-exec-mode. I think it's because i'm using a comment as the first line, and I'm pretty sure that makes it parser-specific - introducing two potential errors means I can't predict which one will be thrown first => it's a tossup if IndentationError is thrown first and we go into indented-exec, or if SyntaxError is thrown first and we don't.
        // expect_parse_err(
        //     "[
        //         # commment to prevent trim from eliminating indent
        //         x = 1invalid
        //     ]",
        //     TestUserPythonExecError::CompilingEvalBrackets {
        //         code_ctx: TestParseContext("[", "\n                # commment to prevent trim from eliminating indent\n                x = 1invalid\n            ", "]"),
        //         code: CString::new("# commment to prevent trim from eliminating indent\n                x = 1invalid").unwrap(),
        //         mode: UserPythonCompileMode::ExecIndentedStmts,
        //         err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
        //     },
        // );
        expect_parse_err(
            "[
                x = 1invalid
                second_statement_at_nonzero_indent_level_to_force_indent_compile = 0
            ]",
            TestUserPythonExecError::CompilingEvalBrackets {
                code_ctx: TestParseContext(
                    "[",
                    "\n                x = 1invalid\n                \
                     second_statement_at_nonzero_indent_level_to_force_indent_compile = 0\n            ",
                    "]",
                ),
                code: CString::new(
                    "x = 1invalid\n                \
                     second_statement_at_nonzero_indent_level_to_force_indent_compile = 0",
                )
                .unwrap(),
                mode: UserPythonCompileMode::ExecStmts,
                err: Regex::new(r"^SyntaxError\s*:\s*invalid syntax").unwrap(),
            },
        );
    }
}
