//! Mutable native iterator dispatch through the original receiver storage.

use crate::Value;

pub(super) fn next(arguments: &[Value]) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "Iterator::next expects one receiver, found {} arguments",
            arguments.len()
        ));
    }
    let Some(reference) = arguments[0].as_reference() else {
        return Err("Iterator::next requires a mutable binding".into());
    };
    if !reference.mutable {
        return Err("Iterator::next requires `&mut self`".into());
    }
    match reference.call_native(crate::value::native_ops::NEXT, &[])? {
        value @ Value::Dynamic(_)
            if matches!(crate::Type::of_value(&value), Some(crate::Type::Option(_))) =>
        {
            Ok(value)
        }
        _ => Err("native Iterator::next must return Option".into()),
    }
}
