use super::*;

#[test]
fn escaped_newline_in_first_line() {
    // a comment ending with an escaped newline at the start line of paragraph just continues the sentence,
    // with leading whitespace ignored
    expect_parse(
        r#"Sentence 1 \
        continued in the next line
        Sentence 2
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1 continued in the next line"),
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        r#"{Sentence 1 \
        continued in the next line}
        Sentence 2
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            vec![TestInline::InlineScope(vec![test_text(
                "Sentence 1 continued in the next line",
            )])],
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
}

#[test]
fn escaped_newline_in_middle_line() {
    // a comment ending with an escaped newline at a middle line of paragraph just continues the sentence,
    // with leading whitespace ignored
    expect_parse(
        r#"Sentence 1
        Sentence 2 \
        continued in the next line
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2 continued in the next line"),
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        r#"Sentence 1
        {Sentence 2 \
        continued in the next line}
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            vec![TestInline::InlineScope(vec![test_text(
                "Sentence 2 continued in the next line",
            )])],
            test_sentence("Sentence 3"),
        ])])),
    );
}

#[test]
fn escaped_newline_in_final_line() {
    // at the end if there's content on the final line it's also the same
    expect_parse(
        r#"Sentence 1
            Sentence 2
            Sentence 3 \
            continued in the next line"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3 continued in the next line"),
        ])])),
    );
    expect_parse(
        r#"Sentence 1
            Sentence 2
            {Sentence 3 \
            continued in the next line}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            vec![TestInline::InlineScope(vec![test_text(
                "Sentence 3 continued in the next line",
            )])],
        ])])),
    );
}

#[test]
fn escaped_newline_then_blank() {
    // if there isn't content, that line is effectively wasted and doesn't end the paragraph
    // FUTURE could make some more advanced error message handling that notes all the escaped newlines in TestBlockModeElem::Para, so it's clearer that a new line is needed.
    expect_parse_err(
        r#"Sentence 1
        Sentence 2
        Sentence 3 \
        
        [TEST_BLOCK] # The above line was blank, but part of the escaped above line so didn't end the mode"#,
        TestSyntaxError::InsufficientBlockSeparation {
            last_block: TestBlockModeElem::Para(TestParseContext(
                "Sentence",
                " 1\n        Sentence 2\n        Sentence 3 \\\n        ",
                "\n",
            )),
            next_block_start: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
        },
    );

    // adding a second blank line actually ends the paragraph
    expect_parse(
        r#"Sentence 1
        Sentence 2
        Sentence 3 \
        
        
        [TEST_BLOCK] # With a second blank line, we're home free."#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![
                test_sentence("Sentence 1"),
                test_sentence("Sentence 2"),
                test_sentence("Sentence 3"),
            ]),
            TestBlock::TestOwnedBlock(vec![]),
        ])),
    );

    // if inside an inline scope, then the line continues
    expect_parse_err(
        r#"Sentence 1
        Sentence 2
        {Sentence 3 \
        }
        [TEST_BLOCK] # The above line didn't end paragraph mode"#,
        TestSyntaxError::InsufficientBlockSeparation {
            last_block: TestBlockModeElem::Para(TestParseContext(
                "Sentence",
                " 1\n        Sentence 2\n        {Sentence 3 \\\n        }",
                "\n",
            )),
            next_block_start: TestBlockModeElem::BlockFromCode(TestParseSpan("[TEST_BLOCK]")),
        },
    );
}
