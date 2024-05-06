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
        "[BUILD_CUSTOM_BLOCK]{\n}",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![])])),
    )
}

#[test]
fn code_block_scope_opened_with_whitespaces_then_newline() {
    expect_parse(
        "[BUILD_CUSTOM_BLOCK]{       \n}",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![])])),
    )
}

#[test]
fn code_block_scope_opened_with_whitespaces_then_comment_then_newline() {
    expect_parse(
        "[BUILD_CUSTOM_BLOCK]{       # wowie a comment!\n}",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![])])),
    )
}

#[test]
fn code_block_scope_opened_with_comment() {
    expect_parse(
        "[BUILD_CUSTOM_BLOCK]{# wowie a comment\n}",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![])])),
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
        "[BUILD_CUSTOM_INLINE]{inline}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![test_text("inline")]),
        ]])])),
    )
}

#[test]
fn code_inline_scope_opened_with_whitespaces_then_content() {
    expect_parse(
        "[BUILD_CUSTOM_INLINE]{       inline      }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![test_text("inline")]),
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
        "[BUILD_CUSTOM_INLINE]{}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![]),
        ]])])),
    )
}

// EOFs inside block and inline scopes should both fail equally

#[test]
fn eof_in_inline_scope() {
    expect_parse_err(
        "{ wow some data and then EOF",
        TestSyntaxError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}

#[test]
fn eof_in_block_scope() {
    expect_parse_err(
        "{   \n wow some data and then EOF",
        TestSyntaxError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    )
}
