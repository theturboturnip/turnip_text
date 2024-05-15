use super::*;

fn example_flat_block() -> Vec<TestBlock> {
    vec![
        TestBlock::Paragraph(vec![test_sentence("one")]),
        TestBlock::Paragraph(vec![test_sentence("two")]),
        TestBlock::Paragraph(vec![test_sentence("three")]),
        TestBlock::Paragraph(vec![test_sentence("four")]),
        TestBlock::Paragraph(vec![test_sentence("five")]),
        TestBlock::Paragraph(vec![test_sentence("six")]),
    ]
}

#[test]
fn test_in_doc_block_scopes_are_flattened() {
    expect_parse(
        "
    one

    # Empty blockscopes when flattened mean nothing
    {

    }

    {
        two

        {
            three

            {
                four
            }
            
            five
        }
    }

    six
    ",
        Ok(test_doc(example_flat_block())),
    )
}

#[test]
fn test_code_produced_block_scopes_are_flattened() {
    expect_parse(
        r#"
    [-
    from _native import BlockScope, Paragraph, Sentence, Text

    def paragraph_of(x):
        return Paragraph([Sentence([Text(x)])])
    -]

    [-
        BlockScope([
            paragraph_of("one"),
            BlockScope([]),
            BlockScope([
                paragraph_of("two"),
                BlockScope([
                    paragraph_of("three"),
                    BlockScope([
                        paragraph_of("four"),
                    ]),
                    paragraph_of("five"),
                ])
            ]),
            paragraph_of("six")
        ])
    -]

    "#,
        Ok(test_doc(example_flat_block())),
    )
}

#[test]
fn test_code_nodes_surrounding_block_scopes_are_not_flattened() {
    expect_parse(
        r#"
    [-
    from _native import BlockScope, Paragraph, Sentence, Text

    def paragraph_of(x):
        return Paragraph([Sentence([Text(x)])])
    -]

    [-
        CustomBlock(
            BlockScope([
                paragraph_of("one"),
                BlockScope([]),
                BlockScope([
                    paragraph_of("two"),
                    BlockScope([
                        paragraph_of("three"),
                        BlockScope([
                            paragraph_of("four"),
                        ]),
                        paragraph_of("five"),
                    ])
                ]),
                paragraph_of("six")
            ])
        )
    -]

    "#,
        // The toplevel vec![] inside TestBlock::CustomBlock is the first block scope
        Ok(test_doc(vec![TestBlock::CustomBlock(vec![
            TestBlock::Paragraph(vec![test_sentence("one")]),
            TestBlock::BlockScope(vec![]),
            TestBlock::BlockScope(vec![
                TestBlock::Paragraph(vec![test_sentence("two")]),
                TestBlock::BlockScope(vec![
                    TestBlock::Paragraph(vec![test_sentence("three")]),
                    TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence("four")])]),
                    TestBlock::Paragraph(vec![test_sentence("five")]),
                ]),
            ]),
            TestBlock::Paragraph(vec![test_sentence("six")]),
        ])])),
    )
}

#[test]
fn test_inserted_file_block_scopes_are_flattened() {
    expect_parse(
        r#"
    [-
    s = test_src("""

            three

            {

            }

            {
                four
            }
            
            five

    """)
    -]

    one

    {
        two

        {
            [s]
        }
    }

    six
    "#,
        Ok(test_doc(example_flat_block())),
    )
}

#[test]
fn test_inserted_file_code_produced_block_scopes_are_flattened() {
    expect_parse(
        r#"
    [--
    s = test_src("""

    [-
    class IdentityBuilder:
        def build_from_blocks(self, blocks):
            return blocks
    -]

            three

            [IdentityBuilder()]{
                {

                }

                {
                    {
                        {
                            four
                        }
                    }
                }
            }
            
            five

    """)
    --]

    one

    {
        two

        {
            [s]
        }
    }

    six
    "#,
        Ok(test_doc(example_flat_block())),
    )
}

#[test]
fn test_start_para_inline_scopes_are_flattened() {
    expect_parse(
        "{{{one} two} {} {three {four {five}}}}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("one"),
            test_text(" two"),
            test_text(" "),
            test_text(" "),
            test_text("three "),
            test_text("four "),
            test_text("five"),
        ]])])),
    )
}

#[test]
fn test_start_para_code_produced_inline_scopes_are_flattened() {
    expect_parse(
        r#"[- from _native import InlineScope, Text -] [---  InlineScope([
            InlineScope([
                InlineScope([
                    InlineScope([
                        Text("one"),
                    ])
                ]),
                InlineScope([
                    Text("two"),
                ]),
                InlineScope([]),
                InlineScope([
                    Text("three"),
                    InlineScope([
                        Text("four"),
                        InlineScope([
                            Text("five"),
                        ]),
                    ]),
                ]),
            ])
        ])  ---]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("one"),
            test_text("two"),
            test_text("three"),
            test_text("four"),
            test_text("five"),
        ]])])),
    )
}

#[test]
fn test_mid_para_inline_scopes_are_flattened() {
    expect_parse(
        "wow some stuff
        some more stuff and then {{{one} two} {} {three {four {five}}}}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("wow some stuff"),
            vec![
                test_text("some more stuff and then "),
                test_text("one"),
                test_text(" two"),
                test_text(" "),
                test_text(" "),
                test_text("three "),
                test_text("four "),
                test_text("five"),
            ],
        ])])),
    )
}

#[test]
fn test_mid_para_code_produced_inline_scopes_are_flattened() {
    expect_parse(
        r#"[- from _native import InlineScope, Text -]
        
        
        wow some stuff
        some more stuff and then [---  InlineScope([
            InlineScope([
                InlineScope([
                    InlineScope([
                        Text("one"),
                    ])
                ]),
                InlineScope([
                    Text("two"),
                ]),
                InlineScope([]),
                InlineScope([
                    Text("three"),
                    InlineScope([
                        Text("four"),
                        InlineScope([
                            Text("five"),
                        ]),
                    ]),
                ]),
            ])
        ])  ---]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("wow some stuff"),
            vec![
                test_text("some more stuff and then "),
                test_text("one"),
                test_text("two"),
                test_text("three"),
                test_text("four"),
                test_text("five"),
            ],
        ])])),
    )
}

#[test]
fn test_code_nodes_surrounding_inline_scopes_are_not_flattened() {
    expect_parse(
        r#"[- from _native import InlineScope, Text -] [---  CustomInline(
            InlineScope([
                InlineScope([
                    InlineScope([
                        Text("one"),
                    ])
                ]),
                InlineScope([
                    Text("two"),
                ]),
                InlineScope([]),
                InlineScope([
                    Text("three"),
                    InlineScope([
                        Text("four"),
                        InlineScope([
                            Text("five"),
                        ]),
                    ]),
                ]),
            ])
        )  ---]"#,
        // The first vec![] of CustomInline is the first InlineScope
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![
                TestInline::InlineScope(vec![TestInline::InlineScope(vec![test_text("one")])]),
                TestInline::InlineScope(vec![test_text("two")]),
                TestInline::InlineScope(vec![]),
                TestInline::InlineScope(vec![
                    test_text("three"),
                    TestInline::InlineScope(vec![
                        test_text("four"),
                        TestInline::InlineScope(vec![test_text("five")]),
                    ]),
                ]),
            ]),
        ]])])),
    )
}
