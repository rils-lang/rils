//! Constructors for collections whose element layout is inferred from the call site.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    Type, Value,
    value::{
        DynamicObject, HashMapValue, HashSetValue, IndexedStorage, record_codec::NativeRecordCodec,
        record_layout::RecordLayoutResolver,
    },
};

use rils_value::{DynamicType, DynamicValue};

use super::NativeOwnedContext;

pub(super) fn empty(owner: &str) -> Result<Value, String> {
    let value = match owner {
        "Vec" => Value::Vec(Rc::new(IndexedStorage {
            active_iterators: Cell::new(0),
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        })),
        "HashMap" => Value::HashMap(Rc::new(HashMapValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(HashMap::new()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        })),
        "HashSet" => Value::HashSet(Rc::new(HashSetValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(HashSet::new()),
            element_type: RefCell::new(Type::Unknown),
        })),
        _ => return Err(format!("{owner} has no empty collection constructor")),
    };
    Ok(value)
}

pub(super) fn from_array(
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!(
            "Vec::from expects one array, found {} arguments",
            arguments.len()
        ));
    }
    let Value::Array(array) = arguments.pop().expect("arity checked") else {
        return Err("Vec::from expects an array".into());
    };
    if array
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.references > 0)
    {
        return Err("cannot move an array into Vec while an element is referenced".into());
    }
    let element_type = array.element_type.borrow().clone();
    if let Some(element_type) = &element_type {
        let ty = Type::Named {
            name: "Vec".into(),
            arguments: vec![element_type.clone()],
        };
        let mut resolver = RecordLayoutResolver::with_enums(&context.structs, &context.enums);
        if let Ok(layout) = resolver.resolve(&ty) {
            let item_layout = layout
                .sequence_item()
                .ok_or("Vec layout has no element layout")?
                .clone();
            let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
            let values = array
                .elements
                .borrow_mut()
                .drain(..)
                .map(|slot| {
                    let value = slot.value.ok_or("cannot move a partially moved array")?;
                    codec.into_native(value, item_layout.clone())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let payload = DynamicValue::sequence(layout.clone(), values)?;
            return Ok(Value::Dynamic(DynamicObject::new(
                Rc::new(DynamicType::new(layout)),
                payload,
            )?));
        }
    }
    let elements = array.elements.borrow_mut().drain(..).collect();
    Ok(Value::Vec(Rc::new(IndexedStorage {
        active_iterators: Cell::new(0),
        elements: RefCell::new(elements),
        element_type: RefCell::new(element_type),
    })))
}
