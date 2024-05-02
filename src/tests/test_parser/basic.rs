use std::ffi::CString;

use crate::error::UserPythonCompileMode;

use super::*;

#[test]
fn test_basic_text() {
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
fn test_inline_code() {
    expect_parse(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_code_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [- len((1,2,3)) -]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_code_with_long_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [---- len((1,2,3)) ----]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): \[\- len((1,2,3)) \-\]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"Number of values in (1,2,3): [- len((1,2,3)) -]"#,
        )])])),
    )
}

#[test]
fn test_inline_list_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [- len([1,2,3]) -]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_block_scope() {
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
fn test_raw_scope() {
    expect_parse(
        "#{It's f&%#ing raw}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_raw_text("It's f&%#ing raw"),
        ]])])),
    )
}

#[test]
fn test_inline_scope() {
    expect_parse(
        r#"Outside the scope {inside the scope}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            TestInline::InlineScope(vec![test_text("inside the scope")]),
        ]])])),
    )
}

#[test]
fn test_inline_escaped_scope() {
    expect_parse(
        r#"Outside the scope \{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope {not inside a scope}",
        )])])),
    )
}

#[test]
fn test_raw_scope_newlines() {
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
fn test_raw_scope_crlf_newlines() {
    expect_parse(
        "Outside the scope #{\r\ninside the raw scope\r\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
fn test_inline_raw_scope() {
    expect_parse(
        r#"Outside the scope #{inside the raw scope}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("inside the raw scope"),
        ]])])),
    )
}

#[test]
fn test_owned_block_scope() {
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
fn test_owned_block_scope_with_non_block_builder() {
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
fn test_owned_inline_scope() {
    expect_parse(
        r"[TEST_INLINE_BUILDER]{special text}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedInline(vec![test_text("special text")]),
        ]])])),
    )
}

#[test]
fn test_owned_inline_scope_with_non_inline_builder() {
    expect_parse_err(
        r"[None]{special text}",
        TestUserPythonExecError::CoercingInlineScopeBuilder {
            code_ctx: TestParseContext("[", "None", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*InlineScopeBuilder.*Got None.*").unwrap(),
        },
    )
}

#[test]
fn test_owned_inline_raw_scope_with_newline() {
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
fn test_owned_inline_raw_scope_with_non_raw_builder() {
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
fn test_inline_raw_escaped_scope() {
    expect_parse(
        r#"Outside the scope \#\{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope #{not inside a scope}",
        )])])),
    )
}

#[test]
fn test_plain_hashes() {
    expect_parse(
        r#"This has a string of ####### hashes in the middle"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("This has a string of"), // The first hash in the chain starts a comment, and trailing whitespace is ignored
        ])])),
    )
}

#[test]
fn test_comments() {
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
fn test_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash! \\#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            r#"About to see a backslash! \"#,
        )]])])),
    )
}

#[test]
fn test_escaped_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#,
        )])])),
    )
}

#[test]
fn test_escaped_notspecial() {
    expect_parse(
        r#"\a"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
fn test_escaped_newline() {
    expect_parse(
        r#"escaped \
newline"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "escaped newline",
        )])])),
    )
}

#[test]
fn test_newline_in_code() {
    expect_parse(
        "[len((1,\r\n2))]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}
#[test]
fn test_code_close_in_text() {
    expect_parse_err(
        "not code ] but closed code",
        TestInterpError::CodeCloseOutsideCode(TestParseSpan("]")),
    )
}
#[test]
fn test_inline_scope_close_outside_scope() {
    expect_parse_err(
        "not in a scope } but closed scope",
        TestInterpError::InlineScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
#[test]
fn test_block_scope_close_outside_scope() {
    expect_parse_err(
        "} # not in a scope",
        TestInterpError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
// Scope closes at the start of a line directly after a paragraph are treated differently
// We assume you couldn't possibly be closing an inline scope! There can't be any to close!
// So you must be trying to close a block-level scope...
#[test]
fn test_block_scope_close_outside_scope_after_para() {
    expect_parse_err(
        "wow some content\nthat could imply the next scope close is in a paragraph i.e. inline \
         mode\n} # not in a scope",
        TestInterpError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
#[test]
fn test_raw_scope_close_outside_scope() {
    expect_parse_err(
        "text in a scope with a mismatched }### # comment",
        TestInterpError::RawScopeCloseOutsideRawScope(TestParseSpan("}###")),
    )
}
#[test]
fn test_mismatching_raw_scope_close() {
    expect_parse_err(
        "##{ text in a scope with a }#",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParseSpan("##{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
fn test_ended_inside_code() {
    expect_parse_err(
        "text [code",
        TestInterpError::EndedInsideCode {
            code_start: TestParseSpan("["),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
fn test_ended_inside_raw_scope() {
    expect_parse_err(
        "text #{raw",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParseSpan("#{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
fn test_ended_inside_scope() {
    expect_parse_err(
        "text {scope",
        TestInterpError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
fn test_newline_inside_inline_scope() {
    expect_parse_err(
        "text {scope\n",
        TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParseSpan("{"),
            sentence_break: TestParseSpan("\n"),
        },
    )
}
#[test]
fn test_block_scope_open_inline_para() {
    expect_parse_err(
        "text {\n",
        TestInterpError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext("text", "", " ")),
            block_scope_open: TestParseSpan("{"),
        },
    )
}
#[test]
fn test_block_scope_open_inline_multiline_para() {
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
fn test_block_scope_open_inline() {
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
fn test_eof_inside_block_scope() {
    expect_parse_err(
        "{\n",
        TestInterpError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}
#[test]
fn test_eof_inside_para_inside_block_scope() {
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
fn test_block_scope_vs_inline_scope() {
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
fn test_strip_leading_whitespace() {
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
fn test_strip_trailing_whitespace() {
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
fn test_strip_leading_scope_whitespace() {
    expect_parse(
        "{ no leading whitespace}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no leading whitespace")]),
        ]])])),
    )
}

#[test]
fn test_strip_trailing_scope_whitespace() {
    expect_parse(
        "{no trailing whitespace }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no trailing whitespace")]),
        ]])])),
    )
}

#[test]
fn test_dont_strip_whitespace_between_scopes() {
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
fn test_strip_whitespace_after_scope() {
    expect_parse(
        "{ stuff }     \n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
    )
}

#[test]
fn test_strip_whitespace_between_scope_end_and_comment() {
    expect_parse(
        "{ stuff }     # stuff in a comment!\n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
    )
}

#[test]
fn test_strip_trailing_whitespace_before_comment() {
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
fn test_not_strip_trailing_whitespace_before_escaped_newline() {
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
fn test_emit_block_from_code() {
    expect_parse(
        "[TEST_BLOCK]",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
fn test_cant_emit_block_from_code_inside_paragraph() {
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

#[test]
fn test_raw_scope_emitting_block_from_block_level() {
    expect_parse(
        "[TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
fn test_raw_scope_emitting_inline_from_block_level() {
    expect_parse(
        "[TEST_RAW_INLINE_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw("some raw stuff that goes in a block!".into()),
        ]])])),
    )
}

#[test]
fn test_raw_scope_cant_emit_block_inside_paragraph() {
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
fn test_raw_scope_emitting_inline_inside_paragraph() {
    expect_parse(
        "Inside a paragraph, you can [TEST_RAW_INLINE_BUILDER]#{insert an inline raw!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Inside a paragraph, you can "),
            TestInline::TestOwnedRaw("insert an inline raw!".into()),
        ]])])),
    )
}

#[test]
fn test_emitting_none_at_block() {
    expect_parse(
        "
[None]
",
        Ok(test_doc(vec![])),
    )
}

#[test]
fn test_emitting_none_inline() {
    expect_parse(
        "Check it out, there's [None]!",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Check it out, there's "),
            test_text("!"),
        ]])])),
    )
}

#[test]
fn test_assign_and_recall() {
    expect_parse(
        "[x = 5]

[x]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "5",
        )]])])),
    )
}

#[test]
fn test_emit_none() {
    expect_parse("[None]", Ok(test_doc(vec![])))
}

#[test]
fn test_cant_eval_none_for_block_builder() {
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
fn test_cant_assign_for_block_builder() {
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
fn test_cant_assign_for_raw_builder() {
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
fn test_cant_assign_for_inline_builder() {
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
fn test_syntax_errs_passed_thru() {
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
fn test_block_scope_builder_return_none() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed
}",
        Ok(test_doc(vec![])),
    )
}

#[test]
fn test_block_scope_builder_return_none_with_end_inside_para() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed
}",
        Ok(test_doc(vec![])),
    )
}
