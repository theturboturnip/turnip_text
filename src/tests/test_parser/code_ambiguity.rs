use super::*;

#[test]
fn test_code_followed_by_newline_doesnt_build() {
    // Create a class which is a block scope + inline scope + raw scope builder all in one, and also a block in its own right! See what happens when we create it with no owning responsibilities
    expect_parse(
        r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]

"#,
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
fn test_code_followed_by_content_doesnt_build() {
    // Create a class which is a block scope + inline scope + raw scope builder all in one, and also a block in its own right! See what happens when we create it with no owning responsibilities
    expect_parse(
        r#"
[-
class Super:
    is_inline = True
    test_inline = InlineScope([])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()] and stuff

"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedInline(vec![]),
            test_text(" and stuff"),
        ]])])),
    )
}

#[test]
fn test_code_followed_by_block_scope_must_build() {
    // Test that things that can build do in fact build.
    // In this case make a big thing that would give a block of text if it was used directly, and None if it's used as a builder.
    // Assert it returns None.
    expect_parse(
        r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        return None
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]{
    stuff
}

"#,
        Ok(test_doc(vec![])),
    );
    // Test that things that can't build throw errors
    expect_parse_err(
        "[TEST_BLOCK]{
        }",
        TestUserPythonExecError::CoercingBlockScopeBuilder {
            code_ctx: TestParseContext("[", "TEST_BLOCK", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*BlockScopeBuilder.*build_from_blocks.*Got <TestOwnedBlock.*")
                .unwrap(),
        },
    )
}

#[test]
fn test_code_followed_by_inline_scope_must_build() {
    // Test that things that can build do in fact build.
    // In this case make a big thing that would give inlines if it was used directly, and a sentinel if it's used as a builder.
    // Assert it returns the sentinel.
    expect_parse(
        r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        return TEST_INLINE
    def build_from_raw(self, raw):
        raise RuntimeError("argh shouldn't run this")
-]

[Super()]{ stuff }

"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedInline(vec![]),
        ]])])),
    );
    // Test that things that can't build throw errors
    expect_parse_err(
        "[TEST_INLINE]{}",
        TestUserPythonExecError::CoercingInlineScopeBuilder {
            code_ctx: TestParseContext("[", "TEST_INLINE", "]"),
            err: Regex::new(
                r"TypeError\s*:\s*Expected.*InlineScopeBuilder.*build_from_inlines.*Got <TestOwnedInline.*",
            )
            .unwrap(),
        },
    )
}

#[test]
fn test_code_followed_by_raw_scope_must_build() {
    // Test that things that can build do in fact build.
    // In this case make a big thing that would give inlines if it was used directly, and a sentinel if it's used as a builder.
    // Assert it returns the sentinel.
    expect_parse(
        r#"
[-
class Super:
    is_block = True
    test_block = BlockScope([Paragraph([Sentence([Text("shouldnt see this")])])])

    def build_from_blocks(self, blocks):
        raise RuntimeError("argh shouldn't run this")
    def build_from_inlines(self, inlines):
        raise RuntimeError("argh shouldn't run this")
    def build_from_raw(self, raw):
        return TEST_RAW
-]

[Super()]#{ stuff }#

"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw("".to_string()),
        ]])])),
    );
    // Test that things that can't build throw errors
    expect_parse_err(
        "[TEST_INLINE]#{}#",
        TestUserPythonExecError::CoercingRawScopeBuilder {
            code_ctx: TestParseContext("[", "TEST_INLINE", "]"),
            err: Regex::new(r"TypeError\s*:\s*Expected.*RawScopeBuilder.*build_from_raw.*Got <TestOwnedInline.*")
                .unwrap(),
        },
    )
}
