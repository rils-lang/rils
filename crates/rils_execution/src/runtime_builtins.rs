use std::rc::Rc;

#[cfg(test)]
use std::{cell::RefCell, collections::VecDeque};

use crate::{environment::AssignError, types::Type, value::Value};

#[cfg(test)]
use crate::value::{IndexedStorage, OwnedIteratorValue};

mod binary_heap;
mod boxed;
pub(crate) mod btree_map;
pub(crate) mod btree_set;
mod callback;
mod cell_native;
mod collection_constructor;
mod collection_iter;
mod indexed_iter;
mod native;
mod native_map;
mod native_set;
pub(crate) use native::{StringOutput, string_input, usize_input as string_usize_input};
pub mod native_value;
mod option_result;
mod range;
mod rc_native;
mod sequence_receiver;
mod vec_deque;
mod vector;
pub(crate) mod vector_dynamic;

pub type NativeCallback<'a, E> = dyn FnMut(&Value, &[Value]) -> Result<Value, E> + 'a;

/// Resolved nominal declarations available to an owned native call.
pub struct NativeOwnedContext {
    pub structs: Vec<Rc<crate::value::StructType>>,
    pub enums: Vec<Rc<crate::value::EnumType>>,
}

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
                [Value::Reference(reference)] => {
                    reference.read().and_then(|value| value.clone_owned())
                }
                [value] => value.clone_owned(),
                _ => Err(format!(
                    "Clone::clone expects one argument, found {}",
                    arguments.len()
                )),
            });
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
    if let Some((owner, member)) = owned_sum_member(symbol) {
        return Some(option_result::call_owned(
            owner.path,
            member.name,
            arguments,
            context,
        ));
    }
    native::call_owned_symbol(symbol, arguments, context)
}

pub fn requires_owned_native_call(symbol: &str) -> bool {
    owned_sum_member(symbol).is_some() || native::is_owned_symbol(symbol)
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

pub fn call_native_symbol_with_callback<E>(
    symbol: &str,
    arguments: &[Value],
    callback: &mut NativeCallback<'_, E>,
) -> Option<Result<Value, NativeCallError<E>>> {
    native::call_callback_symbol(symbol, arguments, callback)
        .or_else(|| call_native_symbol(symbol, arguments).map(|result| result.map_err(Into::into)))
}

#[cfg(test)]
fn owned_iterator_value(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(items, element_type)))
}

fn import_receiver(value: &Value) -> Result<Value, String> {
    match value {
        Value::Reference(reference) => import_receiver(&reference.read()?),
        Value::Dynamic(object)
            if crate::value::native_layouts::vec::matches(
                object.descriptor().layout().rils_type(),
            ) || crate::value::native_layouts::btree_set::matches(
                object.descriptor().layout().rils_type(),
            ) || crate::value::native_layouts::hash_set::matches(
                object.descriptor().layout().rils_type(),
            ) || crate::value::native_layouts::hash_map::matches(
                object.descriptor().layout().rils_type(),
            ) || crate::value::native_layouts::btree_map::matches(
                object.descriptor().layout().rils_type(),
            ) || matches!(object.descriptor().layout().rils_type(), Type::Named { name, .. } if name == "Rc" || name == "Weak" || name == "Cell" || name == "RefCell") =>
        {
            Ok(value.clone())
        }
        Value::Dynamic(_) => value
            .materialize_native_sum()
            .ok_or("dynamic value has no runtime receiver adapter")?,
        value => Ok(value.clone()),
    }
}

fn assignment_error_message(error: AssignError) -> String {
    match error {
        AssignError::Undefined => "assignment target is undefined".into(),
        AssignError::Immutable => "cannot assign to immutable local".into(),
        AssignError::TypeMismatch(expected) => {
            format!("assignment value must have type {expected}")
        }
        AssignError::OptionRequiresAnnotation => {
            "Option assignment requires a type annotation".into()
        }
        AssignError::ReferenceEscape => "reference cannot escape its scope".into(),
        AssignError::BorrowedTarget => {
            "cannot replace a value while part of it is referenced".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{environment::StorageSlot, value::ReferenceValue};

    fn mutable_receiver(value: Value) -> Value {
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        storage.borrow_mut().initialize(value);
        Value::Reference(Rc::new(ReferenceValue::new_storage(storage, true)))
    }

    #[test]
    fn option_and_result_state_members_use_native_symbols() {
        let option = Value::Option {
            value: Some(Rc::new(Value::from_i32(7))),
            element_type: Some(Type::I32),
        };
        let result = Value::Result {
            value: Err(Rc::new(Value::from_string("failed"))),
            ok_type: Some(Type::I32),
            error_type: Some(Type::String),
        };
        let cases = [
            (
                "core::option::option::is_some",
                option.clone(),
                Value::Bool(true),
            ),
            ("core::option::option::is_none", option, Value::Bool(false)),
            (
                "core::result::result::is_ok",
                result.clone(),
                Value::Bool(false),
            ),
            ("core::result::result::is_err", result, Value::Bool(true)),
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
        };
        let option = Value::Option {
            value: Some(Rc::new(Value::from_i32(7))),
            element_type: Some(Type::I32),
        };
        assert_eq!(
            call_native_owned_symbol(symbol, vec![option], &context)
                .unwrap()
                .unwrap(),
            Value::from_i32(7)
        );

        let missing = Value::Option {
            value: None,
            element_type: Some(Type::I32),
        };
        assert!(
            call_native_owned_symbol(symbol, vec![missing], &context)
                .unwrap()
                .unwrap_err()
                .contains("None")
        );
    }

    #[test]
    fn mutable_members_update_their_receivers() {
        let vector = mutable_receiver(Value::Vec(Rc::new(IndexedStorage {
            active_iterators: std::cell::Cell::new(0),
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::I32)),
        })));
        assert_eq!(
            call_native_symbol(
                rils_builtins::builtin_member("Vec", "push")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                &[vector.clone(), Value::from_i32(7)],
            )
            .unwrap()
            .unwrap(),
            Value::Unit
        );
        let Value::Reference(vector) = &vector else {
            unreachable!();
        };
        let Value::Vec(vector) = vector.read().unwrap() else {
            unreachable!();
        };
        assert_eq!(vector.elements.borrow()[0].value, Some(Value::from_i32(7)));

        let option = mutable_receiver(Value::Option {
            value: Some(Rc::new(Value::from_i32(3))),
            element_type: Some(Type::I32),
        });
        assert_eq!(
            call_native_symbol(
                rils_builtins::builtin_member("Option", "take")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                std::slice::from_ref(&option),
            )
            .unwrap()
            .unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::from_i32(3))),
                element_type: Some(Type::I32),
            }
        );
        let Value::Reference(option) = &option else {
            unreachable!();
        };
        assert!(matches!(
            option.read().unwrap(),
            Value::Option { value: None, .. }
        ));

        let iterator = mutable_receiver(owned_iterator_value(
            VecDeque::from([Value::from_i32(11)]),
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
            Value::Option {
                value: Some(Rc::new(Value::from_i32(11))),
                element_type: Some(Type::I32),
            }
        );
        assert!(matches!(
            call_native_symbol(next_symbol, &[iterator])
                .unwrap()
                .unwrap(),
            Value::Option { value: None, .. }
        ));

        let range = mutable_receiver(
            crate::value::native_range(Value::from_i32(2), Value::from_i32(3)).unwrap(),
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
            Value::Option {
                value: Some(Rc::new(Value::from_i32(2))),
                element_type: Some(Type::I32),
            }
        );
    }

    #[test]
    fn mutable_members_reject_non_reference_receivers() {
        assert!(
            call_native_symbol(
                rils_builtins::builtin_member("Vec", "push")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                &[Value::Unit, Value::from_i32(1)],
            )
            .unwrap()
            .unwrap_err()
            .contains("&mut self")
        );
        assert!(
            call_native_symbol(
                rils_builtins::builtin_member("Option", "take")
                    .unwrap()
                    .native_symbol
                    .unwrap(),
                &[Value::Unit],
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
