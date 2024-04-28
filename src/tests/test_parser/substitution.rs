use super::*;

#[test]
fn test_raw_newlines() {
    expect_parse(
        "#{\r}# #{\n}# #{\r\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_raw_text("\n"),
            test_text(" "),
            test_raw_text("\n"),
            test_text(" "),
            test_raw_text("\n"),
        ]])])),
    )
}

#[test]
fn test_short_hyphen_strings() {
    expect_parse(
        r"
    -
    \-

    --
    \--
    -\-

    ---
    \---
    -\--
    --\-
    \-\--
    -\-\-
    \-\-\-

    \----
    ---\-
    \---\-
    \----\-

    iubdqiouwbdw---iubwduibqwdb
    uqwbduibdqiudw--iqwbdiuqwbud

    \-\-\-\-\-\-\-\-",
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![
                test_sentence("-"),
                // Escaped dash = ASCII dash
                test_sentence("-"),
            ]),
            TestBlock::Paragraph(vec![
                // two dashes = en-dash literal
                test_sentence("\u{2013}"),
                // If one is escaped, it becomes a normal ASCII dash and the other is just a normal ASCII dash too
                test_sentence("--"),
                test_sentence("--"),
            ]),
            TestBlock::Paragraph(vec![
                // three dashes = em-dash literal
                test_sentence("\u{2014}"),
                // \--- = \- + -- = escaped + en-dash
                test_sentence("-\u{2013}"),
                // -\-- = - + \- + - = three ASCII
                test_sentence("---"),
                // --\- = en-dash + escaped
                test_sentence("\u{2013}-"),
                // \-\-- = \- + \- + - = three ASCII
                test_sentence("---"),
                // -\-\- = - + \- + \- = three ASCII
                test_sentence("---"),
                // \-\-\- = three escaped = three ASCII
                test_sentence("---"),
            ]),
            TestBlock::Paragraph(vec![
                test_sentence("-\u{2014}"),
                test_sentence("\u{2014}-"),
                test_sentence("-\u{2013}-"),
                test_sentence("-\u{2014}-"),
            ]),
            TestBlock::Paragraph(vec![
                test_sentence("iubdqiouwbdw\u{2014}iubwduibqwdb"),
                test_sentence("uqwbduibdqiudw\u{2013}iqwbdiuqwbud"),
            ]),
            TestBlock::Paragraph(vec![test_sentence("--------")]),
        ])),
    );
}

#[test]
fn test_long_hyphen_strings() {
    expect_parse(
        "----",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "----",
        )])])),
    );
    expect_parse(
        "-----",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "-----",
        )])])),
    );
    expect_parse(
        "------",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "------",
        )])])),
    );
    expect_parse(
        "-------",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "-------",
        )])])),
    );
    expect_parse(
        "--------",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "--------",
        )])])),
    );
    expect_parse(
        "---------",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "---------",
        )])])),
    );
    expect_parse(
        "----------",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "----------",
        )])])),
    );
}

// See notes/syntax.md - the reason we use dashes *inside* eval-brackets for disambiguation, instead of outside, is so that we can have all the nice hyphen constructs directly next to code.
#[test]
fn test_hyphens_outside_code() {
    expect_parse(
        r"-[1]--[2]---[3]---[4]-\--[5]\-[6]\-[7]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("-"),
            test_text("1"),
            test_text("\u{2013}"),
            test_text("2"),
            test_text("\u{2014}"),
            test_text("3"),
            test_text("\u{2014}"),
            test_text("4"),
            test_text("---"),
            test_text("5"),
            test_text("-"),
            test_text("6"),
            test_text("-"),
            test_text("7"),
        ]])])),
    )
}
