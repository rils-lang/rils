use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::types::Type;

use super::{
    FieldSlot, HashKey, MapCollection, ReferenceValue, SequenceValue, SetCollection, Value,
};

#[derive(Clone)]
pub struct OwnedIteratorValue {
    pub items: RefCell<VecDeque<Value>>,
    pub element_type: Type,
    pub source: Option<Rc<SequenceValue>>,
    pub cursor: std::cell::Cell<usize>,
}

impl OwnedIteratorValue {
    pub fn from_sequence(source: Rc<SequenceValue>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            element_type,
            source: Some(source),
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_items(items: VecDeque<Value>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(items),
            element_type,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        if let Some(value) = self.items.borrow_mut().pop_front() {
            return Ok(Some(value));
        }
        let Some(source) = &self.source else {
            return Ok(None);
        };
        let index = self.cursor.get();
        let mut elements = source.elements.borrow_mut();
        if index >= elements.len() {
            return Ok(None);
        }
        let value = elements[index]
            .value
            .take()
            .ok_or_else(|| "cannot iterate a partially moved collection".to_owned())?;
        self.cursor.set(index + 1);
        Ok(Some(value))
    }

    pub fn materialize(&self) -> Result<(), String> {
        let Some(source) = &self.source else {
            return Ok(());
        };
        let mut elements = source.elements.borrow_mut();
        let mut items = self.items.borrow_mut();
        for slot in elements.iter_mut().skip(self.cursor.get()) {
            let value = slot
                .value
                .take()
                .ok_or_else(|| "cannot iterate a partially moved collection".to_owned())?;
            items.push_back(value);
            self.cursor.set(self.cursor.get() + 1);
        }
        Ok(())
    }

    pub fn contains_reference(&self) -> bool {
        self.items.borrow().iter().any(Value::contains_reference)
            || self.source.as_ref().is_some_and(|source| {
                source
                    .elements
                    .borrow()
                    .iter()
                    .skip(self.cursor.get())
                    .filter_map(|slot| slot.value.as_ref())
                    .any(Value::contains_reference)
            })
    }
}

pub struct BorrowedSequenceIterValue {
    pub source: Rc<ReferenceValue>,
    pub sequence: Rc<SequenceValue>,
    pub index: std::cell::Cell<usize>,
    pub length: usize,
    pub element_type: Type,
}

impl BorrowedSequenceIterValue {
    pub fn next(&self) -> Result<Option<Value>, String> {
        let (Value::Array(source) | Value::Vec(source)) = self.source.read()? else {
            return Err("iterator source is no longer a sequence".into());
        };
        if !Rc::ptr_eq(&source, &self.sequence) {
            return Err("iterator source has been replaced".into());
        }
        let index = self.index.get();
        if index >= self.length {
            return Ok(None);
        }
        let reference = ReferenceValue::new_guarded_sequence_element(
            self.sequence.clone(),
            index,
            false,
            Some(self.source.clone()),
        )?;
        self.index.set(index + 1);
        Ok(Some(Value::Reference(Rc::new(reference))))
    }
}

impl Drop for BorrowedSequenceIterValue {
    fn drop(&mut self) {
        self.sequence
            .active_iterators
            .set(self.sequence.active_iterators.get().saturating_sub(1));
    }
}

pub struct BorrowedMapIteratorValue {
    pub source: Rc<ReferenceValue>,
    pub map: MapCollection,
    pub keys: Vec<HashKey>,
    pub index: std::cell::Cell<usize>,
    pub key_type: Type,
    pub value_type: Type,
}

impl BorrowedMapIteratorValue {
    pub fn item_type(&self) -> Type {
        Type::Tuple(vec![
            Type::Reference {
                mutable: false,
                inner: Box::new(self.key_type.clone()),
            },
            Type::Reference {
                mutable: false,
                inner: Box::new(self.value_type.clone()),
            },
        ])
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        let index = self.index.get();
        let Some(key) = self.keys.get(index).cloned() else {
            return Ok(None);
        };
        let key_ref = Value::Reference(Rc::new(ReferenceValue::new_map_key(
            self.map.clone(),
            key.clone(),
            Some(self.source.clone()),
        )?));
        let value_ref = Value::Reference(Rc::new(ReferenceValue::new_map_value(
            self.map.clone(),
            key,
            Some(self.source.clone()),
        )?));
        self.index.set(index + 1);
        let item_type = self.item_type();
        let Type::Tuple(types) = item_type else {
            unreachable!()
        };
        Ok(Some(Value::Tuple(Rc::new(SequenceValue {
            active_iterators: std::cell::Cell::new(0),
            elements: RefCell::new(vec![
                FieldSlot {
                    value: Some(key_ref),
                    type_annotation: types[0].clone(),
                    references: 0,
                },
                FieldSlot {
                    value: Some(value_ref),
                    type_annotation: types[1].clone(),
                    references: 0,
                },
            ]),
            element_type: RefCell::new(None),
        }))))
    }
}

impl Drop for BorrowedMapIteratorValue {
    fn drop(&mut self) {
        self.map
            .borrowed()
            .set(self.map.borrowed().get().saturating_sub(1));
    }
}

pub struct BorrowedSetIteratorValue {
    pub source: Rc<ReferenceValue>,
    pub set: SetCollection,
    pub keys: Vec<HashKey>,
    pub index: std::cell::Cell<usize>,
    pub element_type: Type,
}

impl BorrowedSetIteratorValue {
    pub fn item_type(&self) -> Type {
        Type::Reference {
            mutable: false,
            inner: Box::new(self.element_type.clone()),
        }
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        let index = self.index.get();
        let Some(key) = self.keys.get(index).cloned() else {
            return Ok(None);
        };
        let value = Value::Reference(Rc::new(ReferenceValue::new_set_item(
            self.set.clone(),
            key,
            Some(self.source.clone()),
        )?));
        self.index.set(index + 1);
        Ok(Some(value))
    }
}

impl Drop for BorrowedSetIteratorValue {
    fn drop(&mut self) {
        self.set
            .borrowed()
            .set(self.set.borrowed().get().saturating_sub(1));
    }
}
