/// Compile-fail tests using trybuild.
///
/// These test that the DAO macro produces correct compile errors for:
/// - Invalid SQL (nonexistent table)
/// - Parameter count mismatch
/// - Missing return type
///
/// Requires DAO_DATABASE_URL to be set during compilation.
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
