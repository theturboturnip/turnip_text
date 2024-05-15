//! These are tests for strict blank-line syntax checking - where the parser ensures that there is always a blank line between two block-level elements.
//! There are five block-level elements:
//! - Paragraph
//! - Block scope
//! - Block emitted from code (which could have used a BlockScopeBuilder, InlineScopeBuilder, or RawBuilder)
//! - Header emitted from code (which could have used a BlockScopeBuilder, InlineScopeBuilder, or RawBuilder)
//! - TurnipTextSource emitted from code
//!
//! When one is emitted we know exactly what kind it is, but we may not get all the way through parsing the next one.
//! Thus we have fewer options for what the next one should be:
//! - Scope-open (which is ambiguously block or inline when opened from block-mode)
//! - Code-open
//! - Text content (including raw scopes)

use std::vec;

use const_format::concatcp;

use super::*;

const CREATED_PARA: &str = "Bee movie script\nLorem ipsum\n";
const CREATED_PARA_CTX: TestParseContext =
    TestParseContext("Bee", " movie script\nLorem ipsum", "\n");

const PARA_STARTING_WITH_SCOPE: &str =
    "{ wow some stuff in an inline scope }\nand then more of the Bee movie script\n";
const PARA_STARTING_WITH_INLINE_CODE: &str = "[--CUSTOM_INLINE--] Bee movie script\nlorem\n";
const PARA_STARTING_WITH_RAW_SCOPE: &str =
    "####{raw stuff}#### and then even more about bees\nbees, i tell you!\n";

const CREATED_BSCOPE: &str = "{
    block_scope_content
}";
const CREATED_BSCOPE_CTX: TestParseContext =
    TestParseContext("{", "\n    block_scope_content\n", "}");

const CREATED_BLOCK_BARE: &str = "[CUSTOM_BLOCK]";
const CREATED_BLOCK_BARE_SPAN: TestParseSpan = TestParseSpan(CREATED_BLOCK_BARE);
const CREATED_BLOCK_FROM_BLOCK: &str = "[CustomBlockBuilder()]{\nblock_in_block\n}";
const CREATED_BLOCK_FROM_BLOCK_SPAN: TestParseSpan = TestParseSpan(CREATED_BLOCK_FROM_BLOCK);
const CREATED_BLOCK_FROM_INLINE: &str = "[CustomBlockBuilderFromInline()]{ inline_in_block }";
const CREATED_BLOCK_FROM_INLINE_SPAN: TestParseSpan = TestParseSpan(CREATED_BLOCK_FROM_INLINE);
const CREATED_BLOCK_FROM_RAW: &str = "[CustomBlockBuilderFromRaw()]#{raw_in_block}#";
const CREATED_BLOCK_FROM_RAW_SPAN: TestParseSpan = TestParseSpan(CREATED_BLOCK_FROM_RAW);

const CREATED_HEADER_BARE: &str = "[CustomHeader(weight=1)]";
const CREATED_HEADER_BARE_SPAN: TestParseSpan = TestParseSpan(CREATED_HEADER_BARE);
const CREATED_HEADER_FROM_BLOCK: &str = "[CustomHeaderBuilder(weight=1)]{\nblock_in_header\n}";
const CREATED_HEADER_FROM_BLOCK_SPAN: TestParseSpan = TestParseSpan(CREATED_HEADER_FROM_BLOCK);
const CREATED_HEADER_FROM_INLINE: &str = "[CustomHeaderBuilder(weight=1)]{ inline_in_header }";
const CREATED_HEADER_FROM_INLINE_SPAN: TestParseSpan = TestParseSpan(CREATED_HEADER_FROM_INLINE);
const CREATED_HEADER_FROM_RAW: &str = "[CustomHeaderBuilder(weight=1)]#{raw_in_header}#";
const CREATED_HEADER_FROM_RAW_SPAN: TestParseSpan = TestParseSpan(CREATED_HEADER_FROM_RAW);

const CREATED_FILE: &str = "[test_src('beans')]";
const CREATED_FILE_SPAN: TestParseSpan = TestParseSpan(CREATED_FILE);

#[test]
fn test_primitives() {
    expect_parse(
        CREATED_PARA,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Bee movie script"),
            test_sentence("Lorem ipsum"),
        ])])),
    );
    expect_parse(
        CREATED_BSCOPE,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "block_scope_content",
        )])])),
    );
    expect_parse(
        CREATED_BLOCK_BARE,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![])])),
    );
    expect_parse(
        CREATED_BLOCK_FROM_BLOCK,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![test_sentence("block_in_block")]),
        ])])),
    );
    // Note - things inside CustomBlock don't get collapsed! In most other places
    expect_parse(
        CREATED_BLOCK_FROM_INLINE,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![test_text(
                "inline_in_block",
            )])]]),
        ])])),
    );
    expect_parse(
        CREATED_BLOCK_FROM_RAW,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![vec![test_raw_text("raw_in_block")]]),
        ])])),
    );

    expect_parse(
        CREATED_HEADER_BARE,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (1, None, None),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    );
    expect_parse(
        CREATED_HEADER_FROM_BLOCK,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    1,
                    Some(TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("block_in_header"),
                    ])])),
                    None,
                ),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    );
    expect_parse(
        CREATED_HEADER_FROM_INLINE,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    1,
                    None,
                    Some(TestInline::InlineScope(vec![test_text("inline_in_header")])),
                ),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    );
    expect_parse(
        CREATED_HEADER_FROM_RAW,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    1,
                    None,
                    Some(TestInline::InlineScope(vec![test_raw_text(
                        "raw_in_header",
                    )])),
                ),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    );

    expect_parse(
        CREATED_FILE,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "beans",
        )])])),
    )
}

/// This generates test cases for any block-mode-element that
/// - must be followed directly by a newline before any other element
/// - don't parse the following element at all if the newline rule is violated
///
/// This makes it unsuitable for paragraphs, because in the case of code-emitting-blah
/// right after a paragraph the parser will parse the code because it may be emitting an inline.
macro_rules! test_needs_newline {
    ( $last_block:expr, $last_block_elem:expr) => {
        #[test]
        fn to_para_newline() {
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_PARA));
            expect_parse_any_ok(concatcp!($last_block, "\n", PARA_STARTING_WITH_SCOPE));
            expect_parse_any_ok(concatcp!($last_block, "\n", PARA_STARTING_WITH_INLINE_CODE));
            expect_parse_any_ok(concatcp!($last_block, "\n", PARA_STARTING_WITH_RAW_SCOPE));

            expect_parse_err(
                concatcp!($last_block, " ", CREATED_PARA),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan(
                        CREATED_PARA_CTX.0,
                    )),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", PARA_STARTING_WITH_SCOPE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", PARA_STARTING_WITH_INLINE_CODE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[--")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", PARA_STARTING_WITH_RAW_SCOPE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("####{")),
                },
            );
        }

        #[test]
        fn to_block_scope_newline() {
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_BSCOPE));
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_BSCOPE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan(
                        CREATED_BSCOPE_CTX.0,
                    )),
                },
            );
        }

        #[test]
        fn to_code_emitting_block() {
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_BLOCK_BARE));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_BLOCK_FROM_BLOCK));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_BLOCK_FROM_INLINE));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_BLOCK_FROM_RAW));

            expect_parse_err(
                concatcp!($last_block, " ", CREATED_BLOCK_BARE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_BLOCK_FROM_BLOCK),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_BLOCK_FROM_INLINE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_BLOCK_FROM_RAW),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
        }

        #[test]
        fn to_code_emitting_header() {
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_HEADER_BARE));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_HEADER_FROM_BLOCK));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_HEADER_FROM_INLINE));
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_HEADER_FROM_RAW));

            expect_parse_err(
                concatcp!($last_block, " ", CREATED_HEADER_BARE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_HEADER_FROM_BLOCK),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_HEADER_FROM_INLINE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_HEADER_FROM_RAW),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
        }

        #[test]
        fn to_source_blank_line() {
            expect_parse_any_ok(concatcp!($last_block, "\n", CREATED_FILE));
            expect_parse_err(
                concatcp!($last_block, " ", CREATED_FILE),
                TestSyntaxError::InsufficientBlockSeparation {
                    last_block: $last_block_elem,
                    next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("[")),
                },
            );
        }
    };
}

// -------------------------------------
// Paragraphs
// -------------------------------------
// Paragraphs have special interactions with scopes,
// and need blank lines between themselves and other block-level elements
// because a fully blank line is needed to end the paragraph.
// This does not apply to end-scopes for readability.
mod para {
    use super::*;

    /// There should always be a blank line between a paragraph ending and a paragraph starting
    /// (otherwise they'd be the same paragraph)
    #[test]
    fn para_to_para_blank_line() {
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
    /// This seems inconsistent with block-scope-closing - see [para_to_scope_close_adj] - but it *is* consistent with code generating a block - see [para_to_block_code_blank_line] - and that's more important
    /// because in both the scope-open and code cases you're generating a new block.
    /// We want to avoid creating new blocks on adjacent lines to creating other blocks, because that implies they're "together" in some way.
    #[test]
    fn para_to_scope_open_blank_line() {
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_BSCOPE));
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_BSCOPE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan("{")),
            },
        );
    }

    /// There does not need to a blank line between a paragraph ending and closing an enclosing block scope
    /// This should *not* trigger insufficient space - it's fine to close a block scope directly after a paragraph
    #[test]
    fn para_to_scope_close_adj() {
        expect_parse_any_ok(
            r#"{
            Paragraph one
        }"#,
        );
    }

    /// There should always be a blank line between a paragraph ending and code-emitting-block
    #[test]
    fn para_to_block_code_blank_line() {
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_BLOCK_BARE));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_BLOCK_FROM_BLOCK));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_BLOCK_FROM_INLINE));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_BLOCK_FROM_RAW));

        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_BLOCK_BARE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_BLOCK_BARE_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_BLOCK_FROM_BLOCK),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_BLOCK_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_BLOCK_FROM_INLINE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_INLINE_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_BLOCK_FROM_RAW),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_RAW_SPAN),
            },
        );
    }

    /// There should always be a blank line between a paragraph ending and code-emitting-header
    #[test]
    fn para_to_header_code_blank_line() {
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_HEADER_BARE));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_HEADER_FROM_BLOCK));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_HEADER_FROM_INLINE));
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_HEADER_FROM_RAW));

        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_HEADER_BARE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_HEADER_BARE_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_HEADER_FROM_BLOCK),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_BLOCK_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_HEADER_FROM_INLINE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_INLINE_SPAN),
            },
        );
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_HEADER_FROM_RAW),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_RAW_SPAN),
            },
        );
    }

    /// There should always be a blank line between a paragraph ending and code-emitting-source
    #[test]
    fn para_to_source_blank_line() {
        expect_parse_any_ok(concatcp!(CREATED_PARA, "\n", CREATED_FILE));
        expect_parse_err(
            concatcp!(CREATED_PARA, CREATED_FILE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::Para(CREATED_PARA_CTX),
                next_block_start: TestBlockModeElem::SourceFromCode(CREATED_FILE_SPAN),
            },
        );
    }
}

// -------------------------------------
// Block Scopes
// -------------------------------------
// A newline (not a blank line) is required after the end of a block scope before any content can be emitted.
// This means a new block can be created in the very next line, but not on the same line.
mod block_scope {
    use super::*;

    test_needs_newline!(
        CREATED_BSCOPE,
        TestBlockModeElem::BlockScope(CREATED_BSCOPE_CTX)
    );
}

mod code_emitting_block {
    mod bare {
        use super::super::*;
        test_needs_newline!(
            CREATED_BLOCK_BARE,
            TestBlockModeElem::BlockFromCode(CREATED_BLOCK_BARE_SPAN)
        );
    }
    mod from_block {
        use super::super::*;
        test_needs_newline!(
            CREATED_BLOCK_FROM_BLOCK,
            TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_BLOCK_SPAN)
        );
    }
    mod from_inline {
        use super::super::*;
        test_needs_newline!(
            CREATED_BLOCK_FROM_INLINE,
            TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_INLINE_SPAN)
        );
    }
    mod from_raw {
        use super::super::*;
        test_needs_newline!(
            CREATED_BLOCK_FROM_RAW,
            TestBlockModeElem::BlockFromCode(CREATED_BLOCK_FROM_RAW_SPAN)
        );
    }
}

mod code_emitting_header {
    mod bare {
        use super::super::*;
        test_needs_newline!(
            CREATED_HEADER_BARE,
            TestBlockModeElem::BlockFromCode(CREATED_HEADER_BARE_SPAN)
        );
    }
    mod from_block {
        use super::super::*;
        test_needs_newline!(
            CREATED_HEADER_FROM_BLOCK,
            TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_BLOCK_SPAN)
        );
    }
    mod from_inline {
        use super::super::*;
        test_needs_newline!(
            CREATED_HEADER_FROM_INLINE,
            TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_INLINE_SPAN)
        );
    }
    mod from_raw {
        use super::super::*;
        test_needs_newline!(
            CREATED_HEADER_FROM_RAW,
            TestBlockModeElem::BlockFromCode(CREATED_HEADER_FROM_RAW_SPAN)
        );
    }
}

mod code_emitting_source {
    use super::*;

    test_needs_newline!(
        CREATED_FILE,
        TestBlockModeElem::SourceFromCode(CREATED_FILE_SPAN)
    );

    /// A special case: The parser handles TurnipTextSource differently to other eval-bracket outcomes,
    /// in that when the eval-brackets finish the TurnipTextSource is immediately emitted instead of checking
    /// the next token to see if an argument should be attached. If an argument *is* attached it can never be valid
    /// and should always fail. Right now that failure is counted as InsufficientBlockSeparation -
    /// there cannot be content between the end of an eval-bracket evaluating TurnipTextSource and the following newline.
    #[test]
    fn to_directly_following_scope() {
        expect_parse_err(
            concatcp!(CREATED_FILE, CREATED_BSCOPE),
            TestSyntaxError::InsufficientBlockSeparation {
                last_block: TestBlockModeElem::SourceFromCode(CREATED_FILE_SPAN),
                next_block_start: TestBlockModeElem::AnyToken(TestParseSpan(CREATED_BSCOPE_CTX.0)),
            },
        );
    }
}
