//! Owned array-to-Vec construction.

use super::NativeOwnedContext;
use crate::Value;

pub(super) fn from_array(
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "Vec::from expects one array, found {} arguments",
            arguments.len()
        ));
    }
    let value = arguments.pop().expect("arity checked");
    if let Value::Dynamic(object) = value {
        return crate::value::native_array::into_vec(object, context);
    }
    Err("Vec::from expects an array".into())
}
