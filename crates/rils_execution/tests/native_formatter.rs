use std::rc::Rc;

use rils_execution::{
    Type, Value,
    formatting::{FormatterBuffer, buffer_from_value, formatter_value},
    value::record_layout::RecordLayoutResolver,
};

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
