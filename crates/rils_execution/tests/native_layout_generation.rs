use rils_execution::{Type, value::native_layouts};
use rils_value::DynamicValue;

#[test]
fn stdlib_declarations_generate_integer_string_and_generic_option_layouts() {
    let i32_layout = native_layouts::integer::layout(&Type::I32).unwrap();
    let usize_layout = native_layouts::integer::layout(&Type::USIZE).unwrap();
    let string_layout = native_layouts::string::layout();
    assert!(i32_layout.is_copy());
    assert!(usize_layout.is_copy());
    assert!(!string_layout.is_copy());

    let i32_option = native_layouts::option::layout(i32_layout.clone()).unwrap();
    let usize_option = native_layouts::option::layout(usize_layout.clone()).unwrap();
    let string_option = native_layouts::option::layout(string_layout.clone()).unwrap();
    assert_eq!(i32_option.rils_type().to_string(), "Option<i32>");
    assert_eq!(usize_option.rils_type().to_string(), "Option<usize>");
    assert_eq!(string_option.rils_type().to_string(), "Option<string>");

    let i32_value = DynamicValue::some(
        i32_option,
        DynamicValue::from_rust(i32_layout, 9_i32).unwrap(),
    )
    .unwrap();
    let usize_value = DynamicValue::some(
        usize_option,
        DynamicValue::from_rust(usize_layout, 10_usize).unwrap(),
    )
    .unwrap();
    let string_value = DynamicValue::some(
        string_option,
        DynamicValue::from_rust(
            string_layout,
            rils_stdlib::stdlib::string::String::from("hello".to_owned()),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(i32_value.is_inline());
    assert!(usize_value.is_inline());
    assert!(!string_value.is_inline());
}
