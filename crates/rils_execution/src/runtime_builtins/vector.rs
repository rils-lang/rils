//! Transfers Vec storage to exported Rust methods for the duration of one native call.

use std::{cell::RefMut, rc::Rc};

use rils_stdlib::stdlib::{
    collections::vector::Vec as NativeVec, iterator::Iter as NativeIterator,
    option::Option as NativeOption,
};

use crate::{
    types::{Type, merge_types},
    value::{FieldSlot, IndexedStorage, OwnedIteratorValue, Value},
};

use super::{import_receiver, indexed_iter};

struct Receiver<'a> {
    elements: RefMut<'a, Vec<FieldSlot>>,
    native: NativeVec<FieldSlot>,
}

impl<'a> Receiver<'a> {
    fn new(sequence: &'a IndexedStorage) -> Self {
        let mut elements = sequence.elements.borrow_mut();
        let native =
            <NativeVec<FieldSlot> as From<Vec<FieldSlot>>>::from(std::mem::take(&mut *elements));
        Self { elements, native }
    }
}

impl Drop for Receiver<'_> {
    fn drop(&mut self) {
        *self.elements = std::mem::take(&mut self.native).into_inner();
    }
}

fn receiver(
    arguments: &[Value],
    arity: usize,
    mutable: bool,
) -> Result<Rc<IndexedStorage>, String> {
    if arguments.len() != arity {
        return Err(format!(
            "Vec method expects {arity} arguments, found {}",
            arguments.len()
        ));
    }
    if mutable
        && !matches!(arguments.first(), Some(Value::Reference(reference)) if reference.mutable)
    {
        return Err("Vec method requires `&mut self`".into());
    }
    match import_receiver(&arguments[0])? {
        Value::Vec(sequence) => Ok(sequence),
        Value::Array(sequence) if !mutable => Ok(sequence),
        value => Err(format!(
            "Vec method expects a compatible sequence, found {}",
            value.type_name()
        )),
    }
}

pub(super) trait QueryOutput {
    fn into_value(self) -> Value;
}

impl QueryOutput for usize {
    fn into_value(self) -> Value {
        crate::numeric::native_usize(self)
    }
}

impl QueryOutput for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

pub(super) fn with_shared<R: QueryOutput>(
    arguments: &[Value],
    call: impl FnOnce(&NativeVec<FieldSlot>) -> R,
) -> Result<Value, String> {
    let storage = receiver(arguments, 1, false)?;
    let native = Receiver::new(&storage);
    Ok(call(&native.native).into_value())
}

pub(super) fn iter(arguments: &[Value]) -> Result<Value, String> {
    indexed_iter::borrow(arguments)
}

pub(super) fn contains(arguments: &[Value]) -> Result<Value, String> {
    let storage = receiver(arguments, 2, false)?;
    let needle = import_receiver(&arguments[1])?;
    Ok(Value::Bool(
        storage
            .elements
            .borrow()
            .iter()
            .any(|slot| slot.value.as_ref() == Some(&needle)),
    ))
}

pub(super) fn push(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, FieldSlot),
) -> Result<Value, String> {
    let sequence = receiver(arguments, 2, true)?;
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
    let element_type = merge_types(&current, &actual)
        .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
    let mut native = Receiver::new(&sequence);
    call(
        &mut native.native,
        FieldSlot {
            value: Some(value.clone()),
            type_annotation: element_type.clone(),
            references: 0,
        },
    );
    *sequence.element_type.borrow_mut() = Some(element_type);
    Ok(Value::Unit)
}

pub(super) fn push_owned(mut arguments: Vec<Value>) -> Result<Value, String> {
    let sequence = receiver(&arguments, 2, true)?;
    indexed_iter::reject_growth(&sequence)?;
    let value = arguments.pop().expect("arity checked");
    let current = sequence
        .elements
        .borrow()
        .first()
        .map(|slot| slot.type_annotation.clone())
        .or_else(|| sequence.element_type.borrow().clone())
        .unwrap_or(Type::Unknown);
    let actual = Type::of_value(&value).unwrap_or(Type::Unknown);
    let element_type = merge_types(&current, &actual)
        .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
    Receiver::new(&sequence).native.push(FieldSlot {
        value: Some(value),
        type_annotation: element_type.clone(),
        references: 0,
    });
    *sequence.element_type.borrow_mut() = Some(element_type);
    Ok(Value::Unit)
}

pub(super) fn pop(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>) -> NativeOption<FieldSlot>,
) -> Result<Value, String> {
    let sequence = receiver(arguments, 1, true)?;
    indexed_iter::reject_mutation(&sequence)?;
    if sequence
        .elements
        .borrow()
        .last()
        .is_some_and(|slot| slot.references > 0)
    {
        return Err("cannot pop a referenced Vec element".into());
    }
    let element_type = sequence
        .element_type
        .borrow()
        .clone()
        .unwrap_or(Type::Unknown);
    let mut native = Receiver::new(&sequence);
    let value = match call(&mut native.native) {
        NativeOption::Some(slot) => slot.value.map(Rc::new),
        NativeOption::None => None,
    };
    Ok(Value::Option {
        value,
        element_type: Some(element_type),
    })
}

pub(super) fn clear(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>),
) -> Result<Value, String> {
    let sequence = receiver(arguments, 1, true)?;
    indexed_iter::reject_mutation(&sequence)?;
    if sequence
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.references > 0)
    {
        return Err("cannot remove a referenced Vec element".into());
    }
    call(&mut Receiver::new(&sequence).native);
    Ok(Value::Unit)
}

pub(super) fn truncate(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, usize),
) -> Result<Value, String> {
    let sequence = receiver(arguments, 2, true)?;
    indexed_iter::reject_mutation(&sequence)?;
    let Some(length) = arguments[1].as_usize() else {
        return Err("Vec::truncate length must be usize".into());
    };
    if sequence
        .elements
        .borrow()
        .get(length..)
        .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
    {
        return Err("cannot remove a referenced Vec element".into());
    }
    call(&mut Receiver::new(&sequence).native, length);
    Ok(Value::Unit)
}

fn reorder_receiver(
    arguments: &[Value],
    arity: usize,
) -> Result<(Rc<IndexedStorage>, usize), String> {
    let sequence = receiver(arguments, arity, true)?;
    indexed_iter::reject_mutation(&sequence)?;
    let Some(index) = arguments[1].as_usize() else {
        return Err("Vec index must be usize".into());
    };
    if sequence
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.references > 0)
    {
        return Err("cannot reorder a Vec while an element is referenced".into());
    }
    Ok((sequence, index))
}

pub(super) fn insert(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, usize, FieldSlot),
) -> Result<Value, String> {
    let (sequence, index) = reorder_receiver(arguments, 3)?;
    if index > sequence.elements.borrow().len() {
        return Err(format!("index {index} is out of bounds for insertion"));
    }
    let expected = sequence
        .element_type
        .borrow()
        .clone()
        .unwrap_or(Type::Unknown);
    let actual = Type::of_value(&arguments[2]).unwrap_or(Type::Unknown);
    let element_type = merge_types(&expected, &actual)
        .ok_or_else(|| format!("Vec element type is `{expected}`, found `{actual}`"))?;
    call(
        &mut Receiver::new(&sequence).native,
        index,
        FieldSlot {
            value: Some(arguments[2].clone()),
            type_annotation: element_type.clone(),
            references: 0,
        },
    );
    *sequence.element_type.borrow_mut() = Some(element_type);
    Ok(Value::Unit)
}

pub(super) fn insert_owned(mut arguments: Vec<Value>) -> Result<Value, String> {
    let (sequence, index) = reorder_receiver(&arguments, 3)?;
    if index > sequence.elements.borrow().len() {
        return Err(format!("index {index} is out of bounds for insertion"));
    }
    let value = arguments.pop().expect("arity checked");
    let expected = sequence
        .element_type
        .borrow()
        .clone()
        .unwrap_or(Type::Unknown);
    let actual = Type::of_value(&value).unwrap_or(Type::Unknown);
    let element_type = merge_types(&expected, &actual)
        .ok_or_else(|| format!("Vec element type is `{expected}`, found `{actual}`"))?;
    Receiver::new(&sequence).native.insert(
        index,
        FieldSlot {
            value: Some(value),
            type_annotation: element_type.clone(),
            references: 0,
        },
    );
    *sequence.element_type.borrow_mut() = Some(element_type);
    Ok(Value::Unit)
}

pub(super) fn remove(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, usize) -> FieldSlot,
) -> Result<Value, String> {
    let (sequence, index) = reorder_receiver(arguments, 2)?;
    if index >= sequence.elements.borrow().len() {
        return Err(format!("index {index} is out of bounds"));
    }
    call(&mut Receiver::new(&sequence).native, index)
        .value
        .ok_or_else(|| format!("element at index {index} has been moved"))
}

pub(super) fn swap_remove(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, usize) -> FieldSlot,
) -> Result<Value, String> {
    let (sequence, index) = reorder_receiver(arguments, 2)?;
    if index >= sequence.elements.borrow().len() {
        return Err(format!("index {index} is out of bounds"));
    }
    call(&mut Receiver::new(&sequence).native, index)
        .value
        .ok_or_else(|| format!("element at index {index} has been moved"))
}

pub(super) fn extend(
    arguments: &[Value],
    call: impl FnOnce(&mut NativeVec<FieldSlot>, NativeVec<FieldSlot>),
) -> Result<Value, String> {
    let destination = receiver(arguments, 2, true)?;
    indexed_iter::reject_growth(&destination)?;
    let Value::Vec(source) = import_receiver(&arguments[1])? else {
        return Err("Vec::extend source must be Vec".into());
    };
    if Rc::ptr_eq(&destination, &source) {
        return Err("Vec cannot extend itself".into());
    }
    indexed_iter::reject_mutation(&source)?;
    if source
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.references > 0)
    {
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
    let element_type = merge_types(&destination_type, &source_type).ok_or_else(|| {
        format!("Vec element type is `{destination_type}`, found `{source_type}`")
    })?;
    let elements = std::mem::take(&mut *source.elements.borrow_mut());
    call(
        &mut Receiver::new(&destination).native,
        <NativeVec<FieldSlot> as From<Vec<FieldSlot>>>::from(elements),
    );
    *destination.element_type.borrow_mut() = Some(element_type);
    Ok(Value::Unit)
}

pub(super) fn into_iter(
    arguments: &[Value],
    call: impl FnOnce(NativeVec<FieldSlot>) -> NativeIterator<FieldSlot>,
) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "Vec::into_iter expects one receiver, found {} arguments",
            arguments.len()
        ));
    }
    let sequence = match &arguments[0] {
        Value::Vec(sequence) | Value::Array(sequence) => sequence,
        value => {
            return Err(format!(
                "into_iter receiver is not a collection: {}",
                value.type_name()
            ));
        }
    };
    indexed_iter::reject_mutation(sequence)?;
    if sequence
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.references > 0)
    {
        return Err("cannot iterate a collection while an element is referenced".into());
    }
    if sequence
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.value.is_none())
    {
        return Err("cannot iterate a partially moved collection".into());
    }
    let element_type = sequence
        .element_type
        .borrow()
        .clone()
        .unwrap_or(Type::Unknown);
    let elements = std::mem::take(&mut *sequence.elements.borrow_mut());
    let native = <NativeVec<FieldSlot> as From<Vec<FieldSlot>>>::from(elements);
    let items = call(native)
        .into_inner()
        .ok_or("Vec::into_iter did not return the owned Vec storage")?;
    Ok(Value::OwnedIterator(Rc::new(
        OwnedIteratorValue::from_slots(items, element_type),
    )))
}
