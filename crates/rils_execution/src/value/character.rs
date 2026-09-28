//! Native storage for the built-in character type.

use std::rc::Rc;

use super::{NativeObject, NativeType, Value, native_ops};
use crate::Type;

thread_local! {
    static DESCRIPTOR: Rc<NativeType> = Rc::new(
        NativeType::new::<char>(Type::Char)
            .with_copy::<char>()
            .register_method(native_ops::EQUAL, |context| {
                let [other] = context.arguments() else {
                    return Err("char equality expects one argument".into());
                };
                let other = char_payload(other).ok_or("char equality expects char")?;
                Ok(Value::Bool(context.receiver::<char, _>(|value| *value == other)?))
            })
            .register_method(native_ops::DISPLAY, |context| {
                Ok(super::native_string(
                    context.receiver::<char, _>(|value| value.to_string())?,
                ))
            }),
    );
}

pub fn native_char(value: char) -> Value {
    Value::Native(
        NativeObject::new(DESCRIPTOR.with(Rc::clone), value)
            .expect("character descriptor matches its Rust payload"),
    )
}

pub fn char_payload(value: &Value) -> Option<char> {
    match value {
        Value::Char(value) => Some(*value),
        Value::Native(object) if object.descriptor().rils_type() == &Type::Char => {
            object.with::<char, _>(|value| *value).ok()
        }
        _ => None,
    }
}
