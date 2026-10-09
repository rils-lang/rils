// HashKey orders its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, rc::Rc};

use crate::value::borrowed::with_legacy;

use crate::{
    types::{Type, merge_types},
    value::{BTreeSetValue, HashKey, KeyIdentity, Value},
};

pub(super) fn call(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing BTreeSet receiver")?;
    let mutating = matches!(method, "clear" | "insert" | "remove");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeSet mutation requires a mutable reference".into());
    }
    with_legacy(receiver, |value| {
        let Value::BTreeSet(set) = value else {
            return Err("expected BTreeSet receiver".into());
        };
        if mutating && set.borrowed.get() > 0 {
            return Err("cannot mutate BTreeSet while it is borrowed by an iterator".into());
        }
        match method {
            "len" => Ok(crate::numeric::native_usize(
                set.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .len(),
            )),
            "is_empty" => Ok(Value::Bool(
                set.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .is_empty(),
            )),
            "clear" => {
                set.entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?
                    .clear();
                Ok(Value::Unit)
            }
            "contains" => {
                let key = query(
                    arguments,
                    &*set
                        .element_type
                        .try_borrow()
                        .map_err(|_| "collection element type is accessed")?,
                )?;
                Ok(Value::Bool(
                    set.entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?
                        .contains(&key),
                ))
            }
            "insert" => Err("insert requires owned arguments".into()),
            "remove" => {
                let key = query(
                    arguments,
                    &*set
                        .element_type
                        .try_borrow()
                        .map_err(|_| "collection element type is accessed")?,
                )?;
                Ok(Value::Bool(
                    set.entries
                        .try_borrow_mut()
                        .map_err(|_| "collection entries are accessed")?
                        .remove(&key),
                ))
            }
            "first_cloned" | "last_cloned" => {
                let entries = set
                    .entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?;
                let key = if method == "first_cloned" {
                    entries.first()
                } else {
                    entries.last()
                };
                Ok(Value::Option {
                    value: key.map(HashKey::to_value).transpose()?.map(Rc::new),
                    element_type: Some(
                        set.element_type
                            .try_borrow()
                            .map_err(|_| "collection element type is accessed")?
                            .clone(),
                    ),
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
                with_legacy(other, |value| {
                    let Value::BTreeSet(other) = value else {
                        return Err("expected other BTreeSet".into());
                    };
                    let element_type = merge_types(
                        &*set
                            .element_type
                            .try_borrow()
                            .map_err(|_| "collection element type is accessed")?,
                        &*other
                            .element_type
                            .try_borrow()
                            .map_err(|_| "collection element type is accessed")?,
                    )
                    .ok_or("BTreeSet element types do not match")?;
                    let left = set
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
                    let right = other
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
                    match method {
                        "is_subset" => Ok(Value::Bool(left.is_subset(&right))),
                        "is_superset" => Ok(Value::Bool(left.is_superset(&right))),
                        "is_disjoint" => Ok(Value::Bool(left.is_disjoint(&right))),
                        _ => {
                            let entries = match method {
                                "union" => left
                                    .union(&right)
                                    .map(HashKey::clone_owned)
                                    .collect::<Result<_, _>>()?,
                                "intersection" => left
                                    .intersection(&right)
                                    .map(HashKey::clone_owned)
                                    .collect::<Result<_, _>>()?,
                                "difference" => left
                                    .difference(&right)
                                    .map(HashKey::clone_owned)
                                    .collect::<Result<_, _>>()?,
                                "symmetric_difference" => left
                                    .symmetric_difference(&right)
                                    .map(HashKey::clone_owned)
                                    .collect::<Result<_, _>>()?,
                                _ => unreachable!(),
                            };
                            Ok(Value::BTreeSet(Rc::new(BTreeSetValue {
                                borrowed: std::cell::Cell::new(0),
                                entries: RefCell::new(entries),
                                element_type: RefCell::new(element_type),
                            })))
                        }
                    }
                })
            }
            _ => Err("unsupported BTreeSet operation".into()),
        }
    })
}

pub(crate) fn into_iter(set: Rc<BTreeSetValue>) -> Result<Value, String> {
    if set.borrowed.get() > 0 {
        return Err("cannot mutate BTreeSet while it is borrowed by an iterator".into());
    }
    let element_type = set
        .element_type
        .try_borrow()
        .map_err(|_| "collection element type is accessed")?
        .clone();
    let mut entries = set
        .entries
        .try_borrow_mut()
        .map_err(|_| "cannot consume BTreeSet while its entries are accessed")?;
    for key in entries.iter() {
        key.check_move()?;
    }
    let entries = std::mem::take(&mut *entries);
    let collection_type = Type::Named {
        name: "BTreeSet".into(),
        arguments: vec![element_type.clone()],
    };
    Ok(crate::iteration::generated_collection_iterator(
        entries.into_iter().map(HashKey::into_value),
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

fn query(arguments: &[Value], expected: &Type) -> Result<KeyIdentity, String> {
    KeyIdentity::from_value(
        arguments.get(1).ok_or("missing BTreeSet key")?,
        Some(expected),
        true,
    )
}
