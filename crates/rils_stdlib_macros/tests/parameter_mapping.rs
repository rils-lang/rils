extern crate self as parameter_fixture;

#[path = "fixtures/parameter_storage.rs"]
#[macro_use]
mod fixture;

macro_rules! source {
    ($($tokens:tt)*) => {
        const SOURCE: &str = rils_stdlib_macros::decl_rils_source! { $($tokens)* };
    };
}
buffer_definition!(source);

#[test]
fn parameter_mapping_keeps_backend_input_types_and_exports_array_shape() {
    let rust = fixture::Buffer::<i32>::from([1, 2, 3]).into_storage();
    let runtime =
        fixture::Buffer::<i32, fixture::RuntimeArray<i32>>::from::<3>(vec![1, 2, 3]).into_storage();
    assert_eq!(rust, runtime.0);
    assert!(SOURCE.replace(' ', "").contains("values:[T;N]"), "{SOURCE}");
    assert!(!SOURCE.contains("S::Array"));
    rils_syntax::parser::parse_builtin_declarations(rils_syntax::lex(SOURCE).unwrap()).unwrap();
}
