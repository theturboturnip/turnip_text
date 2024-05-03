use crate::lexer::Escapable;

use super::helpers::*;
use super::test_lexer::*;
use super::test_parser::*;

/// Run the lexer AND parser on given data, checking the results of both against expected versions as specified in [super::test_lexer::expect_lex] and [super::test_parser::expect_parse]
fn expect_lex_parse<'a>(
    data: &str,
    expected_stok_types: Vec<TestTTToken<'a>>,
    expected_parse: Result<TestDocument, TestTTErrorWithContext>,
) {
    println!("{:?}", data);

    expect_lex(data, expected_stok_types);
    expect_parse(data, expected_parse)
}

use TestTTToken::*;

// These tests are condensed versions of the tests in [test_parser] that also sanity-check the tokens generated.

#[test]
fn test_inline_code() {
    expect_lex_parse(
        r#"3=[len((1,2,3))]"#,
        vec![
            OtherText("3="),
            CodeOpen(0),
            OtherText("len((1,2,3))"),
            CodeClose(0),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_code_with_extra_delimiter() {
    expect_lex_parse(
        r#"3=[-len((1,2,3))-]"#,
        vec![
            OtherText("3="),
            CodeOpen(1),
            OtherText("len((1,2,3))"),
            CodeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_code_with_long_extra_delimiter() {
    expect_lex_parse(
        r#"3=[----len((1,2,3))----]"#,
        vec![
            OtherText("3="),
            CodeOpen(4),
            OtherText("len((1,2,3))"),
            CodeClose(4),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_inline_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"3=\[[len((1,2,3))]\]"#,
        vec![
            OtherText("3="),
            Escaped(Escapable::SqrOpen),
            CodeOpen(0),
            OtherText("len((1,2,3))"),
            CodeClose(0),
            Escaped(Escapable::SqrClose),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3=["),
            test_text("3"),
            test_text("]"),
        ]])])),
    )
}

#[test]
fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_lex_parse(
        r#"3=\[\[ len((1,2,3)) \]\]"#,
        vec![
            OtherText("3="),
            Escaped(Escapable::SqrOpen),
            Escaped(Escapable::SqrOpen),
            Whitespace(" "),
            OtherText("len((1,2,3))"),
            Whitespace(" "),
            Escaped(Escapable::SqrClose),
            Escaped(Escapable::SqrClose),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"3=[[ len((1,2,3)) ]]"#,
        )])])),
    )
}

#[test]
fn test_inline_list_with_extra_delimiter() {
    expect_lex_parse(
        r#"3=[-len([1,2,3])-]"#,
        vec![
            OtherText("3="),
            CodeOpen(1),
            OtherText("len("),
            CodeOpen(0),
            OtherText("1,2,3"),
            CodeClose(0),
            OtherText(")"),
            CodeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("3="),
            test_text("3"),
        ]])])),
    )
}

#[test]
fn test_block_scope() {
    expect_lex_parse(
        r#"outside

{
inside

inside 2
}"#,
        vec![
            OtherText("outside"),
            Newline,
            Newline,
            ScopeOpen,
            Newline,
            OtherText("inside"),
            Newline,
            Newline,
            OtherText("inside"),
            Whitespace(" "),
            OtherText("2"),
            Newline,
            ScopeClose,
            EOF,
        ],
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("outside")]),
            TestBlock::BlockScope(vec![
                TestBlock::Paragraph(vec![test_sentence("inside")]),
                TestBlock::Paragraph(vec![test_sentence("inside 2")]),
            ]),
        ])),
    )
}

#[test]
fn test_raw_scope() {
    expect_lex_parse(
        "#{It's f&%#ing raw}#",
        vec![
            RawScopeOpen(1),
            OtherText("It's"),
            Whitespace(" "),
            OtherText("f&%"),
            Hashes(1),
            OtherText("ing"),
            Whitespace(" "),
            OtherText("raw"),
            RawScopeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::Raw("It's f&%#ing raw".into()),
        ]])])),
    )
}

#[test]
fn test_inline_scope() {
    expect_lex_parse(
        r#"outside {inside}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            ScopeOpen,
            OtherText("inside"),
            ScopeClose,
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            TestInline::InlineScope(vec![test_text("inside")]),
        ]])])),
    )
}

#[test]
fn test_inline_escaped_scope() {
    expect_lex_parse(
        r#"outside \{not inside\}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            Escaped(Escapable::SqgOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("inside"),
            Escaped(Escapable::SqgClose),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "outside {not inside}",
        )])])),
    )
}

#[test]
fn test_raw_scope_newlines() {
    expect_lex_parse(
        "outside #{\ninside\n}#",
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside"),
            Newline,
            RawScopeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text("\ninside\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
fn test_raw_scope_crlf_newlines() {
    expect_lex_parse(
        "outside #{\r\ninside\r\n}#",
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside"),
            Newline,
            RawScopeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text("\ninside\n"),
        ]])])),
    )
}

#[test]
fn test_inline_raw_scope() {
    expect_lex_parse(
        r#"outside #{inside}#"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            RawScopeOpen(1),
            OtherText("inside"),
            RawScopeClose(1),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("outside "),
            test_raw_text("inside"),
        ]])])),
    )
}

#[test]
fn test_inline_raw_escaped_scope() {
    expect_lex_parse(
        r#"outside \#\{not inside\}"#,
        vec![
            OtherText("outside"),
            Whitespace(" "),
            Escaped(Escapable::Hash),
            Escaped(Escapable::SqgOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("inside"),
            Escaped(Escapable::SqgClose),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "outside #{not inside}",
        )])])),
    )
}

#[test]
fn test_plain_hashes() {
    expect_lex_parse(
        r#"before ####### after"#,
        vec![
            OtherText("before"),
            Whitespace(" "),
            Hashes(7),
            Whitespace(" "),
            OtherText("after"),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("before"), // The first hash in the chain starts a comment, and trailing whitespace is ignored
        ])])),
    )
}

#[test]
fn test_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"\\#"#,
        vec![Escaped(Escapable::Backslash), Hashes(1), EOF],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "\\",
        )]])])),
    )
}

#[test]
fn test_escaped_special_with_escaped_backslash() {
    expect_lex_parse(
        r#"\\\[not code"#,
        vec![
            Escaped(Escapable::Backslash),
            Escaped(Escapable::SqrOpen),
            OtherText("not"),
            Whitespace(" "),
            OtherText("code"),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\[not code"#,
        )])])),
    )
}

#[test]
fn test_escaped_notspecial() {
    expect_lex_parse(
        r#"\a"#,
        vec![Backslash, OtherText("a"), EOF],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
fn test_escaped_newline() {
    expect_lex_parse(
        r#"escaped \
newline"#,
        vec![
            OtherText("escaped"),
            Whitespace(" "),
            Escaped(Escapable::Newline),
            OtherText("newline"),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "escaped newline",
        )])])),
    )
}

#[test]
fn test_newline_in_code() {
    expect_lex_parse(
        "[len((1,\r\n2))]",
        vec![
            CodeOpen(0),
            OtherText("len((1,"),
            Newline,
            OtherText("2))"),
            CodeClose(0),
            EOF,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}

#[test]
fn test_block_scope_vs_inline_scope() {
    expect_lex_parse(
        r#"{
block
}

{inline}"#,
        vec![
            ScopeOpen,
            Newline,
            OtherText("block"),
            Newline,
            ScopeClose,
            Newline,
            Newline,
            ScopeOpen,
            OtherText("inline"),
            ScopeClose,
            EOF,
        ],
        Ok(test_doc(vec![
            TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence("block")])]),
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![test_text(
                "inline",
            )])]]),
        ])),
    )
}
