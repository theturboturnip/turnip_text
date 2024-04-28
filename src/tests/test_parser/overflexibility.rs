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
