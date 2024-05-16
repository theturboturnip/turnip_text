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
    from _native import Blocks, Paragraph, Sentence, Text

    def paragraph_of(x):
        return Paragraph([Sentence([Text(x)])])
    -]

    [-
        Blocks([
            paragraph_of("one"),
            Blocks([]),
            Blocks([
                paragraph_of("two"),
                Blocks([
                    paragraph_of("three"),
                    Blocks([
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
    from _native import Blocks, Paragraph, Sentence, Text

    def paragraph_of(x):
        return Paragraph([Sentence([Text(x)])])
    -]

    [-
        CustomBlock(
            Blocks([
                paragraph_of("one"),
                Blocks([]),
                Blocks([
                    paragraph_of("two"),
                    Blocks([
                        paragraph_of("three"),
                        Blocks([
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
            TestBlock::Blocks(vec![]),
            TestBlock::Blocks(vec![
                TestBlock::Paragraph(vec![test_sentence("two")]),
                TestBlock::Blocks(vec![
                    TestBlock::Paragraph(vec![test_sentence("three")]),
                    TestBlock::Blocks(vec![TestBlock::Paragraph(vec![test_sentence("four")])]),
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
        r#"[- from _native import Inlines, Text -] [---  Inlines([
            Inlines([
                Inlines([
                    Inlines([
                        Text("one"),
                    ])
                ]),
                Inlines([
                    Text("two"),
                ]),
                Inlines([]),
                Inlines([
                    Text("three"),
                    Inlines([
                        Text("four"),
                        Inlines([
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
        r#"[- from _native import Inlines, Text -]
        
        
        wow some stuff
        some more stuff and then [---  Inlines([
            Inlines([
                Inlines([
                    Inlines([
                        Text("one"),
                    ])
                ]),
                Inlines([
                    Text("two"),
                ]),
                Inlines([]),
                Inlines([
                    Text("three"),
                    Inlines([
                        Text("four"),
                        Inlines([
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
        r#"[- from _native import Inlines, Text -] [---  CustomInline(
            Inlines([
                Inlines([
                    Inlines([
                        Text("one"),
                    ])
                ]),
                Inlines([
                    Text("two"),
                ]),
                Inlines([]),
                Inlines([
                    Text("three"),
                    Inlines([
                        Text("four"),
                        Inlines([
                            Text("five"),
                        ]),
                    ]),
                ]),
            ])
        )  ---]"#,
        // The first vec![] of CustomInline is the first Inlines
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::CustomInline(vec![
                TestInline::Inlines(vec![TestInline::Inlines(vec![test_text("one")])]),
                TestInline::Inlines(vec![test_text("two")]),
                TestInline::Inlines(vec![]),
                TestInline::Inlines(vec![
                    test_text("three"),
                    TestInline::Inlines(vec![
                        test_text("four"),
                        TestInline::Inlines(vec![test_text("five")]),
                    ]),
                ]),
            ]),
        ]])])),
    )
}
