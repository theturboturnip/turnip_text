// All kinds of builder should be able to build Header | Block | Inline | None.

use super::*;

#[test]
fn test_inline_scope_builder_building_inline() {
    expect_parse(
        "building [BUILD_CUSTOM_INLINE]{something built} inline",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("building "),
            TestInline::CustomInline(vec![test_text("something built")]),
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

        [BUILD_CUSTOM_BLOCK]{
            blocks inside blocks! [BUILD_CUSTOM_INLINE]{ with otehr stuff in them! }
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
        "building [BUILD_CUSTOM_RAW]#{some raw stuff}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("building "),
            TestInline::CustomRaw("some raw stuff".to_string()),
        ]])])),
    )
}

// Make sure all the kinds of builder that emit inlines start an inline, and don't just create and close a paragraph

#[test]
fn test_inline_scope_builder_building_inline_creates_paragraph() {
    expect_parse(
        "[BUILD_CUSTOM_INLINE]{something built} inline",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![test_text("something built")]),
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

        [BUILD_CUSTOM_BLOCK]{
            blocks inside blocks! [BUILD_CUSTOM_INLINE]{ with otehr stuff in them! }
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
        "[BUILD_CUSTOM_RAW]#{some raw stuff}# and this continues",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomRaw("some raw stuff".to_string()),
            test_text(" and this continues"),
        ]])])),
    )
}

// All kinds of builder should be able to build blocks, even if their arguments aren't blocks

#[test]
fn test_inline_scope_builder_building_block() {
    expect_parse(
        "[BUILD_CUSTOM_BLOCK_FROM_INLINE]{only inlines :)}",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
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
        [BUILD_CUSTOM_BLOCK]{
            Stuff
        }
    "#,
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![test_sentence("Stuff")]),
        ])])),
    )
}

#[test]
fn test_raw_scope_builder_building_block() {
    expect_parse(
        "[BUILD_CUSTOM_BLOCK_FROM_RAW]#{ block! }#",
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![vec![test_raw_text(" block! ")]]),
        ])])),
    )
}

// Even if each builder can build blocks, they shouldn't be able to emit a block in an inline context

#[test]
fn test_inline_scope_builder_building_block_in_inline() {
    expect_parse_err(
        "{wow i'm in an inline context [BUILD_CUSTOM_BLOCK_FROM_INLINE]{only inlines :)}}",
        TestSyntaxError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            code_span: TestParseSpan("[BUILD_CUSTOM_BLOCK_FROM_INLINE]{only inlines :)}"),
        },
    )
}

#[test]
fn test_block_scope_builder_building_block_in_inline() {
    expect_parse_err(
        r#"
        {wow i'm in an inline context [BUILD_CUSTOM_BLOCK]{
            Stuff
        } continuing the inline context}
    "#,
        TestSyntaxError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            code_span: TestParseSpan("[BUILD_CUSTOM_BLOCK]{\n            Stuff\n        }"),
        },
    )
}

#[test]
fn test_raw_scope_builder_building_block_in_inline() {
    expect_parse_err(
        "{wow i'm in an inline context [BUILD_CUSTOM_BLOCK_FROM_RAW]#{ block! }# continuing the \
         inline context}",
        TestSyntaxError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            code_span: TestParseSpan("[BUILD_CUSTOM_BLOCK_FROM_RAW]#{ block! }#"),
        },
    )
}

// All kinds of builder should be able to build headers, even if their arguments aren't blocks
#[test]
fn test_inline_scope_builder_building_header() {
    expect_parse(
        "[CustomHeaderBuilder()]{ Wowee i wish I had inline content }",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    0,
                    None,
                    Some(TestInline::InlineScope(vec![test_text(
                        "Wowee i wish I had inline content",
                    )])),
                ),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    )
}

#[test]
fn test_block_scope_builder_building_header() {
    expect_parse(
        "[CustomHeaderBuilder()]{
        Wowee i wish I had block content
    }",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    0,
                    Some(TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("Wowee i wish I had block content"),
                    ])])),
                    None,
                ),
                contents: TestBlock::BlockScope(vec![]),
                subsegments: vec![],
            }],
        }),
    )
}

#[test]
fn test_raw_scope_builder_building_header() {
    expect_parse(
        "[CustomHeaderBuilder()]#{ Wowee i wish I had inline content }#",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (
                    0,
                    None,
                    Some(TestInline::InlineScope(vec![test_raw_text(
                        " Wowee i wish I had inline content ",
                    )])),
                ),
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
        "And as I was saying [CustomHeaderBuilder()]{ Wowee i wish I had inline content }",
        TestSyntaxError::CodeEmittedHeaderInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "And",
                " as I was saying",
                " ",
            )),
            code_span: TestParseSpan(
                "[CustomHeaderBuilder()]{ Wowee i wish I had inline content }",
            ),
        },
    )
}

#[test]
fn test_block_scope_builder_building_header_in_inline() {
    expect_parse_err(
        "And as I was saying [CustomHeaderBuilder()]{
            Wowee i wish I had block content
        }",
        TestSyntaxError::CodeEmittedHeaderInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "And",
                " as I was saying",
                " ",
            )),
            code_span: TestParseSpan(
                "[CustomHeaderBuilder()]{
            Wowee i wish I had block content
        }",
            ),
        },
    )
}

#[test]
fn test_raw_scope_builder_building_header_in_inline() {
    expect_parse_err(
        "And as I was saying [CustomHeaderBuilder()]#{ Wowee i wish 
            I had inline 
            and raw
            content }#",
        TestSyntaxError::CodeEmittedHeaderInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "And",
                " as I was saying",
                " ",
            )),
            code_span: TestParseSpan(
                "[CustomHeaderBuilder()]#{ Wowee i wish 
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
        "stuff at the start of a sentence [CUSTOM_INLINE_SWALLOWER]{ this is gonna be swallowed \
         }
        [CUSTOM_INLINE_SWALLOWER]{ this is gonna be swallowed } stuff at the end of a sentence",
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
        [CUSTOM_INLINE_SWALLOWER]{ this is gonna be swallowed }
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
        "[CUSTOM_INLINE_SWALLOWER]{ this is gonna be swallowed }
        [CUSTOM_INLINE_SWALLOWER]{ so is this }",
        Ok(test_doc(vec![])),
    )
}

#[test]
fn test_block_scope_builder_building_none() {
    expect_parse(
        "[CUSTOM_BLOCK_SWALLOWER]{
            this is gonna be swallowed
        
            so is this!
        }",
        Ok(test_doc(vec![])),
    )
}

#[test]
fn test_raw_scope_builder_building_none() {
    expect_parse(
        "[CUSTOM_RAW_SWALLOWER]#{ this is gonna be swallowed }#",
        Ok(test_doc(vec![])),
    )
}

// Builders are *only* allowed to emit Header | Inline | Block | None.
// They aren't allowed to emit objects that fit none of those,
// and aren't allowed to emit objects that fit multiple of those.
// That includes things that are typically coercible to inline like lists, ints, floats, strings.

#[test]
fn test_builders_cant_emit_objects_fitting_none() {
    // Test for block, raw, inline builders
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_blocks(self, arg):
            return object() # This isn't header, inline, block, or none
    -]

    [-ObjectBuilder()-]{
        block
    }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", "\n        block\n    ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_inlines(self, arg):
            return object() # This isn't header, inline, block, or none
    -]

    [-ObjectBuilder()-]{ inline }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", " inline ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_raw(self, arg):
            return object() # This isn't header, inline, block, or none
    -]

    [-ObjectBuilder()-]#{raw}#
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("#{", "raw", "}#"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
}

#[test]
fn test_builders_cant_emit_coercible_to_inline() {
    // Test for block, raw, inline builders
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_blocks(self, arg):
            return 1.0 # This is coercible, but that's invalid
    -]

    [-ObjectBuilder()-]{
        block
    }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", "\n        block\n    ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_inlines(self, arg):
            return 1982309 # This is coercible, but that's invalid
    -]

    [-ObjectBuilder()-]{ inline }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", " inline ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class ObjectBuilder:
        def build_from_raw(self, arg):
            return "a tasty string" # This is coercible, but that's invalid
    -]

    [-ObjectBuilder()-]#{raw}#
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("#{", "raw", "}#"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
}

#[test]
fn test_builders_cant_emit_objects_fitting_multiple() {
    // Test for block, raw, inline builders
    expect_parse_err(
        r#"
    [-
    class FitMultiple:
        is_block = True
        is_inline = True
        is_header = True
        weight = 0
    class ObjectBuilder:
        def build_from_blocks(self, arg):
            return FitMultiple() # This is header, inline, and block!
    -]

    [-ObjectBuilder()-]{
        block
    }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", "\n        block\n    ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class FitMultiple:
        is_block = True
        is_inline = True
        is_header = True
        weight = 0
    class ObjectBuilder:
        def build_from_inlines(self, arg):
            return FitMultiple() # This is header, inline, and block!
    -]

    [-ObjectBuilder()-]{ inline }
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("{", " inline ", "}"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
    expect_parse_err(
        r#"
    [-
    class FitMultiple:
        is_block = True
        is_inline = True
        is_header = True
        weight = 0
    class ObjectBuilder:
        def build_from_raw(self, arg):
            return FitMultiple() # This is header, inline, and block!
    -]

    [-ObjectBuilder()-]#{raw}#
    "#,
        TestUserPythonError::CoercingBuildResultToElement {
            code_ctx: TestParseContext("[-", "ObjectBuilder()", "-]"),
            arg_ctx: TestParseContext("#{", "raw", "}#"),
            err: Regex::new("TypeError").unwrap(),
        },
    );
}
