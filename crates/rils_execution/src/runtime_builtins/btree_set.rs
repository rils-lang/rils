// HashKey orders its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, rc::Rc};

use crate::{
    types::{Type, merge_types},
    value::{BTreeSetValue, HashKey, Value},
};

pub(super) fn call(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing BTreeSet receiver")?;
    let mutating = matches!(method, "clear" | "insert" | "remove");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeSet mutation requires a mutable reference".into());
    }
    let Value::BTreeSet(set) = super::import_receiver(receiver)? else {
        return Err("expected BTreeSet receiver".into());
    };
    if mutating && set.borrowed.get() > 0 {
        return Err("cannot mutate BTreeSet while it is borrowed by an iterator".into());
    }
    match method {
        "len" => Ok(crate::numeric::native_usize(set.entries.borrow().len())),
        "is_empty" => Ok(Value::Bool(set.entries.borrow().is_empty())),
        "clear" => {
            set.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        "contains" => {
            let key = key(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow().contains(&key)))
        }
        "insert" => {
            let key = key(arguments, 1)?;
            let element_type = merge_types(&set.element_type.borrow(), &key.ty())
                .ok_or("BTreeSet element type mismatch")?;
            let inserted = set.entries.borrow_mut().insert(key);
            *set.element_type.borrow_mut() = element_type;
            Ok(Value::Bool(inserted))
        }
        "remove" => {
            let key = key(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow_mut().remove(&key)))
        }
        "first_cloned" | "last_cloned" => {
            let entries = set.entries.borrow();
            let key = if method == "first_cloned" {
                entries.first()
            } else {
                entries.last()
            };
            Ok(Value::Option {
                value: key.map(HashKey::to_value).map(Rc::new),
                element_type: Some(set.element_type.borrow().clone()),
            })
        }
        "is_subset"
        | "is_superset"
        | "is_disjoint"
        | "union"
        | "intersection"
        | "difference"
        | "symmetric_difference" => {
            let other = arguments.get(1).ok_or("missing other BTreeSet")?;
            let Value::BTreeSet(other) = super::import_receiver(other)? else {
                return Err("expected other BTreeSet".into());
            };
            let element_type =
                merge_types(&set.element_type.borrow(), &other.element_type.borrow())
                    .ok_or("BTreeSet element types do not match")?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            match method {
                "is_subset" => Ok(Value::Bool(left.is_subset(&right))),
                "is_superset" => Ok(Value::Bool(left.is_superset(&right))),
                "is_disjoint" => Ok(Value::Bool(left.is_disjoint(&right))),
                _ => {
                    let entries = match method {
                        "union" => left.union(&right).cloned().collect(),
                        "intersection" => left.intersection(&right).cloned().collect(),
                        "difference" => left.difference(&right).cloned().collect(),
                        "symmetric_difference" => {
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
        _ => Err("unsupported BTreeSet operation".into()),
    }
}

pub(crate) fn into_iter(set: Rc<BTreeSetValue>) -> Result<Value, String> {
    if set.borrowed.get() > 0 {
        return Err("cannot mutate BTreeSet while it is borrowed by an iterator".into());
    }
    let element_type = set.element_type.borrow().clone();
    let entries = std::mem::take(&mut *set.entries.borrow_mut());
    let collection_type = Type::Named {
        name: "BTreeSet".into(),
        arguments: vec![element_type.clone()],
    };
    Ok(crate::iteration::generated_collection_iterator(
        entries.into_iter().map(|key| key.to_value()),
        element_type,
        &collection_type,
    ))
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let member = rils_builtins::builtin_member("BTreeSet", "new")?;
    if member.native_symbol != Some(symbol) {
        return None;
    }
    Some(if arguments.is_empty() {
        Ok(Value::BTreeSet(Rc::new(BTreeSetValue {
            borrowed: std::cell::Cell::new(0),
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::Unknown),
        })))
    } else {
        Err(format!(
            "BTreeSet::new expects 0 arguments, found {}",
            arguments.len()
        ))
    })
}

fn key(arguments: &[Value], index: usize) -> Result<HashKey, String> {
    HashKey::from_ordered_value(arguments.get(index).ok_or("missing BTreeSet element")?)
        .map_err(|_| "BTreeSet elements must be bool, integer, char, or string".into())
}
