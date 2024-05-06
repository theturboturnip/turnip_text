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
        [BUILD_CUSTOM_BLOCK]{
            [test_src("some valid stuff in an inner file")]
        }"#,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![test_sentence("some valid stuff in an inner file")]),
        ])])),
    )
}

// Can't insert a file in any inline mode
#[test]
fn test_no_inserted_file_in_paragraph() {
    expect_parse_err(
        r#"wow i'm inside a paragraph! [test_src("some more data O.O")]"#,
        TestSyntaxError::CodeEmittedSourceInInlineMode {
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
        TestSyntaxError::CodeEmittedSourceInInlineMode {
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
        r#"[BUILD_CUSTOM_INLINE]{wow i'm inside an inline scope builder! [test_src("some more data O.O")] }"#,
        TestSyntaxError::CodeEmittedSourceInInlineMode {
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
        TestSyntaxError::BlockScopeCloseOutsideScope(TestParseSpan("}")),
    )
}
