//! Unification of type and const parameters in callable signatures.
use super::{Type, merge_types};
use std::collections::HashMap;

pub fn infer_generic_arguments(
    expected: &Type,
    actual: &Type,
    bindings: &mut HashMap<String, Type>,
) -> Result<(), String> {
    fn bind(name: &str, actual: &Type, bindings: &mut HashMap<String, Type>) -> Result<(), String> {
        let inferred = match bindings.get(name) {
            Some(current) => merge_types(current, actual).ok_or_else(|| {
                format!("generic parameter `{name}` inferred as both `{current}` and `{actual}`")
            })?,
            None => actual.clone(),
        };
        bindings.insert(name.into(), inferred);
        Ok(())
    }
    match (expected, actual) {
        (Type::Variable(name) | Type::BoundVariable { name, .. }, actual) => {
            bind(name, actual, bindings)
        }
        (
            Type::ArrayParameter { element, length },
            Type::Array {
                element: actual,
                length: count,
            },
        ) => {
            bind(length, &Type::ConstUsize(*count), bindings)?;
            infer_generic_arguments(element, actual, bindings)
        }
        (
            Type::ArrayParameter { element, length },
            Type::ArrayParameter {
                element: actual,
                length: count,
            },
        ) => {
            bind(length, &Type::Variable(count.clone()), bindings)?;
            infer_generic_arguments(element, actual, bindings)
        }
        (
            Type::Array { element, length },
            Type::Array {
                element: actual,
                length: count,
            },
        ) if length == count => infer_generic_arguments(element, actual, bindings),
        (Type::Option(expected), Type::Option(actual))
        | (Type::Slice(expected), Type::Slice(actual)) => {
            infer_generic_arguments(expected, actual, bindings)
        }
        (
            Type::Slice(expected),
            Type::Array {
                element: actual, ..
            },
        ) => infer_generic_arguments(expected, actual, bindings),
        (
            Type::Reference { mutable, inner },
            Type::Reference {
                mutable: actual_mut,
                inner: actual,
            },
        ) if !mutable || *actual_mut => infer_generic_arguments(inner, actual, bindings),
        (Type::Result(ok, err), Type::Result(actual_ok, actual_err)) => {
            infer_generic_arguments(ok, actual_ok, bindings)?;
            infer_generic_arguments(err, actual_err, bindings)
        }
        (Type::Tuple(expected), Type::Tuple(actual)) => infer_list(expected, actual, bindings),
        (
            Type::Named { name, arguments },
            Type::Named {
                name: actual_name,
                arguments: actual,
            },
        ) if name == actual_name => infer_list(arguments, actual, bindings),
        (
            Type::Function {
                parameters,
                return_type,
            },
            Type::Function {
                parameters: actual_parameters,
                return_type: actual_return,
            },
        ) => {
            if let (Some(expected), Some(actual)) = (parameters, actual_parameters) {
                infer_list(expected, actual, bindings)?;
            }
            infer_generic_arguments(return_type, actual_return, bindings)
        }
        _ if merge_types(expected, actual).is_some() => Ok(()),
        _ => Err(format!("argument expects `{expected}`, found `{actual}`")),
    }
}

fn infer_list(
    expected: &[Type],
    actual: &[Type],
    bindings: &mut HashMap<String, Type>,
) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err("generic type arity mismatch".into());
    }
    for (expected, actual) in expected.iter().zip(actual) {
        infer_generic_arguments(expected, actual, bindings)?;
    }
    Ok(())
}
