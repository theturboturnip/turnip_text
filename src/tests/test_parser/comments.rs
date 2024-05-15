use super::*;

#[test]
fn comment_ending_with_newline_in_block_mode() {
    // It should be legal for a comment to end with a newline in block mode
    expect_parse_any_ok("Wow some stuff # a comment ending with a \n");
    // It should count as a newline for the sake of block separation
    expect_parse_any_ok(
        "
    [CUSTOM_BLOCK]   # this comment doesn't interfere with the following line
    [CUSTOM_BLOCK]
    ",
    );
}

#[test]
fn comment_ending_with_escaped_newline_in_block_mode() {
    expect_parse_err(
        "# block mode comment \\\n",
        TestSyntaxError::EscapedNewlineInBlockMode {
            newline: TestParseSpan("\\\n"),
        },
    );
}

#[test]
fn comment_ending_with_newline_in_paragraph() {
    // A comment partway through a line eliminates content
    expect_parse(
        "
    Wow I love being in a paragraph
    It's so # great
    Whatever
    ",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Wow I love being in a paragraph"),
            test_sentence("It's so"), // trailing whitespace is stripped
            test_sentence("Whatever"),
        ])])),
    );
    // A comment at the start of the first line or end line wipes the whole line
    expect_parse(
        "
    #Sentence 1
    Sentence 2
    Sentence 3",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            // test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        "
    Sentence 1
    Sentence 2
    #Sentence 3",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            // test_sentence("Sentence 3"),
        ])])),
    );
    // A comment at the start of the middle line shouldn't split the paragraph
    expect_parse(
        "
    Sentence 1
    #Sentence 2
    Sentence 3",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            // test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
}

#[test]
fn comment_ending_with_escaped_newline_in_paragraph() {
    // a comment ending with an escaped newline at the start or middle lines of paragraph just continue the sentence,
    // with leading whitespace ignored
    expect_parse(
        r#"Sentence 1 # a comment \
        continued in the next line
        Sentence 2
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            vec![test_text("Sentence 1 continued in the next line")],
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        r#"{Sentence 1 # a comment \
        continued in the next line}
        Sentence 2
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            vec![test_text("Sentence 1 continued in the next line")],
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        r#"Sentence 1
        Sentence 2 # a comment \
        continued in the next line
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            vec![test_text("Sentence 2 continued in the next line")],
            test_sentence("Sentence 3"),
        ])])),
    );
    expect_parse(
        r#"Sentence 1
        {Sentence 2 # a comment \
        continued in the next line}
        Sentence 3"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            vec![test_text("Sentence 2 continued in the next line")],
            test_sentence("Sentence 3"),
        ])])),
    );
    // at the end if there's content on the final line it's also the same
    expect_parse(
        r#"Sentence 1
        Sentence 2
        Sentence 3 # a comment \
        continued in the next line"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            vec![test_text("Sentence 3 continued in the next line")],
        ])])),
    );
    expect_parse(
        r#"Sentence 1
        Sentence 2
        {Sentence 3 # a comment \
        continued in the next line}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            vec![test_text("Sentence 3 continued in the next line")],
        ])])),
    );
    // if there isn't content, that line is effectively wasted and doesn't end the paragraph
    // FUTURE could make some more advanced error message handling that notes all the escaped newlines in TestBlockModeElem::Para, so it's clearer that a new line is needed.
    expect_parse_err(
        r#"Sentence 1
        Sentence 2
        Sentence 3 # a comment \
        
        [CUSTOM_BLOCK] # The above line was blank, but part of the escaped above line so didn't end the mode"#,
        TestSyntaxError::InsufficientBlockSeparation {
            last_block: TestBlockModeElem::Para(TestParseContext(
                "Sentence",
                " 1\n        Sentence 2\n        Sentence 3 # a comment \\\n        ",
                "\n",
            )),
            next_block_start: TestBlockModeElem::BlockFromCode(TestParseSpan("[CUSTOM_BLOCK]")),
        },
    );

    // adding a second blank line actually ends the paragraph
    expect_parse(
        r#"Sentence 1
        Sentence 2
        Sentence 3 # a comment \
        
        
        [CUSTOM_BLOCK] # With a second blank line, we're home free."#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![
                test_sentence("Sentence 1"),
                test_sentence("Sentence 2"),
                test_sentence("Sentence 3"),
            ]),
            TestBlock::CustomBlock(vec![]),
        ])),
    );

    // if inside an inline scope, then the line continues
    expect_parse_err(
        r#"Sentence 1
        Sentence 2
        {Sentence 3 # a comment \
        }
        [CUSTOM_BLOCK] # The above line didn't end paragraph mode"#,
        TestSyntaxError::InsufficientBlockSeparation {
            last_block: TestBlockModeElem::Para(TestParseContext(
                "Sentence",
                " 1\n        Sentence 2\n        {Sentence 3 # a comment \\\n        }",
                "\n",
            )),
            next_block_start: TestBlockModeElem::BlockFromCode(TestParseSpan("[CUSTOM_BLOCK]")),
        },
    );
}

#[test]
fn comment_ending_with_eof_in_paragraph() {
    expect_parse(
        "
    Sentence 1
    Sentence 2
    Sentence 3# comment right here shouldn't prevent content from being noted",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Sentence 1"),
            test_sentence("Sentence 2"),
            test_sentence("Sentence 3"),
        ])])),
    )

    // This can't happen inside an inline scope because then you'd end inside the inline scope
}

#[test]
fn comment_not_allowed_in_inline_scope_unless_newline_escaped() {
    // Either the comment ends with an EOF
    expect_parse_err(
        "{ wow some stuff in an inline scope # but the comment eliminates the close-scope token}",
        TestSyntaxError::EndedInsideScope {
            scope_start: TestParseSpan("{"),
            eof_span: TestParseSpan(""),
        },
    );
    // or with a newline
    expect_parse_err(
        "{ wow some stuff in an inline scope # but the comment eliminates the close-scope token \n",
        TestSyntaxError::SentenceBreakInInlineScope {
            scope_start: TestParseSpan("{"),
            sentence_break: TestParseSpan("\n"),
        },
    );
    // or with an escaped newline newline
    expect_parse_any_ok("{ wow some stuff in an inline scope # but the comment eliminates the close-scope token \\\n and the line continues }");
}
