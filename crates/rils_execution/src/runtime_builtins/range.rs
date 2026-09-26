//! Mutable native iterator receiver adapter shared by legacy calls.

use crate::Value;

pub(super) fn next(arguments: &[Value]) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "Iterator::next expects one receiver, found {} arguments",
            arguments.len()
        ));
    }
    let Value::Reference(reference) = &arguments[0] else {
        return Err("Iterator::next requires a mutable iterator binding".into());
    };
    if !reference.mutable {
        return Err("Iterator::next requires `&mut self`".into());
    }
    let Value::Native(range) = reference.read()? else {
        return Err("Iterator::next receiver is not a native iterator".into());
    };
    match range
        .call(crate::value::native_ops::NEXT, &[])
        .ok_or_else(|| "Iterator::next receiver is not an iterator".to_owned())??
    {
        value @ Value::Option { .. } => Ok(value),
        _ => Err("native Iterator::next must return Option".into()),
    }
}
