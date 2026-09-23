use std::{cell::RefCell, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::{Type, merge_types},
    value::{BTreeSetValue, HashKey, SequenceIteratorValue, Value},
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    if id == BuiltinId::BtreeSetNew {
        return Ok(Value::BTreeSet(Rc::new(BTreeSetValue {
            borrowed: std::cell::Cell::new(0),
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::Unknown),
        })));
    }
    let receiver = arguments.first().ok_or("missing BTreeSet receiver")?;
    let mutating = matches!(
        id,
        BuiltinId::BtreeSetClear | BuiltinId::BtreeSetInsert | BuiltinId::BtreeSetRemove
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeSet mutation requires a mutable reference".into());
    }
    let Value::BTreeSet(set) = super::import_receiver(receiver)? else {
        return Err("expected BTreeSet receiver".into());
    };
    if (mutating || id == BuiltinId::BtreeSetIntoIter) && set.borrowed.get() > 0 {
        return Err("cannot mutate BTreeSet while it is borrowed by an iterator".into());
    }
    match id {
        BuiltinId::BtreeSetLen => Ok(Value::Usize(set.entries.borrow().len())),
        BuiltinId::BtreeSetIsEmpty => Ok(Value::Bool(set.entries.borrow().is_empty())),
        BuiltinId::BtreeSetClear => {
            set.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        BuiltinId::BtreeSetContains => {
            let key = key(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow().contains(&key)))
        }
        BuiltinId::BtreeSetInsert => {
            let key = key(arguments, 1)?;
            let element_type = merge_types(&set.element_type.borrow(), &key.ty())
                .ok_or("BTreeSet element type mismatch")?;
            let inserted = set.entries.borrow_mut().insert(key);
            *set.element_type.borrow_mut() = element_type;
            Ok(Value::Bool(inserted))
        }
        BuiltinId::BtreeSetRemove => {
            let key = key(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow_mut().remove(&key)))
        }
        BuiltinId::BtreeSetFirstCloned | BuiltinId::BtreeSetLastCloned => {
            let entries = set.entries.borrow();
            let key = if id == BuiltinId::BtreeSetFirstCloned {
                entries.first()
            } else {
                entries.last()
            };
            Ok(Value::Option {
                value: key.map(HashKey::to_value).map(Rc::new),
                element_type: Some(set.element_type.borrow().clone()),
            })
        }
        BuiltinId::BtreeSetIsSubset
        | BuiltinId::BtreeSetIsSuperset
        | BuiltinId::BtreeSetIsDisjoint
        | BuiltinId::BtreeSetUnion
        | BuiltinId::BtreeSetIntersection
        | BuiltinId::BtreeSetDifference
        | BuiltinId::BtreeSetSymmetricDifference => {
            let other = arguments.get(1).ok_or("missing other BTreeSet")?;
            let Value::BTreeSet(other) = super::import_receiver(other)? else {
                return Err("expected other BTreeSet".into());
            };
            let element_type =
                merge_types(&set.element_type.borrow(), &other.element_type.borrow())
                    .ok_or("BTreeSet element types do not match")?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            match id {
                BuiltinId::BtreeSetIsSubset => Ok(Value::Bool(left.is_subset(&right))),
                BuiltinId::BtreeSetIsSuperset => Ok(Value::Bool(left.is_superset(&right))),
                BuiltinId::BtreeSetIsDisjoint => Ok(Value::Bool(left.is_disjoint(&right))),
                _ => {
                    let entries = match id {
                        BuiltinId::BtreeSetUnion => left.union(&right).cloned().collect(),
                        BuiltinId::BtreeSetIntersection => {
                            left.intersection(&right).cloned().collect()
                        }
                        BuiltinId::BtreeSetDifference => left.difference(&right).cloned().collect(),
                        BuiltinId::BtreeSetSymmetricDifference => {
                            left.symmetric_difference(&right).cloned().collect()
                        }
                        _ => unreachable!(),
                    };
                    Ok(Value::BTreeSet(Rc::new(BTreeSetValue {
                        borrowed: std::cell::Cell::new(0),
                        entries: RefCell::new(entries),
                        element_type: RefCell::new(element_type),
                    })))
                }
            }
        }
        BuiltinId::BtreeSetIntoIter => {
            let element_type = set.element_type.borrow().clone();
            let entries = std::mem::take(&mut *set.entries.borrow_mut());
            let items = entries.into_iter().map(|key| key.to_value()).collect();
            Ok(Value::SequenceIterator(Rc::new(SequenceIteratorValue {
                items: RefCell::new(items),
                element_type,
            })))
        }
        _ => Err("unsupported BTreeSet operation".into()),
    }
}

fn key(arguments: &[Value], index: usize) -> Result<HashKey, String> {
    HashKey::from_value(arguments.get(index).ok_or("missing BTreeSet element")?)
        .map_err(|_| "BTreeSet elements must be bool, integer, char, or string".into())
}
