use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{
    environment::AssignError,
    types::Type,
    value::{FieldSlot, IndexedStorage, OwnedIteratorValue, Value},
};

mod binary_heap;
mod boxed;
mod btree_map;
mod btree_set;
mod callback;
mod cell_native;
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
                declaration.id,
                target,
                arguments,
            ));
        }
        if rils_builtins::builtin_function(symbol)
            .is_some_and(|function| function.native_symbol == Some(symbol))
            && symbol == rils_builtins::BuiltinId::Clone.canonical_path()?
        {
            return Some(call(rils_builtins::BuiltinId::Clone, arguments));
        }
        let member = rils_builtins::native_member(symbol)?;
        let id = member.builtin_id?;
        if let Some(signature) = member.signature {
            let expected = signature.parameters.len() + usize::from(member.receiver.is_some());
            if arguments.len() != expected {
                return Some(Err(format!(
                    "native method `{symbol}` expects {expected} arguments, found {}",
                    arguments.len()
                )));
            }
        }
        Some(call(id, arguments))
    })
}

/// Dispatches a native method that consumes its arguments without cloning them.
pub fn call_native_owned_symbol(
    symbol: &str,
    arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if is_option_unwrap_symbol(symbol) {
        return Some(call_owned_option_unwrap(arguments));
    }
    native::call_owned_symbol(symbol, arguments, context)
}

pub fn requires_owned_native_call(symbol: &str) -> bool {
    is_option_unwrap_symbol(symbol) || native::is_owned_symbol(symbol)
}

fn is_option_unwrap_symbol(symbol: &str) -> bool {
    let Some((owner, member)) = rils_builtins::native_member_owner(symbol) else {
        return false;
    };
    rils_builtins::builtin("Option").is_some_and(|option| {
        std::ptr::eq(owner, option) && member.name == "unwrap" && member.native_bridge
    })
}

pub fn call_native_symbol_with_callback<E>(
    symbol: &str,
    arguments: &[Value],
    callback: &mut NativeCallback<'_, E>,
) -> Option<Result<Value, NativeCallError<E>>> {
    native::call_callback_symbol(symbol, arguments, callback)
        .or_else(|| call_native_symbol(symbol, arguments).map(|result| result.map_err(Into::into)))
}

pub fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    if id == BuiltinId::IteratorIntoIter {
        return match arguments {
            [iterator] => Ok(iterator.clone()),
            _ => Err("IntoIterator::into_iter expects one iterator".into()),
        };
    }

    if let Some(result) = native::call(id, arguments) {
        return result;
    }

    if let Some(result) = native_set::call(id, arguments) {
        return result;
    }
    if let Some(result) = native_map::call(id, arguments) {
        return result;
    }
    if id == rils_builtins::BuiltinId::VecExtend
        && let Some(result) = vector_dynamic::extend(arguments)
    {
        return result;
    }

    match id {
        BuiltinId::BtreeSetLen
        | BuiltinId::BtreeSetIsEmpty
        | BuiltinId::BtreeSetClear
        | BuiltinId::BtreeSetContains
        | BuiltinId::BtreeSetInsert
        | BuiltinId::BtreeSetRemove
        | BuiltinId::BtreeSetFirstCloned
        | BuiltinId::BtreeSetLastCloned
        | BuiltinId::BtreeSetIsSubset
        | BuiltinId::BtreeSetIsSuperset
        | BuiltinId::BtreeSetIsDisjoint
        | BuiltinId::BtreeSetUnion
        | BuiltinId::BtreeSetIntersection
        | BuiltinId::BtreeSetDifference
        | BuiltinId::BtreeSetSymmetricDifference
        | BuiltinId::BtreeSetIntoIter => btree_set::call(id, arguments),
        BuiltinId::BtreeMapLen
        | BuiltinId::BtreeMapIsEmpty
        | BuiltinId::BtreeMapClear
        | BuiltinId::BtreeMapContainsKey
        | BuiltinId::BtreeMapInsert
        | BuiltinId::BtreeMapGetCloned
        | BuiltinId::BtreeMapRemove
        | BuiltinId::BtreeMapFirstKeyCloned
        | BuiltinId::BtreeMapLastKeyCloned
        | BuiltinId::BtreeMapIntoIter => btree_map::call(id, arguments),
        BuiltinId::BinaryHeapLen
        | BuiltinId::BinaryHeapIsEmpty
        | BuiltinId::BinaryHeapPush
        | BuiltinId::BinaryHeapPop
        | BuiltinId::BinaryHeapPeekCloned
        | BuiltinId::BinaryHeapClear => binary_heap::call(id, arguments),
        BuiltinId::VecDequeLen
        | BuiltinId::VecDequeIsEmpty
        | BuiltinId::VecDequePushFront
        | BuiltinId::VecDequePushBack
        | BuiltinId::VecDequePopFront
        | BuiltinId::VecDequePopBack
        | BuiltinId::VecDequeFrontCloned
        | BuiltinId::VecDequeBackCloned
        | BuiltinId::VecDequeClear => vec_deque::call(id, arguments),
        BuiltinId::Clone => match &arguments[0] {
            Value::Reference(reference) => reference.read()?.clone_owned(),
            value => Err(format!(
                "`clone` expects a reference, found {}; use `clone(&value)`",
                value.type_name()
            )),
        },
        BuiltinId::VecPush => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::push requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::push requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("push receiver is not Vec".into());
            };
            indexed_iter::reject_growth(&sequence)?;
            let value = &arguments[1];
            let current = sequence
                .elements
                .borrow()
                .first()
                .map(|slot| slot.type_annotation.clone())
                .or_else(|| sequence.element_type.borrow().clone())
                .unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&current, &actual)
                .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
            *sequence.element_type.borrow_mut() = Some(element_type.clone());
            sequence.elements.borrow_mut().push(FieldSlot {
                value: Some(value.clone()),
                type_annotation: element_type,
                references: 0,
            });
            Ok(Value::Unit)
        }
        BuiltinId::VecPop => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::pop requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::pop requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("pop receiver is not Vec".into());
            };
            indexed_iter::reject_mutation(&sequence)?;
            let element_type = sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let value = {
                let mut elements = sequence.elements.borrow_mut();
                if elements.last().is_some_and(|slot| slot.references > 0) {
                    return Err("cannot pop a referenced Vec element".into());
                }
                elements.pop().and_then(|slot| slot.value).map(Rc::new)
            };
            Ok(Value::Option {
                value,
                element_type: Some(element_type),
            })
        }
        BuiltinId::VecClear | BuiltinId::VecTruncate => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            indexed_iter::reject_mutation(&sequence)?;
            let length = if id == BuiltinId::VecClear {
                0
            } else {
                let Some(length) = arguments[1].as_usize() else {
                    return Err("Vec::truncate length must be usize".into());
                };
                length
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements
                .get(length..)
                .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
            {
                return Err("cannot remove a referenced Vec element".into());
            }
            elements.truncate(length);
            Ok(Value::Unit)
        }
        BuiltinId::VecInsert | BuiltinId::VecRemove | BuiltinId::VecSwapRemove => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            indexed_iter::reject_mutation(&sequence)?;
            let Some(index) = arguments[1].as_usize() else {
                return Err("Vec index must be usize".into());
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements.iter().any(|slot| slot.references > 0) {
                return Err("cannot reorder a Vec while an element is referenced".into());
            }
            if id == BuiltinId::VecInsert {
                if index > elements.len() {
                    return Err(format!("index {index} is out of bounds for insertion"));
                }
                let value = &arguments[2];
                let expected = sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown);
                let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                let element_type = crate::types::merge_types(&expected, &actual)
                    .ok_or_else(|| format!("Vec element type is `{expected}`, found `{actual}`"))?;
                *sequence.element_type.borrow_mut() = Some(element_type.clone());
                elements.insert(
                    index,
                    FieldSlot {
                        value: Some(value.clone()),
                        type_annotation: element_type,
                        references: 0,
                    },
                );
                return Ok(Value::Unit);
            }
            if index >= elements.len() {
                return Err(format!("index {index} is out of bounds"));
            }
            let slot = if id == BuiltinId::VecRemove {
                elements.remove(index)
            } else {
                elements.swap_remove(index)
            };
            slot.value
                .ok_or_else(|| format!("element at index {index} has been moved"))
        }
        BuiltinId::VecExtend => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::extend requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::extend requires `&mut self`".into());
            }
            let Value::Vec(destination) = reference.read()? else {
                return Err("extend receiver is not Vec".into());
            };
            indexed_iter::reject_growth(&destination)?;
            let Value::Vec(source) = &arguments[1] else {
                return Err("Vec::extend source must be Vec".into());
            };
            if Rc::ptr_eq(&destination, source) {
                return Err("Vec cannot extend itself".into());
            }
            indexed_iter::reject_mutation(source)?;
            let mut source_elements = source.elements.borrow_mut();
            if source_elements.iter().any(|slot| slot.references > 0) {
                return Err("cannot move from a Vec while an element is referenced".into());
            }
            let destination_type = destination
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let source_type = source
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&destination_type, &source_type)
                .ok_or_else(|| {
                    format!("Vec element type is `{destination_type}`, found `{source_type}`")
                })?;
            *destination.element_type.borrow_mut() = Some(element_type);
            destination
                .elements
                .borrow_mut()
                .extend(source_elements.drain(..));
            Ok(Value::Unit)
        }
        BuiltinId::HashMapIter
        | BuiltinId::BtreeMapIter
        | BuiltinId::HashSetIter
        | BuiltinId::BtreeSetIter => collection_iter::call(id, arguments),
        BuiltinId::HashMapLen
        | BuiltinId::HashMapIsEmpty
        | BuiltinId::HashMapClear
        | BuiltinId::HashMapContainsKey
        | BuiltinId::HashMapInsert
        | BuiltinId::HashMapGetCloned
        | BuiltinId::HashMapRemove
        | BuiltinId::HashMapKeysCloned
        | BuiltinId::HashMapValuesCloned
        | BuiltinId::HashMapIntoIter
        | BuiltinId::HashSetLen
        | BuiltinId::HashSetIsEmpty
        | BuiltinId::HashSetClear
        | BuiltinId::HashSetContains
        | BuiltinId::HashSetInsert
        | BuiltinId::HashSetRemove
        | BuiltinId::HashSetIsSubset
        | BuiltinId::HashSetIsSuperset
        | BuiltinId::HashSetIsDisjoint
        | BuiltinId::HashSetUnion
        | BuiltinId::HashSetIntersection
        | BuiltinId::HashSetDifference
        | BuiltinId::HashSetSymmetricDifference
        | BuiltinId::HashSetIntoIter => crate::hash_collections::call(id, arguments),
        BuiltinId::IteratorNext => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Iterator::next requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Iterator::next requires `&mut self`".into());
            }
            let (next, element_type) = match reference.read()? {
                Value::OwnedIterator(iterator) => (iterator.next()?, iterator.element_type.clone()),
                Value::BorrowedIndexedIterator(iterator) => (
                    iterator.next()?,
                    Type::Reference {
                        mutable: false,
                        inner: Box::new(iterator.element_type.clone()),
                    },
                ),
                Value::BorrowedMapIterator(iterator) => (iterator.next()?, iterator.item_type()),
                Value::BorrowedSetIterator(iterator) => (iterator.next()?, iterator.item_type()),
                value if crate::value::native_ops::is_iterator(&value) => {
                    return range::next(arguments);
                }
                _ => return Err("next receiver is not an iterator".into()),
            };
            Ok(Value::Option {
                value: next.map(Rc::new),
                element_type: Some(element_type),
            })
        }
        BuiltinId::RangeNext => range::next(arguments),
        BuiltinId::IteratorCount
        | BuiltinId::IteratorLast
        | BuiltinId::IteratorNth
        | BuiltinId::IteratorCollectVec
        | BuiltinId::IteratorTake
        | BuiltinId::IteratorSkip
        | BuiltinId::IteratorRev
        | BuiltinId::IteratorEnumerate => {
            let Value::OwnedIterator(iterator) = import_receiver(&arguments[0])? else {
                return Err("iterator method receiver is not a built-in iterator".into());
            };
            let element_type = iterator.element_type.clone();
            iterator.materialize()?;
            let mut items = iterator.items.borrow_mut();
            let count = || match arguments.get(1) {
                Some(value) => value.as_usize().ok_or_else(|| {
                    format!("iterator count must be usize, found {}", value.type_name())
                }),
                None => Err("missing iterator count".into()),
            };
            match id {
                BuiltinId::IteratorCount => {
                    let count = items.len();
                    items.clear();
                    Ok(crate::numeric::native_usize(count))
                }
                BuiltinId::IteratorLast => {
                    let value = items.pop_back().map(Rc::new);
                    items.clear();
                    Ok(Value::Option {
                        value,
                        element_type: Some(element_type),
                    })
                }
                BuiltinId::IteratorNth => {
                    let count = count()?;
                    let skipped = count.min(items.len());
                    items.drain(..skipped);
                    let value = (skipped == count)
                        .then(|| items.pop_front())
                        .flatten()
                        .map(Rc::new);
                    Ok(Value::Option {
                        value,
                        element_type: Some(element_type),
                    })
                }
                BuiltinId::IteratorCollectVec => {
                    let elements = items
                        .drain(..)
                        .map(|value| FieldSlot {
                            value: Some(value),
                            type_annotation: element_type.clone(),
                            references: 0,
                        })
                        .collect();
                    Ok(Value::Vec(Rc::new(IndexedStorage {
                        active_iterators: std::cell::Cell::new(0),
                        elements: RefCell::new(elements),
                        element_type: RefCell::new(Some(element_type)),
                    })))
                }
                BuiltinId::IteratorTake | BuiltinId::IteratorSkip => {
                    let count = count()?.min(items.len());
                    let selected = if id == BuiltinId::IteratorTake {
                        let selected = items.drain(..count).collect();
                        items.clear();
                        selected
                    } else {
                        items.drain(..count);
                        items.drain(..).collect()
                    };
                    Ok(owned_iterator_value(selected, element_type))
                }
                BuiltinId::IteratorRev => Ok(owned_iterator_value(
                    items.drain(..).rev().collect(),
                    element_type,
                )),
                BuiltinId::IteratorEnumerate => Ok(owned_iterator_value(
                    items
                        .drain(..)
                        .enumerate()
                        .map(|(index, value)| {
                            tuple_value(vec![crate::numeric::native_usize(index), value])
                        })
                        .collect(),
                    Type::Tuple(vec![Type::USIZE, element_type]),
                )),
                _ => unreachable!("iterator built-in was matched above"),
            }
        }
        _ => Err(format!(
            "runtime built-in `{id:?}` has no direct implementation"
        )),
    }
}

fn call_owned_option_unwrap(mut arguments: Vec<Value>) -> Result<Value, String> {
    if arguments
        .first()
        .and_then(Type::of_value)
        .is_some_and(|ty| matches!(ty, Type::Result(_, _)))
    {
        return option_result::call("Result", "unwrap", &arguments);
    }
    arguments
        .pop()
        .ok_or_else(|| "Option::unwrap expects a receiver".to_owned())
        .and_then(crate::value::dynamic_option::take_owned)
        .and_then(|item| item.ok_or_else(|| "called `unwrap` on `None`".to_owned()))
}

fn owned_iterator_value(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(items, element_type)))
}

fn tuple_value(values: Vec<Value>) -> Value {
    let element_types = values
        .iter()
        .map(|value| Type::of_value(value).unwrap_or(Type::Unknown))
        .collect();
    Value::Tuple(Rc::new(IndexedStorage {
        active_iterators: std::cell::Cell::new(0),
        elements: RefCell::new(
            values
                .into_iter()
                .map(|value| FieldSlot {
                    type_annotation: Type::of_value(&value).unwrap_or(Type::Unknown),
                    value: Some(value),
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(Some(Type::Tuple(element_types))),
    }))
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
        use rils_builtins::BuiltinId;

        let vector = mutable_receiver(Value::Vec(Rc::new(IndexedStorage {
            active_iterators: std::cell::Cell::new(0),
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::I32)),
        })));
        assert_eq!(
            call(BuiltinId::VecPush, &[vector.clone(), Value::from_i32(7)]).unwrap(),
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
        assert_eq!(
            call(BuiltinId::IteratorNext, std::slice::from_ref(&iterator)).unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::from_i32(11))),
                element_type: Some(Type::I32),
            }
        );
        assert!(matches!(
            call(BuiltinId::IteratorNext, &[iterator]).unwrap(),
            Value::Option { value: None, .. }
        ));

        let range = mutable_receiver(
            crate::value::native_range(Value::from_i32(2), Value::from_i32(3)).unwrap(),
        );
        assert_eq!(
            call(BuiltinId::RangeNext, std::slice::from_ref(&range)).unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::from_i32(2))),
                element_type: Some(Type::I32),
            }
        );
    }

    #[test]
    fn mutable_members_reject_non_reference_receivers() {
        use rils_builtins::BuiltinId;

        assert!(
            call(BuiltinId::VecPush, &[Value::Unit, Value::from_i32(1)])
                .unwrap_err()
                .contains("mutable binding")
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
            call(
                BuiltinId::IteratorNext,
                &[owned_iterator_value(VecDeque::new(), Type::I32)]
            )
            .unwrap_err()
            .contains("mutable binding")
        );
    }

    #[test]
    fn enumerate_uses_the_shared_builtin_iterator_path() {
        use rils_builtins::BuiltinId;

        let enumerated = call(
            BuiltinId::IteratorEnumerate,
            &[owned_iterator_value(
                VecDeque::from([Value::from_i32(9)]),
                Type::I32,
            )],
        )
        .unwrap();
        let Value::OwnedIterator(iterator) = enumerated else {
            panic!("enumerate must return a built-in iterator");
        };
        let Value::Tuple(tuple) = iterator.items.borrow_mut().pop_front().unwrap() else {
            panic!("enumerate item must be a tuple");
        };
        let fields = tuple.elements.borrow();
        assert_eq!(fields[0].value, Some(Value::Usize(0)));
        assert_eq!(fields[1].value, Some(Value::from_i32(9)));
    }
}
