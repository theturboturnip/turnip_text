use std::ffi::CString;

use super::*;

// The lexer test ensures that dashes inside eval-brackets are tokenized as part of CodeOpen and CodeClose correctly.

#[test]
fn code_open_close_tokens_reported_correctly() {
    // Test they work in compilation
    expect_parse_err(
        "[-- 1invalid --]",
        TestUserPythonError::CompilingEvalBrackets {
            code_ctx: TestParseContext("[--", " 1invalid ", "--]"),
            code: CString::new("1invalid").unwrap(),
            mode: UserPythonCompileMode::ExecStmts,
            err: Regex::new("SyntaxError").unwrap(),
        },
    );
    // Test they work when error is in running the code
    expect_parse_err(
        "
[---
def x():
    raise ValueError()

x()
---]
    ",
        TestUserPythonError::RunningEvalBrackets {
            code_ctx: TestParseContext(
                "[---",
                "\ndef x():\n    raise ValueError()\n\nx()\n",
                "---]",
            ),
            code: CString::new("def x():\n    raise ValueError()\n\nx()").unwrap(),
            mode: UserPythonCompileMode::ExecStmts,
            err: Regex::new("ValueError").unwrap(),
        },
    );
    expect_parse_err(
        "
[---
def x():
    raise ValueError()
---]

    [----x()----]
    ",
        TestUserPythonError::RunningEvalBrackets {
            code_ctx: TestParseContext("[----", "x()", "----]"),
            code: CString::new("x()").unwrap(),
            mode: UserPythonCompileMode::EvalExpr,
            err: Regex::new("ValueError").unwrap(),
        },
    );
    // Test they work when emitting things into the doc
    expect_parse_err(
        "[-----b'bytestirng not allowed'-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "b'bytestirng not allowed'", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        "[-----b'not a scope owner'-----]{
            stuff in a block scope
        }",
        TestUserPythonError::CoercingEvalBracketToBuilder {
            code_ctx: TestParseContext("[-----", "b'not a scope owner'", "-----]"),
            err: Regex::new("TypeError.*instance of BlocksBuilder.*build_from_blocks").unwrap(),
            scope_open: TestParseSpan("{"),
            build_mode: UserPythonBuildMode::FromBlock,
        },
    );
    expect_parse_err(
        "[-----b'not a scope owner'-----]{ stuff in a inline scope }",
        TestUserPythonError::CoercingEvalBracketToBuilder {
            code_ctx: TestParseContext("[-----", "b'not a scope owner'", "-----]"),
            err: Regex::new("TypeError.*instance of InlinesBuilder.*build_from_inlines").unwrap(),
            scope_open: TestParseSpan("{"),
            build_mode: UserPythonBuildMode::FromInline,
        },
    );
    expect_parse_err(
        "[-----b'not a scope owner'-----]###{ stuff in a raw scope }###",
        TestUserPythonError::CoercingEvalBracketToBuilder {
            code_ctx: TestParseContext("[-----", "b'not a scope owner'", "-----]"),
            err: Regex::new("TypeError.*instance of RawScopeBuilder.*build_from_raw").unwrap(),
            scope_open: TestParseSpan("###{"),
            build_mode: UserPythonBuildMode::FromRaw,
        },
    );
}

#[test]
fn empty_code_is_none() {
    expect_parse("[]", Ok(test_doc(vec![])));
    expect_parse("[-  -]", Ok(test_doc(vec![])));
    expect_parse("[--     --]", Ok(test_doc(vec![])));
    expect_parse("[---       ---]", Ok(test_doc(vec![])));
}

#[test]
fn simple_eval_works() {
    expect_parse(
        "[5] ['string']
            
            [CUSTOM_BLOCK]",
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![vec![
                test_text("5"),
                test_text(" "),
                test_text("string"),
            ]]),
            TestBlock::CustomBlock(vec![]),
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
        TestUserPythonError::RunningEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
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
        TestUserPythonError::RunningEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
            code_ctx: TestParseContext("[", "\n                 x = 1invalid\n            ", "]"),
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
        TestUserPythonError::RunningEvalBrackets {
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
        TestUserPythonError::CompilingEvalBrackets {
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
            TestUserPythonError::CompilingEvalBrackets {
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

// Code is must only be able to emit:
// - None
// - instance of TurnipTextSource
// - object fitting Header
// - object fitting Block
// - object fitting Inline, or coercible to inline
//
// It must not be able to emit something that isn't any of those,
// and it must not be able to emit something that is multiple of those.
// coercible-to-inline cannot directly fit inline, turniptextsources and none cannot fit anything,
// so we have to test emitting something that fits header+block+inline

#[test]
fn code_returns_uncoercible_when_emitting_uncoercible_that_fits_none() {
    expect_parse_err(
        "[-----b'bytestirng not coercible'-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "b'bytestirng not coercible'", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        "[-----{}-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "{}", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        "[-----set()-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "set()", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        "[-----object()-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "object()", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
}

#[test]
fn code_returns_uncoercible_when_emitting_uncoercible_that_fits_many() {
    expect_parse_err(
        "[-
        class FitMultiple:
            is_block = True
            is_inline = True
            is_header = True
            weight = 0
        -]
        
        [-----FitMultiple()-----]",
        TestUserPythonError::CoercingEvalBracketToElement {
            code_ctx: TestParseContext("[-----", "FitMultiple()", "-----]"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
}

// User Python code can throw errors in various places - when it's initially run, and when the result is built.
// I am ignoring errors thrown while evaluating typeclasses e.g. if getting Header.weight raises an error, that's out of scope.
#[test]
fn errors_while_running_are_returned() {
    expect_parse_err(
        r#"
    [-
    def function_that_throws():
        raise RuntimeError("Should be raised through TTUserPythonError::RunningEvalBrackets")
    -]

    [function_that_throws()]
    "#,
        TestUserPythonError::RunningEvalBrackets {
            code_ctx: TestParseContext("[", "function_that_throws()", "]"),
            code: CString::new("function_that_throws()").unwrap(),
            mode: UserPythonCompileMode::EvalExpr,
            err: Regex::new(
                "RuntimeError.*Should be raised through TTUserPythonError::RunningEvalBrackets",
            )
            .unwrap(),
        },
    )
}

#[test]
fn errors_while_building_are_returned() {
    expect_parse_err(
        r#"
    [-
    class BuilderThatThrows:
        def build_from_blocks(self, blocks):
            raise RuntimeError("Should be raised through TTUserPythonError::Building")
    -]

    [BuilderThatThrows()]{
        content
    }
    "#,
        TestUserPythonError::Building {
            code_ctx: TestParseContext("[", "BuilderThatThrows()", "]"),
            arg_ctx: TestParseContext("{", "\n        content\n    ", "}"),
            build_mode: UserPythonBuildMode::FromBlock,
            err: Regex::new("RuntimeError.*Should be raised through TTUserPythonError::Building")
                .unwrap(),
        },
    )
}
