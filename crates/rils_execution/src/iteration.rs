//! Shared dispatch for built-in iterable values and iterators.

use crate::{
    Type, runtime_builtins,
    value::{Value, native_ops},
};

pub enum IntoIteratorResult {
    Ready(Value),
    UserDefined(Value),
}

pub fn into_iterator(value: Value) -> Result<IntoIteratorResult, String> {
    into_iterator_with_context(
        value,
        &runtime_builtins::NativeOwnedContext {
            structs: Vec::new(),
            enums: Vec::new(),
            ..Default::default()
        },
    )
}

pub fn into_iterator_with_context(
    value: Value,
    context: &runtime_builtins::NativeOwnedContext,
) -> Result<IntoIteratorResult, String> {
    if matches!(&value, Value::Native(_) if native_ops::is_iterator(&value)) {
        return Ok(IntoIteratorResult::Ready(value));
    }
    // References must use their own trait implementation, never consume a referent.
    if value.as_reference().is_some() {
        return Ok(IntoIteratorResult::UserDefined(value));
    }
    let Some(ty) = Type::of_value(&value) else {
        return Ok(IntoIteratorResult::UserDefined(value));
    };
    let Some(member) = rils_frontend::standard_library::builtin_trait_member_for_type(
        &ty,
        "IntoIterator",
        "into_iter",
    ) else {
        return Ok(IntoIteratorResult::UserDefined(value));
    };
    let Some(symbol) = member.native_symbol else {
        return Ok(IntoIteratorResult::UserDefined(value));
    };
    let result = if runtime_builtins::requires_owned_native_call(symbol) {
        runtime_builtins::call_native_owned_symbol(symbol, vec![value], context)
    } else {
        runtime_builtins::call_native_symbol(symbol, &[value])
    };
    let iterator = result.ok_or_else(|| {
        format!("declared IntoIterator method `{symbol}` has no native implementation")
    })??;
    Ok(IntoIteratorResult::Ready(iterator))
}

pub(crate) fn declared_iterator_type(collection_type: &Type) -> Option<Type> {
    rils_frontend::standard_library::builtin_trait_member_for_type(
        collection_type,
        "IntoIterator",
        "into_iter",
    )?;
    let method = rils_frontend::standard_library::builtin_trait_member_type(
        "IntoIterator",
        collection_type,
        "into_iter",
    )?;
    match method {
        Type::Function { return_type, .. } => Some(*return_type),
        _ => None,
    }
}

/// Returns `None` only when a user-defined iterator method must be dispatched.
pub fn next_builtin(value: &mut Value) -> Option<Result<Option<Value>, String>> {
    match value {
        Value::Native(object) => native_ops::next(object),
        _ => None,
    }
}
