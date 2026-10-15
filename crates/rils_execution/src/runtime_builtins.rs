#[cfg(test)]
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{types::Type, value::Value};

#[cfg(test)]
use crate::value::OwnedIteratorValue;

mod boxed;
mod callback;
mod context;
pub use context::NativeOwnedContext;
pub(crate) use context::resolve_layout;
mod indexed_iter;
mod native;
pub(crate) mod native_output;
pub mod native_value;
mod range;
mod sequence_access;
mod sum_native;

pub type NativeCallback<'a, E> = dyn FnMut(&Value, Vec<Value>) -> Result<Value, E> + 'a;

#[derive(Debug)]
pub enum NativeCallError<E> {
    Bridge(String),
    Callback(E),
}

impl<E> From<String> for NativeCallError<E> {
    fn from(message: String) -> Self {
        Self::Bridge(message)
    }
}

impl<E> From<&str> for NativeCallError<E> {
    fn from(message: &str) -> Self {
        Self::Bridge(message.to_owned())
    }
}

/// Calls a native standard-library method by the path generated from its declaration.
/// Returns `None` when no native bridge has been generated for the symbol yet.
pub fn call_native_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    if is_native_constructor_symbol(symbol) {
        return Some(Err(
            "native construction requires an owned call with a type context".into(),
        ));
    }
    native::call_symbol(symbol, arguments).or_else(|| {
        if let Some((declaration, target)) = rils_builtins::intrinsic_by_symbol(symbol) {
            return Some(crate::numeric::execute_intrinsic(
                declaration.symbol,
                target,
                arguments,
            ));
        }
        if rils_builtins::builtin_member("Clone", "clone")
            .is_some_and(|member| member.native_symbol == Some(symbol))
        {
            return Some(match arguments {
                [value] if value.as_reference().is_some() => {
                    value.as_reference().unwrap().clone_referent()
                }
                [value] => value.clone_owned(),
                _ => Err(format!(
                    "Clone::clone expects one argument, found {}",
                    arguments.len()
                )),
            });
        }
        if let Some((declaration, _)) = rils_builtins::native_member_owner(symbol)
            && declaration.kind == rils_builtins::BuiltinKind::Trait
        {
            return Some(Err(format!(
                "trait method `{symbol}` has no native adapter for this receiver"
            )));
        }
        None
    })
}

/// Dispatches a native method that consumes its arguments without cloning them.
pub fn call_native_owned_symbol(
    symbol: &str,
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if is_native_constructor_symbol(symbol) {
        return call_native_constructor_symbol(symbol, arguments, context, None);
    }
    if let Some((owner, member)) = owned_sum_member(symbol)
        && owner.path == "Option"
        && arguments
            .first()
            .and_then(Type::of_value)
            .is_some_and(|ty| matches!(ty, Type::Result(_, _)))
        && let Some(symbol) = rils_builtins::builtin_member("Result", member.name)
            .and_then(|member| member.native_symbol)
    {
        return native::call_owned_symbol(symbol, arguments, context);
    }
    native::call_owned_symbol(symbol, arguments, context)
}

pub fn requires_owned_native_call(symbol: &str) -> bool {
    is_native_constructor_symbol(symbol)
        || requires_native_callback(symbol)
        || owned_sum_member(symbol).is_some()
        || native::is_owned_symbol(symbol)
}

fn is_native_constructor_symbol(symbol: &str) -> bool {
    native::is_constructor_symbol(symbol)
}

/// Calls a declared storage constructor with the complete result type, when available.
pub fn call_native_constructor_symbol(
    symbol: &str,
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
) -> Option<Result<Value, String>> {
    native::call_constructor_symbol(symbol, arguments, context, expected)
}

pub fn requires_native_callback(symbol: &str) -> bool {
    rils_builtins::requires_native_callback(symbol)
}

fn owned_sum_member(
    symbol: &str,
) -> Option<(
    &'static rils_builtins::BuiltinDeclaration,
    &'static rils_builtins::BuiltinMember,
)> {
    let (owner, member) = rils_builtins::native_member_owner(symbol)?;
    (owner.kind == rils_builtins::BuiltinKind::Enum
        && matches!(
            member.receiver,
            Some(rils_builtins::ReceiverMode::Owned | rils_builtins::ReceiverMode::Mutable)
        )
        && member.native_bridge)
        .then_some((owner, member))
}

/// Calls an exported symbol with owned arguments and a fallible callable hook.
/// `expected` supplies the complete result witness for generic outputs,
/// including constructors and branches on which the callback is not invoked.
pub fn call_native_symbol_with_callback<E>(
    symbol: &str,
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
    callback: &mut NativeCallback<'_, E>,
) -> Option<Result<Value, NativeCallError<E>>> {
    if is_native_constructor_symbol(symbol) {
        call_native_constructor_symbol(symbol, arguments, context, expected)
            .map(|result| result.map_err(Into::into))
    } else if requires_native_callback(symbol) {
        native::call_callback_symbol(symbol, arguments, context, expected, callback).map(|result| {
            let value = result?;
            if let Some(expected) = expected
                && (!expected.is_concrete_type() || !expected.accepts(&value))
            {
                return Err(NativeCallError::Bridge(format!(
                    "native callback result expects {expected}, found {}",
                    value.type_name()
                )));
            }
            Ok(value)
        })
    } else if requires_owned_native_call(symbol) {
        call_native_owned_symbol(symbol, arguments, context)
            .map(|result| result.map_err(Into::into))
    } else {
        call_native_symbol(symbol, &arguments).map(|result| result.map_err(Into::into))
    }
}

#[cfg(test)]
fn owned_iterator_value(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::from_owned_iterator(
        OwnedIteratorValue::from_generator(items.into_iter().map(Ok), element_type).unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{environment::StorageSlot, value::ReferenceValue};

    fn mutable_receiver(value: Value) -> Value {
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        storage.borrow_mut().initialize(value);
        Value::from_reference(Rc::new(ReferenceValue::new_storage(storage, true)))
    }

    #[test]
    fn option_and_result_state_members_use_native_symbols() {
        let option = NativeOwnedContext::default()
            .storage()
            .construct_option(
                &Type::Option(Box::new(Type::I32)),
                Some(Value::from(crate::value::native_i32(7))),
            )
            .unwrap();
        let result = NativeOwnedContext::default()
            .storage()
            .construct_result(
                &Type::Result(Box::new(Type::I32), Box::new(Type::String)),
                Err(Value::from(crate::value::native_string("failed"))),
            )
            .unwrap();
        let cases = [
            (
                "core::option::option::is_some",
                option.clone(),
                Value::from(crate::value::native_bool(true)),
            ),
            (
                "core::option::option::is_none",
                option,
                Value::from(crate::value::native_bool(false)),
            ),
            (
                "core::result::result::is_ok",
                result.clone(),
                Value::from(crate::value::native_bool(false)),
            ),
            (
                "core::result::result::is_err",
                result,
                Value::from(crate::value::native_bool(true)),
            ),
        ];

        for (symbol, receiver, expected) in cases {
            assert_eq!(call_native_symbol(symbol, &[receiver]), Some(Ok(expected)));
        }
    }

    #[test]
    fn unwrap_members_preserve_success_and_failure_paths() {
        let symbol = rils_builtins::builtin_member("Option", "unwrap")
            .unwrap()
            .native_symbol
            .unwrap();
        let context = NativeOwnedContext {
            structs: Vec::new(),
            enums: Vec::new(),
            ..Default::default()
        };
        let option = NativeOwnedContext::default()
            .storage()
            .construct_option(
                &Type::Option(Box::new(Type::I32)),
                Some(Value::from(crate::value::native_i32(7))),
            )
            .unwrap();
        assert_eq!(
            call_native_owned_symbol(symbol, vec![option], &context)
                .unwrap()
                .unwrap(),
            Value::from(crate::value::native_i32(7))
        );

        let missing = context.none(&Type::I32).unwrap();
        assert!(
            call_native_owned_symbol(symbol, vec![missing], &context)
                .unwrap()
                .unwrap_err()
                .contains("None")
        );
    }

    #[test]
    fn mutable_members_update_their_receivers() {
        let vector = mutable_receiver(
            NativeOwnedContext::default()
                .empty_collection(&Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::I32],
                })
                .unwrap(),
        );
        assert_eq!(
            call_native_owned_symbol(
                rils_builtins::builtin_member("Vec", "push")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                vec![vector.clone(), Value::from(crate::value::native_i32(7))],
                &NativeOwnedContext::default()
            )
            .unwrap()
            .unwrap(),
            Value::from(crate::value::native_unit())
        );
        let Some(vector) = &vector.as_reference() else {
            unreachable!();
        };

        vector
            .with_native_view(|view| {
                assert_eq!(view.sequence_len().unwrap(), 1);
                assert_eq!(
                    view.sequence_item(0)
                        .unwrap()
                        .with_rust::<i32, _>(|value| *value)
                        .unwrap(),
                    7
                );
            })
            .unwrap();
        let context = NativeOwnedContext::default();
        let option = mutable_receiver(
            context
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::I32)),
                    Some(Value::from(crate::value::native_i32(3))),
                )
                .unwrap(),
        );
        assert_eq!(
            call_native_owned_symbol(
                rils_builtins::builtin_member("Option", "take")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                vec![option.clone()],
                &context,
            )
            .unwrap()
            .unwrap(),
            NativeOwnedContext::default()
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::I32)),
                    Some(Value::from(crate::value::native_i32(3)))
                )
                .unwrap()
        );
        let Some(option) = &option.as_reference() else {
            unreachable!();
        };
        let value = option.read().unwrap();
        assert!(
            !value
                .with_native_view(|view| view.option_is_some())
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            Type::of_value(&value),
            Some(Type::Option(Box::new(Type::I32)))
        );

        let iterator = mutable_receiver(owned_iterator_value(
            VecDeque::from([Value::from(crate::value::native_i32(11))]),
            Type::I32,
        ));
        let next_symbol = rils_builtins::builtin_member("Iterator", "next")
            .unwrap()
            .native_symbol
            .unwrap();
        assert_eq!(
            call_native_symbol(next_symbol, std::slice::from_ref(&iterator))
                .unwrap()
                .unwrap(),
            NativeOwnedContext::default()
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::I32)),
                    Some(Value::from(crate::value::native_i32(11)))
                )
                .unwrap()
        );
        assert!(
            call_native_symbol(next_symbol, &[iterator])
                .unwrap()
                .unwrap()
                .with_native_view(|view| view.option_is_some())
                .unwrap()
                .is_ok_and(|present| !present)
        );

        let range = mutable_receiver(
            crate::value::native_range(
                Value::from(crate::value::native_i32(2)),
                Value::from(crate::value::native_i32(3)),
            )
            .unwrap(),
        );
        assert_eq!(
            call_native_symbol(
                rils_builtins::builtin_member("Range", "next")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                std::slice::from_ref(&range),
            )
            .unwrap()
            .unwrap(),
            NativeOwnedContext::default()
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::I32)),
                    Some(Value::from(crate::value::native_i32(2)))
                )
                .unwrap()
        );
    }

    #[test]
    fn mutable_members_reject_non_reference_receivers() {
        assert!(
            call_native_owned_symbol(
                rils_builtins::builtin_member("Vec", "push")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                vec![
                    Value::from(crate::value::native_unit()),
                    Value::from(crate::value::native_i32(1))
                ],
                &NativeOwnedContext::default()
            )
            .unwrap()
            .unwrap_err()
            .contains("mutable reference")
        );
        assert!(
            call_native_owned_symbol(
                rils_builtins::builtin_member("Option", "take")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                vec![Value::from(crate::value::native_unit())],
                &NativeOwnedContext::default(),
            )
            .unwrap()
            .unwrap_err()
            .contains("mutable binding")
        );
        assert!(
            call_native_symbol(
                rils_builtins::builtin_member("Iterator", "next")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                &[owned_iterator_value(VecDeque::new(), Type::I32)],
            )
            .unwrap()
            .unwrap_err()
            .contains("mutable binding")
        );
    }
}
