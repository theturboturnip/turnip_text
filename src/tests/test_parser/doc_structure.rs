use super::*;

#[test]
fn test_many_inner_levels() {
    expect_parse(
        "
        outside

        [CustomHeader(weight=1)]

        light!

        [CustomHeader(weight=2)]
        
        [CustomHeader(weight=30)]

        [CustomHeader(weight=54)]
        
        middling

        [CustomHeader(weight=67)]

        [CustomHeader(weight=100)]

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

        [CustomHeader(weight=1)]

        light!

        [CustomHeader(weight=2)]
        
        [CustomHeader(weight=30)]

        [CustomHeader(weight=54)]
        
        middling

        [CustomHeader(weight=2)]

        [CustomHeader(weight=20)]

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

        [CustomHeader(weight=10)]

        1st level

        [CustomHeader(weight=0)]
        
        1st level

        [CustomHeader(weight=10)]

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

        [CustomHeader(weight=1)]

        light!

        [CustomHeader(weight=2)]
        
        [CustomHeader(weight=-10)]

        [CustomHeader(weight=54)]
        
        middling

        [CustomHeader(weight=67)]

        [CustomHeader(weight=100)]

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
fn test_can_create_header_toplevel_file() {
    expect_parse(
        "
        Toplevel content!

        [CustomHeader(weight=123)]
        
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
fn test_can_create_header_in_file() {
    expect_parse(
        r#"
[-
header_in_file = test_src("""[CustomHeader(weight=123)]

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
fn test_header_in_block_scope_in_file_gets_flattened() {
    expect_parse(
        r#"
[-
header_in_file = test_src("""
{
    [CustomHeader(weight=123)]

    Content in file!
}""")
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
fn test_header_in_file_in_block_scope_gets_flattened() {
    expect_parse(
        r#"
[-
header_in_file = test_src("""
[CustomHeader(weight=123)]

Content in file!
""")
-]
        Toplevel content!

        {
            [header_in_file]
            
            Content outside file!
        }"#,
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
fn test_complex_block_scopes_get_flattened() {
    expect_parse(
        r#"
    [-
    from _native import BlockScope, Paragraph, Sentence, Text

    def paragraph_of(x):
        return Paragraph([Sentence([Text(x)])])
    -]
    [-
    s = test_src("""

            three

            {
                [BlockScope([
                    CustomHeader(weight=100),
                    paragraph_of("four")
                ])]
            }
            
            five

    """)
    -]
    
    [CustomHeader(weight=1)]

    one

    {
        two 

        [CustomHeader(weight=2)]

        [s]

    }

    six

    "#,
        Ok(TestDocument {
            contents: TestBlock::BlockScope(vec![]),
            segments: vec![TestDocSegment {
                header: (1, None, None),
                contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                    "two",
                )])]),
                subsegments: vec![TestDocSegment {
                    header: (2, None, None),
                    contents: TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![
                        test_sentence("three"),
                    ])]),
                    subsegments: vec![TestDocSegment {
                        header: (100, None, None),
                        contents: TestBlock::BlockScope(vec![
                            TestBlock::Paragraph(vec![test_sentence("four")]),
                            TestBlock::Paragraph(vec![test_sentence("five")]),
                            TestBlock::Paragraph(vec![test_sentence("six")]),
                        ]),
                        subsegments: vec![],
                    }],
                }],
            }],
        }),
    )
}

#[test]
fn test_cant_create_header_in_paragraph() {
    expect_parse_err(
        "And as I was saying [CustomHeader()]",
        TestSyntaxError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "And",
                " as I was saying",
                " ",
            )),
            code_span: TestParseSpan("[CustomHeader()]"),
        },
    )
}

#[test]
fn test_cant_create_header_inline() {
    expect_parse_err(
        "[BUILD_CUSTOM_BLOCK_FROM_INLINE]{ [CustomHeader()] }",
        TestSyntaxError::CodeEmittedBlockInInlineMode {
            inl_mode: TestInlineModeContext::InlineScope {
                scope_start: TestParseSpan("{"),
            },
            code_span: TestParseSpan("[CustomHeader()]"),
        },
    )
}
