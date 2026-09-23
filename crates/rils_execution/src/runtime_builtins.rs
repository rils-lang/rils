use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{
    environment::{AssignError, StorageSlot},
    types::{IntegerType, Type},
    value::{
        CellValue, FieldSlot, OwnedIteratorValue, RefCellValue, ReferenceValue, SequenceValue,
        Value, WeakValue,
    },
};

mod binary_heap;
mod btree_map;
mod btree_set;
mod collection_iter;
mod option_result;
mod sequence_iter;
mod string;
mod vec_deque;

pub fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    match id {
        BuiltinId::RcNew => {
            let value = arguments
                .first()
                .cloned()
                .ok_or_else(|| "Rc::new expects one value".to_owned())?;
            let type_argument = Type::of_value(&value).unwrap_or(Type::Unknown);
            Ok(Value::Rc(Rc::new(crate::value::RcValue {
                value,
                type_argument,
            })))
        }
        BuiltinId::RcClone => match import_receiver(&arguments[0])? {
            Value::Rc(value) => Ok(Value::Rc(value)),
            Value::Struct(value) if value.type_definition.name == "Rc" => Ok(Value::Struct(value)),
            value => Err(format!("Rc::clone expects Rc, found {}", value.type_name())),
        },
        BuiltinId::RcStrongCount => match import_receiver(&arguments[0])? {
            Value::Rc(value) => Ok(Value::Usize(Rc::strong_count(&value))),
            Value::Struct(value) if value.type_definition.name == "Rc" => {
                Ok(Value::Usize(Rc::strong_count(&value)))
            }
            value => Err(format!(
                "Rc::strong_count expects Rc, found {}",
                value.type_name()
            )),
        },
        BuiltinId::RcDowngrade => match import_receiver(&arguments[0])? {
            Value::Rc(value) => Ok(Value::Weak(Rc::new(WeakValue {
                value: Rc::downgrade(&value),
                type_argument: value.type_argument.clone(),
            }))),
            value => Err(format!(
                "Rc::downgrade expects Rc, found {}",
                value.type_name()
            )),
        },
        BuiltinId::WeakUpgrade => match import_receiver(&arguments[0])? {
            Value::Weak(value) => Ok(Value::Option {
                value: value.value.upgrade().map(Value::Rc).map(Rc::new),
                element_type: Some(Type::Named {
                    name: "Rc".into(),
                    arguments: vec![value.type_argument.clone()],
                }),
            }),
            value => Err(format!(
                "Weak::upgrade expects Weak, found {}",
                value.type_name()
            )),
        },
        BuiltinId::WeakStrongCount => match import_receiver(&arguments[0])? {
            Value::Weak(value) => Ok(Value::Usize(value.value.strong_count())),
            value => Err(format!(
                "Weak::strong_count expects Weak, found {}",
                value.type_name()
            )),
        },
        BuiltinId::WeakWeakCount => match import_receiver(&arguments[0])? {
            Value::Weak(value) => Ok(Value::Usize(value.value.weak_count())),
            value => Err(format!(
                "Weak::weak_count expects Weak, found {}",
                value.type_name()
            )),
        },
        BuiltinId::CellNew => {
            let value = arguments
                .first()
                .cloned()
                .ok_or_else(|| "Cell::new expects one value".to_owned())?;
            let type_argument = Type::of_value(&value).unwrap_or(Type::Unknown);
            Ok(Value::Cell(Rc::new(CellValue {
                value: RefCell::new(value),
                type_argument,
            })))
        }
        BuiltinId::CellGet => match import_receiver(&arguments[0])? {
            Value::Cell(cell) => cell.value.borrow().clone_owned(),
            value => Err(format!(
                "Cell::get expects Cell, found {}",
                value.type_name()
            )),
        },
        BuiltinId::CellSet => match import_receiver(&arguments[0])? {
            Value::Cell(cell) => {
                let value = arguments
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "Cell::set expects a value".to_owned())?;
                *cell.value.borrow_mut() = value;
                Ok(Value::Unit)
            }
            value => Err(format!(
                "Cell::set expects Cell, found {}",
                value.type_name()
            )),
        },
        BuiltinId::CellReplace => match import_receiver(&arguments[0])? {
            Value::Cell(cell) => {
                let value = arguments
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "Cell::replace expects a value".to_owned())?;
                Ok(std::mem::replace(&mut *cell.value.borrow_mut(), value))
            }
            value => Err(format!(
                "Cell::replace expects Cell, found {}",
                value.type_name()
            )),
        },
        BuiltinId::RefCellNew => {
            let value = arguments
                .first()
                .cloned()
                .ok_or_else(|| "RefCell::new expects one value".to_owned())?;
            let type_argument = Type::of_value(&value).unwrap_or(Type::Unknown);
            let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
            storage.borrow_mut().initialize(value);
            Ok(Value::RefCell(Rc::new(RefCellValue {
                storage,
                type_argument,
            })))
        }
        BuiltinId::RefCellBorrow | BuiltinId::RefCellBorrowMut => {
            match import_receiver(&arguments[0])? {
                Value::RefCell(cell) => {
                    let mutable = matches!(id, BuiltinId::RefCellBorrowMut);
                    Ok(Value::Reference(Rc::new(ReferenceValue::new_storage(
                        cell.storage.clone(),
                        mutable,
                    ))))
                }
                value => Err(format!(
                    "RefCell borrow expects RefCell, found {}",
                    value.type_name()
                )),
            }
        }
        BuiltinId::RefCellReplace => match import_receiver(&arguments[0])? {
            Value::RefCell(cell) => {
                let value = arguments
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "RefCell::replace expects a value".to_owned())?;
                let old = cell
                    .storage
                    .borrow_mut()
                    .take()
                    .map_err(|e| format!("{e:?}"))?;
                cell.storage.borrow_mut().initialize(value);
                Ok(old)
            }
            value => Err(format!(
                "RefCell::replace expects RefCell, found {}",
                value.type_name()
            )),
        },
        BuiltinId::BtreeSetNew
        | BuiltinId::BtreeSetLen
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
        BuiltinId::BtreeMapNew
        | BuiltinId::BtreeMapLen
        | BuiltinId::BtreeMapIsEmpty
        | BuiltinId::BtreeMapClear
        | BuiltinId::BtreeMapContainsKey
        | BuiltinId::BtreeMapInsert
        | BuiltinId::BtreeMapGetCloned
        | BuiltinId::BtreeMapRemove
        | BuiltinId::BtreeMapFirstKeyCloned
        | BuiltinId::BtreeMapLastKeyCloned
        | BuiltinId::BtreeMapIntoIter => btree_map::call(id, arguments),
        BuiltinId::BinaryHeapNew
        | BuiltinId::BinaryHeapLen
        | BuiltinId::BinaryHeapIsEmpty
        | BuiltinId::BinaryHeapPush
        | BuiltinId::BinaryHeapPop
        | BuiltinId::BinaryHeapPeekCloned
        | BuiltinId::BinaryHeapClear => binary_heap::call(id, arguments),
        BuiltinId::VecDequeNew
        | BuiltinId::VecDequeLen
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
        BuiltinId::ResultIsOk
        | BuiltinId::ResultIsErr
        | BuiltinId::OptionIsSome
        | BuiltinId::OptionIsNone
        | BuiltinId::OptionUnwrap
        | BuiltinId::ResultUnwrap
        | BuiltinId::OptionUnwrapOr
        | BuiltinId::ResultUnwrapOr
        | BuiltinId::OptionExpect
        | BuiltinId::ResultExpect
        | BuiltinId::ResultOk
        | BuiltinId::ResultErr
        | BuiltinId::ResultUnwrapErr
        | BuiltinId::ResultExpectErr
        | BuiltinId::OptionTake
        | BuiltinId::OptionOr
        | BuiltinId::OptionXor
        | BuiltinId::OptionReplace => option_result::call(id, arguments),
        BuiltinId::SequenceLen | BuiltinId::StringLen => {
            let value = import_receiver(&arguments[0])?;
            let length = match value {
                Value::Array(sequence) | Value::Vec(sequence) => sequence.elements.borrow().len(),
                Value::String(value) => value.len(),
                value => {
                    return Err(format!(
                        "len receiver is not a collection: {}",
                        value.type_name()
                    ));
                }
            };
            Ok(Value::Usize(length))
        }
        BuiltinId::SequenceIsEmpty | BuiltinId::StringIsEmpty => {
            let value = import_receiver(&arguments[0])?;
            let empty = match value {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    sequence.elements.borrow().is_empty()
                }
                Value::String(value) => value.is_empty(),
                value => return Err(format!("{} has no is_empty method", value.type_name())),
            };
            Ok(Value::Bool(empty))
        }
        BuiltinId::SequenceContains | BuiltinId::StringContains => {
            match import_receiver(&arguments[0])? {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    let needle = import_receiver(&arguments[1])?;
                    let contains = sequence
                        .elements
                        .borrow()
                        .iter()
                        .any(|slot| slot.value.as_ref() == Some(&needle));
                    Ok(Value::Bool(contains))
                }
                Value::String(value) => {
                    let Value::String(needle) = &arguments[1] else {
                        return Err("string contains argument must be string".into());
                    };
                    Ok(Value::Bool(value.contains(needle.as_ref())))
                }
                _ => Err("contains receiver is not a collection".into()),
            }
        }
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
            sequence_iter::reject_growth(&sequence)?;
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
            sequence_iter::reject_mutation(&sequence)?;
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
            sequence_iter::reject_mutation(&sequence)?;
            let length = if id == BuiltinId::VecClear {
                0
            } else {
                let Value::Usize(length) = arguments[1] else {
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
            sequence_iter::reject_mutation(&sequence)?;
            let Value::Usize(index) = arguments[1] else {
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
            sequence_iter::reject_growth(&destination)?;
            let Value::Vec(source) = &arguments[1] else {
                return Err("Vec::extend source must be Vec".into());
            };
            if Rc::ptr_eq(&destination, source) {
                return Err("Vec cannot extend itself".into());
            }
            sequence_iter::reject_mutation(source)?;
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
        BuiltinId::SequenceIntoIter => {
            let (Value::Array(sequence) | Value::Vec(sequence)) = &arguments[0] else {
                return Err("into_iter receiver is not a collection".into());
            };
            sequence_iter::reject_mutation(sequence)?;
            if sequence
                .elements
                .borrow()
                .iter()
                .any(|slot| slot.references > 0)
            {
                return Err("cannot iterate a collection while an element is referenced".into());
            }
            let element_type = sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            Ok(Value::OwnedIterator(Rc::new(
                OwnedIteratorValue::from_sequence(sequence.clone(), element_type),
            )))
        }
        BuiltinId::SequenceIter | BuiltinId::SequenceIterNext => sequence_iter::call(id, arguments),
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
            let Value::OwnedIterator(iterator) = reference.read()? else {
                return Err("next receiver is not an iterator".into());
            };
            Ok(Value::Option {
                value: iterator.next()?.map(Rc::new),
                element_type: Some(iterator.element_type.clone()),
            })
        }
        BuiltinId::RangeNext => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Range::next requires a mutable range binding".into());
            };
            if !reference.mutable {
                return Err("Range::next requires `&mut self`".into());
            }
            let Value::Range(mut range) = reference.read()? else {
                return Err("Range::next receiver is not a Range".into());
            };
            let element_type = range.element_type();
            let current = range.next()?;
            reference
                .write(Value::Range(range))
                .map_err(assignment_error_message)?;
            Ok(Value::Option {
                value: current.map(Rc::new),
                element_type: Some(element_type),
            })
        }
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
                Some(Value::Usize(value)) => Ok(*value),
                Some(value) => Err(format!(
                    "iterator count must be usize, found {}",
                    value.type_name()
                )),
                None => Err("missing iterator count".into()),
            };
            match id {
                BuiltinId::IteratorCount => {
                    let count = items.len();
                    items.clear();
                    Ok(Value::Usize(count))
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
                    Ok(Value::Vec(Rc::new(SequenceValue {
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
                        .map(|(index, value)| tuple_value(vec![Value::Usize(index), value]))
                        .collect(),
                    Type::Tuple(vec![Type::USIZE, element_type]),
                )),
                _ => unreachable!("iterator built-in was matched above"),
            }
        }
        BuiltinId::StringStartsWith
        | BuiltinId::StringEndsWith
        | BuiltinId::StringFind
        | BuiltinId::StringTrim
        | BuiltinId::StringTrimStart
        | BuiltinId::StringTrimEnd
        | BuiltinId::StringToLowercase
        | BuiltinId::StringToUppercase
        | BuiltinId::StringRepeat
        | BuiltinId::StringRfind
        | BuiltinId::StringStripPrefix
        | BuiltinId::StringStripSuffix
        | BuiltinId::StringChars
        | BuiltinId::StringBytes
        | BuiltinId::StringLines
        | BuiltinId::StringSplit
        | BuiltinId::StringReplace => string::call(id, arguments),
        _ => Err(format!(
            "runtime built-in `{id:?}` has no direct implementation"
        )),
    }
}

fn owned_iterator_value(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(items, element_type)))
}

fn tuple_value(values: Vec<Value>) -> Value {
    let element_types = values
        .iter()
        .map(|value| Type::of_value(value).unwrap_or(Type::Unknown))
        .collect();
    Value::Tuple(Rc::new(SequenceValue {
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
        Value::Reference(reference) => reference.read(),
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
    fn option_and_result_state_members_use_their_stable_ids() {
        use rils_builtins::BuiltinId;

        let option = Value::Option {
            value: Some(Rc::new(Value::I32(7))),
            element_type: Some(Type::I32),
        };
        let result = Value::Result {
            value: Err(Rc::new(Value::String(Rc::from("failed")))),
            ok_type: Some(Type::I32),
            error_type: Some(Type::String),
        };
        let cases = [
            (BuiltinId::OptionIsSome, option.clone(), Value::Bool(true)),
            (BuiltinId::OptionIsNone, option, Value::Bool(false)),
            (BuiltinId::ResultIsOk, result.clone(), Value::Bool(false)),
            (BuiltinId::ResultIsErr, result, Value::Bool(true)),
        ];

        for (id, receiver, expected) in cases {
            assert_eq!(call(id, &[receiver]).unwrap(), expected, "{id:?}");
        }
    }

    #[test]
    fn unwrap_members_preserve_success_and_failure_paths() {
        use rils_builtins::BuiltinId;

        let option = Value::Option {
            value: Some(Rc::new(Value::I32(7))),
            element_type: Some(Type::I32),
        };
        assert_eq!(
            call(BuiltinId::OptionUnwrap, &[option]).unwrap(),
            Value::I32(7)
        );

        let missing = Value::Option {
            value: None,
            element_type: Some(Type::I32),
        };
        assert!(
            call(BuiltinId::OptionUnwrap, &[missing])
                .unwrap_err()
                .contains("None")
        );
    }

    #[test]
    fn mutable_members_update_their_receivers() {
        use rils_builtins::BuiltinId;

        let vector = mutable_receiver(Value::Vec(Rc::new(SequenceValue {
            active_iterators: std::cell::Cell::new(0),
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::I32)),
        })));
        assert_eq!(
            call(BuiltinId::VecPush, &[vector.clone(), Value::I32(7)]).unwrap(),
            Value::Unit
        );
        let Value::Reference(vector) = &vector else {
            unreachable!();
        };
        let Value::Vec(vector) = vector.read().unwrap() else {
            unreachable!();
        };
        assert_eq!(vector.elements.borrow()[0].value, Some(Value::I32(7)));

        let option = mutable_receiver(Value::Option {
            value: Some(Rc::new(Value::I32(3))),
            element_type: Some(Type::I32),
        });
        assert_eq!(
            call(BuiltinId::OptionTake, std::slice::from_ref(&option)).unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::I32(3))),
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
            VecDeque::from([Value::I32(11)]),
            Type::I32,
        ));
        assert_eq!(
            call(BuiltinId::IteratorNext, std::slice::from_ref(&iterator)).unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::I32(11))),
                element_type: Some(Type::I32),
            }
        );
        assert!(matches!(
            call(BuiltinId::IteratorNext, &[iterator]).unwrap(),
            Value::Option { value: None, .. }
        ));

        let range = mutable_receiver(Value::Range(
            crate::value::RangeValue::new(Value::I32(2), Value::I32(3)).unwrap(),
        ));
        assert_eq!(
            call(BuiltinId::RangeNext, std::slice::from_ref(&range)).unwrap(),
            Value::Option {
                value: Some(Rc::new(Value::I32(2))),
                element_type: Some(Type::I32),
            }
        );
    }

    #[test]
    fn mutable_members_reject_non_reference_receivers() {
        use rils_builtins::BuiltinId;

        assert!(
            call(BuiltinId::VecPush, &[Value::Unit, Value::I32(1)])
                .unwrap_err()
                .contains("mutable binding")
        );
        assert!(
            call(BuiltinId::OptionTake, &[Value::Unit])
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
                VecDeque::from([Value::I32(9)]),
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
        assert_eq!(fields[1].value, Some(Value::I32(9)));
    }
}
