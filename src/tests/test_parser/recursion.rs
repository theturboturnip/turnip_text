use super::*;

#[test]
fn recursion_bad() {
    expect_parse_err(
        r#"
    [-
    import _native as tt
    s = tt.TurnipTextSource.from_string("[s]")
    -]
    
    [s]
    "#,
        TestTTErrorWithContext::FileStackExceededLimit,
    )
}

#[test]
fn test_limited_recursion_ok() {
    expect_parse_any_ok(
        r#"
    [-
    import _native as tt
    # The first file counts as a file
    file_stack_len = 1
    s = tt.TurnipTextSource.from_string("""
[file_stack_len = file_stack_len + 1]

[file_stack_len]

# The default limit is 128
[s if file_stack_len < 128 else None]
    """)
    -]

    [s]

    "#,
    )
}
