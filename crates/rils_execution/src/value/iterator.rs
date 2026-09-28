use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use rils_value::SequenceIteratorLease;

use crate::types::Type;

use super::{
    DynamicObject, FieldSlot, HashKey, IndexedStorage, MapCollection, ReferenceValue,
    SetCollection, Value,
};

#[derive(Clone)]
pub struct OwnedIteratorValue {
    pub items: RefCell<VecDeque<Value>>,
    slots: Option<RefCell<VecDeque<FieldSlot>>>,
    pub element_type: Type,
    pub source: Option<Rc<IndexedStorage>>,
    pub cursor: std::cell::Cell<usize>,
}

impl OwnedIteratorValue {
    pub fn from_indexed(source: Rc<IndexedStorage>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            slots: None,
            element_type,
            source: Some(source),
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_items(items: VecDeque<Value>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(items),
            slots: None,
            element_type,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_slots(slots: VecDeque<FieldSlot>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            slots: Some(RefCell::new(slots)),
            element_type,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        if let Some(value) = self.items.borrow_mut().pop_front() {
            return Ok(Some(value));
        }
        if let Some(slots) = &self.slots {
            return slots
                .borrow_mut()
                .pop_front()
                .map(|slot| {
                    slot.value
                        .ok_or_else(|| "cannot iterate a partially moved collection".to_owned())
                })
                .transpose();
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
        if let Some(slots) = &self.slots {
            let mut slots = slots.borrow_mut();
            let mut items = self.items.borrow_mut();
            while let Some(slot) = slots.pop_front() {
                items.push_back(
                    slot.value
                        .ok_or_else(|| "cannot iterate a partially moved collection".to_owned())?,
                );
            }
            return Ok(());
        }
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
            || self.slots.as_ref().is_some_and(|slots| {
                slots
                    .borrow()
                    .iter()
                    .filter_map(|slot| slot.value.as_ref())
                    .any(Value::contains_reference)
            })
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

pub struct BorrowedIndexedIteratorValue {
    pub source: Rc<ReferenceValue>,
    pub storage: IndexedIteratorStorage,
    pub index: std::cell::Cell<usize>,
    pub length: usize,
    pub element_type: Type,
}

pub enum IndexedIteratorStorage {
    Legacy(Rc<IndexedStorage>),
    Native {
        object: DynamicObject,
        _lease: SequenceIteratorLease,
    },
}

impl BorrowedIndexedIteratorValue {
    pub fn next(&self) -> Result<Option<Value>, String> {
        match (&self.storage, self.source.read()?) {
            (
                IndexedIteratorStorage::Legacy(storage),
                Value::Array(source) | Value::Vec(source),
            ) if Rc::ptr_eq(storage, &source) => {}
            (IndexedIteratorStorage::Native { object, .. }, Value::Dynamic(source))
                if object.same_storage(&source) => {}
            _ => return Err("iterator source has been replaced".into()),
        }
        let index = self.index.get();
        if index >= self.length {
            return Ok(None);
        }
        let reference = match &self.storage {
            IndexedIteratorStorage::Legacy(storage) => ReferenceValue::new_guarded_indexed_element(
                storage.clone(),
                index,
                false,
                Some(self.source.clone()),
            )?,
            IndexedIteratorStorage::Native { object, .. } => {
                ReferenceValue::new_guarded_dynamic_indexed_element(
                    object.clone(),
                    index,
                    false,
                    Some(self.source.clone()),
                )?
            }
        };
        self.index.set(index + 1);
        Ok(Some(Value::Reference(Rc::new(reference))))
    }
}

impl Drop for BorrowedIndexedIteratorValue {
    fn drop(&mut self) {
        if let IndexedIteratorStorage::Legacy(storage) = &self.storage {
            storage
                .active_iterators
                .set(storage.active_iterators.get().saturating_sub(1));
        }
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
        Ok(Some(Value::Tuple(Rc::new(IndexedStorage {
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
