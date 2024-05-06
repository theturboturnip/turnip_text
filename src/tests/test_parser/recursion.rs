use super::*;

#[test]
fn recursion_bad() {
    expect_parse_err(
        r#"
    [-
    s = test_src("[s]")
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
    # The first file counts as a file
    file_stack_len = 1
    s = test_src("""
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
