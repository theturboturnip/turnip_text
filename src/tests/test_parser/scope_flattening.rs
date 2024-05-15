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

// TODO automatically merge Text together
fn example_flat_inline() -> TestInline {
    TestInline::InlineScope(vec![
        test_text("one "),
        test_text("two "),
        test_text("three "),
        test_text("four "),
        test_text("five"),
    ])
}

#[test]
fn test_in_doc_block_scopes_are_flattened() {
    expect_parse(
        "
    one

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
fn test_inserted_file_block_scopes_are_flattened() {
    expect_parse(
        r#"
    [-
    s = test_src("""

            three

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
    todo!()
}

#[test]
fn test_start_para_code_produced_inline_scopes_are_flattened() {
    todo!()
}

#[test]
fn test_mid_para_inline_scopes_are_flattened() {
    todo!()
}

#[test]
fn test_mid_para_code_produced_inline_scopes_are_flattened() {
    todo!()
}
