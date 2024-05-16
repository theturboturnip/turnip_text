use super::*;

#[test]
fn mid_paragraph() {
    expect_parse_err(
        "
    In a paragraph.
    When suddenly, {
        A block scope
    }
    ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "In",
                " a paragraph.\n    When suddenly,",
                " ",
            )),
            block_scope_open: TestParseSpan("{"),
        },
    )
}

// If the block-scope-open is inside an inlines inside a paragraph, the context is the whole paragraph
#[test]
fn mid_paragraph_in_inline_scope() {
    expect_parse_err(
        "
    In a paragraph.
    When suddenly, {after some stuff inline scope {
        A block scope
    }
    ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "In",
                " a paragraph.\n    When suddenly, {after some stuff inline scope",
                " ",
            )),
            block_scope_open: TestParseSpan("{"),
        },
    )
}

#[test]
fn mid_para_in_nested_inline_scopes() {
    expect_parse_err(
        "
    In a paragraph.
    When suddenly, {after some stuff  { inline scope {
        A block scope
    }
    ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Paragraph(TestParseContext(
                "In",
                " a paragraph.\n    When suddenly, {after some stuff  { inline scope",
                " ",
            )),
            block_scope_open: TestParseSpan("{"),
        },
    )
}

#[test]
fn mid_paragraph_started_with_inline_scope() {
    expect_parse_err(
        "
    {after some stuff inline scope {
        A block scope
    }
    ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Inlines {
                scope_start: TestParseSpan("{"),
            },
            block_scope_open: TestParseSpan("{"),
        },
    )
}

// When you attach a builder to code, it resets the inline state.
// We aren't in inline mode cuz we're inside a paragraph, we're in inline mode cuz we're attaching something inline to code.
#[test]
fn code_inline_scope_inside_para() {
    expect_parse_err(
        "
    Some para stuff [BUILD_CUSTOM_INLINE]{after some stuff inline scope {
        A block scope
    }
    ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Inlines {
                scope_start: TestParseSpan("{"),
            },
            block_scope_open: TestParseSpan("{"),
        },
    )
}

#[test]
fn bare_nested_inline_scopes() {
    // This is not a great test lol but I can't make it better without upending the infrastructure
    expect_parse_err(
        "{    inline    {  another inline {
        block! ",
        TestSyntaxError::BlockScopeOpenedInInlineMode {
            inl_mode: TestInlineModeContext::Inlines {
                scope_start: TestParseSpan("{"),
            },
            block_scope_open: TestParseSpan("{"),
        },
    )
}
