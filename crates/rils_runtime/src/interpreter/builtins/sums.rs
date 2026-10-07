//! Owned language constructors share the declaration-driven storage context.

use super::*;
use crate::runtime_builtins::NativeOwnedContext;

pub(super) fn some(
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
) -> Result<Value, String> {
    let value = arguments
        .into_iter()
        .next()
        .expect("Some arity was checked");
    let actual = Type::of_value(&value)
        .map(|item_type| Type::Option(Box::new(item_type)))
        .ok_or("Some item has no concrete type")?;
    let ty = expected
        .map(|expected| merge_types(expected, &actual).unwrap_or_else(|| actual.clone()))
        .unwrap_or(actual);
    context.storage().construct_option(&ty, Some(value))
}

pub(super) fn ok(
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
) -> Result<Value, String> {
    result(arguments, context, expected, true)
}

pub(super) fn err(
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
) -> Result<Value, String> {
    result(arguments, context, expected, false)
}

fn result(
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
    success: bool,
) -> Result<Value, String> {
    let value = arguments
        .into_iter()
        .next()
        .expect("Result arity was checked");
    let active_type = Type::of_value(&value).ok_or("Result item has no concrete type")?;
    let actual = if success {
        Type::Result(Box::new(active_type), Box::new(Type::Unknown))
    } else {
        Type::Result(Box::new(Type::Unknown), Box::new(active_type))
    };
    let Some(expected @ Type::Result(_, _)) = expected else {
        return Err("cannot infer the complete Result type".into());
    };
    let ty = merge_types(expected, &actual)
        .ok_or_else(|| format!("Result construction expects {expected}, found {actual}"))?;
    let branch = if success { Ok(value) } else { Err(value) };
    context.storage().construct_result(&ty, branch)
}
