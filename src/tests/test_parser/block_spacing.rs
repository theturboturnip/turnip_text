use super::*;

// These are tests for strict blank-line syntax checking - where the parser ensures that there is always a blank line between two blocks.

// TODO check separation of headers - they're also block-level elements

// There should always be a blank line between a paragraph ending and a paragraph starting
// (otherwise they'd be the same paragraph)
#[test]
fn test_block_sep_para_para() {
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
fn test_block_sep_para_scope_open() {
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
fn test_block_sep_para_code() {
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
fn test_block_sep_code_para() {
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
fn test_block_sep_para_scope_close() {
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
fn test_block_sep_scope_scope() {
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
            last_block: TestBlockModeElem::BlockScope(TestParseContext("{", "\n            ", "}")),
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
            last_block: TestBlockModeElem::BlockScope(TestParseContext("{", "\n            ", "}")),
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
fn test_block_sep_scope_code() {
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
            last_block: TestBlockModeElem::BlockScope(TestParseContext("{", "\n            ", "}")),
            next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
        },
    );
}

// There should always be a blank line between a code-emitting-block and a scope opening
#[test]
fn test_block_sep_code_scope() {
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
fn test_block_sep_code_code() {
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
fn test_inserted_file_newlines_dont_leak_out() {
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
fn test_block_sep_para_inserted_file() {
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
fn test_block_sep_inserted_file_para() {
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
fn test_block_sep_inserted_file_inserted_file() {
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
fn test_block_sep_inserted_file_block_code() {
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
fn test_block_sep_inserted_file_inline_code() {
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
fn test_block_sep_inserted_file_block_scope() {
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
fn test_block_sep_inserted_file_inline_scope() {
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
