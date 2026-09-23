use rils::{Value, compile, eval};

#[test]
fn option_and_result_trait_bounds_follow_their_arguments() {
    let source = r#"
        fn require_clone<T: Clone>(value: T) -> T { value }
        fn require_copy<T: Copy>(value: T) -> T { value }

        let source_option: Option<string> = Some("text");
        let optional_text = require_clone(source_option);
        let source_result: Result<string, i32> = Ok("text");
        let result_text = require_clone(source_result);
        let source_optional_number: Option<i32> = Some(21i32);
        let optional_number = require_copy(source_optional_number);
        let source_number: Result<i32, i32> = Ok(21);
        let result_number = require_copy(source_number);
        assert!(optional_text.is_some());
        assert!(result_text.is_ok());
        optional_number.unwrap() + result_number.unwrap()
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));

    for source in [
        "fn require_copy<T: Copy>(value: T) -> T { value } let value: Option<string> = Some(\"text\"); require_copy(value)",
        "fn require_copy<T: Copy>(value: T) -> T { value } let value: Result<string, i32> = Ok(\"text\"); require_copy(value)",
    ] {
        assert!(eval(source).unwrap_err().to_string().contains("Copy"));
    }
}
