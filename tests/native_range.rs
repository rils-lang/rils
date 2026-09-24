use rils::{Value, compile, eval};

#[test]
fn native_range_steps_match_in_interpreter_and_vm() {
    for (source, expected) in [
        (
            "let mut total = 39; for value in 1..3 { total = total + value; } total",
            Value::I32(42),
        ),
        (
            "let mut total = 0u32; for value in 254u32..255u32 { total = total + value; } total - 212u32",
            Value::U32(42),
        ),
    ] {
        assert_eq!(eval(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute().unwrap(), expected);
    }

    for direct in [
        "let mut range = 1i8..3i8; if range.next() == Some(1i8) && range.next() == Some(2i8) && range.next() == None { 42 } else { 0 }",
        "let mut range = 254u8..255u8; if range.next() == Some(254u8) && range.next() == None { 42 } else { 0 }",
    ] {
        assert_eq!(eval(direct).unwrap(), Value::I32(42));
        assert_eq!(compile(direct).unwrap().execute().unwrap(), Value::I32(42));
    }
}
