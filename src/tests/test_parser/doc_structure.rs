use super::*;

#[test]
fn test_many_inner_levels() {
    expect_parse(
        "
        outside

        [TestHeader(weight=1)]

        light!

        [TestHeader(weight=2)]
        
        [TestHeader(weight=30)]

        [TestHeader(weight=54)]
        
        middling

        [TestHeader(weight=67)]

        [TestHeader(weight=100)]

        heAVEY

        ",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "outside",
            )])]),
            segments: vec![TestDocSegment {
                header: (1, None, None),
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "light!",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: (2, None, None),
                    contents: TestBlock::BlockScope(vec![]),
                    subsegments: vec![TestDocSegment {
                        header: (30, None, None),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![TestDocSegment {
                            header: (54, None, None),
                            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                test_sentence("middling"),
                            ])]),
                            subsegments: vec![TestDocSegment {
                                header: (67, None, None),
                                contents: TestBlock::BlockScope(vec![]),
                                subsegments: vec![TestDocSegment {
                                    header: (100, None, None),
                                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(
                                        vec![test_sentence("heAVEY")],
                                    )]),
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

        [TestHeader(weight=1)]

        light!

        [TestHeader(weight=2)]
        
        [TestHeader(weight=30)]

        [TestHeader(weight=54)]
        
        middling

        [TestHeader(weight=2)]

        [TestHeader(weight=20)]

        middling again!

        ",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "outside",
            )])]),
            segments: vec![TestDocSegment {
                header: (1, None, None),
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "light!",
                )])]),
                subsegments: vec![
                    TestDocSegment {
                        header: (2, None, None),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![TestDocSegment {
                            header: (30, None, None),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![TestDocSegment {
                                header: (54, None, None),
                                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                    test_sentence("middling"),
                                ])]),
                                subsegments: vec![],
                            }],
                        }],
                    },
                    TestDocSegment {
                        header: (2, None, None),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![TestDocSegment {
                            header: (20, None, None),
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

        [TestHeader(weight=10)]

        1st level

        [TestHeader(weight=0)]
        
        1st level

        [TestHeader(weight=10)]

        2nd level

        ",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "outside",
            )])]),
            segments: vec![
                TestDocSegment {
                    header: (10, None, None),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("1st level"),
                    ])]),
                    subsegments: vec![],
                },
                TestDocSegment {
                    header: (0, None, None),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("1st level"),
                    ])]),
                    subsegments: vec![TestDocSegment {
                        header: (10, None, None),
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

        [TestHeader(weight=1)]

        light!

        [TestHeader(weight=2)]
        
        [TestHeader(weight=-10)]

        [TestHeader(weight=54)]
        
        middling

        [TestHeader(weight=67)]

        [TestHeader(weight=100)]

        heAVEY

        ",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "outside",
            )])]),
            segments: vec![
                TestDocSegment {
                    header: (1, None, None),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("light!"),
                    ])]),
                    subsegments: vec![TestDocSegment {
                        header: (2, None, None),
                        contents: TestBlock::BlockScope(vec![]),
                        subsegments: vec![],
                    }],
                },
                TestDocSegment {
                    header: (-10, None, None),
                    contents: TestBlock::BlockScope(vec![]),
                    subsegments: vec![TestDocSegment {
                        header: (54, None, None),
                        contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                            test_sentence("middling"),
                        ])]),
                        subsegments: vec![TestDocSegment {
                            header: (67, None, None),
                            contents: TestBlock::BlockScope(vec![]),
                            subsegments: vec![TestDocSegment {
                                header: (100, None, None),
                                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                                    test_sentence("heAVEY"),
                                ])]),
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
    [TestHeader()]
    }",
        TestInterpError::CodeEmittedHeaderInBlockScope {
            block_scope_start: TestParseSpan("{"),
            code_span: TestParseSpan("[TestHeader()]"),
        },
    )
}

#[test]
fn test_cant_build_header_block_scope() {
    expect_parse_err(
        "{
    [TestHeaderBuilder()]{
        Sometimes Headers can be built, too!
        But if they're in a block scope it shouldn't be allowed :(
    }
    }",
        TestInterpError::CodeEmittedHeaderInBlockScope {
            block_scope_start: TestParseSpan("{"),
            code_span: TestParseSpan(
                "[TestHeaderBuilder()]{
        Sometimes Headers can be built, too!
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
    [TestHeader()]
    }",
        TestInterpError::CodeEmittedHeaderInBlockScope {
            block_scope_start: TestParseSpan("{"),
            code_span: TestParseSpan("[TestHeader()]"),
        },
    )
}

#[test]
fn test_can_create_header_toplevel_file() {
    expect_parse(
        "
        Toplevel content!

        [TestHeader(weight=123)]
        
        More content!",
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "Toplevel content!",
            )])]),
            segments: vec![TestDocSegment {
                header: (123, None, None),
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "More content!",
                )])]),
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
header_in_file = test_src("""[TestHeader(weight=123)]

Content in file!
""")
-]
        Toplevel content!

        [header_in_file]
        
        Content outside file!"#,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "Toplevel content!",
            )])]),
            segments: vec![TestDocSegment {
                header: (123, None, None),
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
    [TestHeader(weight=123)]

    Content in file!
}""")
-]
        Toplevel content!

        [header_in_file]
        
        Content outside file!"#,
        TestInterpError::CodeEmittedHeaderInBlockScope {
            block_scope_start: TestParseSpan("{"),
            code_span: TestParseSpan("[TestHeader(weight=123)]"),
        },
    )
}

#[test]
fn test_cant_create_header_inner_file_in_block_scope() {
    expect_parse_err(
        r#"
[-
header_in_file = test_src("""
[TestHeader(weight=123)]

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
            code_span: TestParseSpan("[TestHeader(weight=123)]"),
        },
    )
}

#[test]
fn test_cant_create_header_in_paragraph() {
    expect_parse_err(
        "And as I was saying [TestHeader()]",
        TestInterpError::CodeEmittedHeaderInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "And",
                " as I was saying",
                " ",
            )),
            code_span: TestParseSpan("[TestHeader()]"),
        },
    )
}

#[test]
fn test_cant_create_header_inline() {
    expect_parse_err(
        "[TEST_BLOCK_BUILDER_FROM_INLINE]{ [TestHeader()] }",
        TestInterpError::CodeEmittedHeaderInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            code_span: TestParseSpan("[TestHeader()]"),
        },
    )
}
