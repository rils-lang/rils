use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    formatting::{FormatterBuffer, buffer_from_value, formatter_value},
    value::record_layout::RecordLayoutResolver,
};

fn borrowed(value: Value) -> Value {
    let slot = Rc::new(RefCell::new(
        rils_execution::environment::StorageSlot::uninitialized(false),
    ));
    slot.borrow_mut().initialize(value);
    Value::from_reference(Rc::new(rils_execution::value::ReferenceValue::new_storage(
        slot, false,
    )))
}

#[test]
fn nested_formatter_references_keep_the_original_buffer() {
    let buffer = Rc::new(FormatterBuffer::new(false));
    let mut value = formatter_value(buffer.clone()).unwrap();
    for _ in 0..3 {
        value = borrowed(value);
        let observed = buffer_from_value(&value).unwrap();
        assert!(Rc::ptr_eq(&buffer, &observed));
        observed.write_str("x");
    }
    assert_eq!(buffer.finish(), "xxx");
}

#[test]
fn borrowed_native_format_specs_and_literal_queries_preserve_values() {
    for (value, format, expected) in [
        (
            Value::from(rils_execution::value::native_string("hello")),
            "{:.3}",
            "hel",
        ),
        (
            Value::from(rils_execution::value::native_string("hello")),
            "{:?}",
            "\"hello\"",
        ),
        (
            Value::from(rils_execution::value::native_f64(1.25)),
            "{:+.2}",
            "+1.25",
        ),
        (
            Value::from(rils_execution::value::native_i32(42)),
            "{:#x}",
            "0x2a",
        ),
        (
            Value::from(rils_execution::value::native_char('x')),
            "{:?}",
            "'x'",
        ),
    ] {
        let mut reference = value.clone();
        for _ in 0..3 {
            reference = borrowed(reference);
            assert_eq!(
                rils_execution::formatting::format_arguments(
                    format,
                    std::slice::from_ref(&reference)
                )
                .unwrap(),
                expected
            );
            assert!(rils_execution::value::equality::literal_matches(&value, &reference).unwrap());
            assert!(
                !rils_execution::value::equality::literal_matches(
                    &rils_execution::value::native_bool(false).into(),
                    &reference
                )
                .unwrap()
            );
        }
    }
}

#[test]
fn formatter_uses_the_stdlib_native_layout_and_shared_output() {
    let buffer = Rc::new(FormatterBuffer::new(true));
    let value = formatter_value(buffer.clone()).unwrap();
    let Value::Dynamic(object) = &value else {
        panic!("Formatter must use native storage");
    };
    assert_eq!(
        object.descriptor().layout().rils_type().to_string(),
        "Formatter"
    );
    let registered = RecordLayoutResolver::new(&[])
        .resolve(&Type::named("Formatter"))
        .unwrap();
    assert!(registered.compatible_with(object.descriptor().layout()));
    assert!(buffer_from_value(&value).unwrap().alternate());
    buffer_from_value(&value).unwrap().write_str("text");
    assert_eq!(buffer.finish(), "text");
}
