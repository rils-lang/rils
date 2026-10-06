use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use rils_value::{DynamicValue, SequenceIteratorLease};

use crate::types::Type;

use super::record_codec::NativeRecordCodec;
use super::{
    FieldSlot, HashKey, IndexedStorage, MapCollection, ReferenceValue, SetCollection, Value,
};

#[derive(Clone)]
pub struct OwnedIteratorValue {
    pub items: RefCell<VecDeque<Value>>,
    native_items: Option<Rc<RefCell<std::vec::IntoIter<DynamicValue>>>>,
    native_codec: Option<Rc<NativeRecordCodec>>,
    generated_items: Option<Rc<RefCell<Box<dyn Iterator<Item = Value>>>>>,
    slots: Option<RefCell<std::vec::IntoIter<FieldSlot>>>,
    pub element_type: Type,
    pub iterator_type: Option<Type>,
    pub source: Option<Rc<IndexedStorage>>,
    pub cursor: std::cell::Cell<usize>,
}

impl OwnedIteratorValue {
    pub fn from_indexed(source: Rc<IndexedStorage>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            native_items: None,
            native_codec: None,
            generated_items: None,
            slots: None,
            element_type,
            iterator_type: None,
            source: Some(source),
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_items(items: VecDeque<Value>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(items),
            native_items: None,
            native_codec: None,
            generated_items: None,
            slots: None,
            element_type,
            iterator_type: None,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_slots(slots: std::vec::IntoIter<FieldSlot>, element_type: Type) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            native_items: None,
            native_codec: None,
            generated_items: None,
            slots: Some(RefCell::new(slots)),
            element_type,
            iterator_type: None,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_native(
        items: Vec<DynamicValue>,
        element_type: Type,
        codec: NativeRecordCodec,
    ) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            native_items: Some(Rc::new(RefCell::new(items.into_iter()))),
            native_codec: Some(Rc::new(codec)),
            generated_items: None,
            slots: None,
            element_type,
            iterator_type: None,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn from_generator(
        items: impl Iterator<Item = Value> + 'static,
        element_type: Type,
    ) -> Self {
        Self {
            items: RefCell::new(VecDeque::new()),
            native_items: None,
            native_codec: None,
            generated_items: Some(Rc::new(RefCell::new(Box::new(items)))),
            slots: None,
            element_type,
            iterator_type: None,
            source: None,
            cursor: std::cell::Cell::new(0),
        }
    }

    pub fn with_iterator_type(mut self, iterator_type: Type) -> Self {
        self.iterator_type = Some(iterator_type);
        self
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        if let Some(value) = self.items.borrow_mut().pop_front() {
            return Ok(Some(value));
        }
        if let Some(items) = &self.native_items {
            return items
                .borrow_mut()
                .next()
                .map(|item| {
                    self.native_codec
                        .as_ref()
                        .expect("native iterator has a codec")
                        .from_native(item)
                })
                .transpose();
        }
        if let Some(items) = &self.generated_items {
            return Ok(items.borrow_mut().next());
        }
        if let Some(slots) = &self.slots {
            return slots
                .borrow_mut()
                .next()
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
        if let Some(native_items) = &self.native_items {
            let codec = self
                .native_codec
                .as_ref()
                .expect("native iterator has a codec");
            let mut native_items = native_items.borrow_mut();
            let mut items = self.items.borrow_mut();
            for item in native_items.by_ref() {
                items.push_back(codec.from_native(item)?);
            }
            return Ok(());
        }
        if let Some(generated_items) = &self.generated_items {
            self.items
                .borrow_mut()
                .extend(generated_items.borrow_mut().by_ref());
            return Ok(());
        }
        if let Some(slots) = &self.slots {
            let mut slots = slots.borrow_mut();
            let mut items = self.items.borrow_mut();
            for slot in slots.by_ref() {
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
        ((self.native_items.is_some() || self.generated_items.is_some())
            && self.element_type.contains_reference())
            || self.items.borrow().iter().any(Value::contains_reference)
            || self.slots.as_ref().is_some_and(|slots| {
                slots
                    .borrow()
                    .as_slice()
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
    pub map_entries: bool,
    pub native_codec: Option<Rc<NativeRecordCodec>>,
}

pub enum IndexedIteratorStorage {
    Legacy(Rc<IndexedStorage>),
    Native {
        ledger: Rc<rils_value::SequenceBorrowLedger>,
        _lease: SequenceIteratorLease,
    },
}

impl BorrowedIndexedIteratorValue {
    pub fn item_type(&self) -> Type {
        if self.map_entries {
            let Type::Tuple(fields) = &self.element_type else {
                unreachable!("map entry has a pair layout")
            };
            Type::Tuple(
                fields
                    .iter()
                    .map(|field| Type::Reference {
                        mutable: false,
                        inner: Box::new(field.clone()),
                    })
                    .collect(),
            )
        } else {
            Type::Reference {
                mutable: false,
                inner: Box::new(self.element_type.clone()),
            }
        }
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        match &self.storage {
            IndexedIteratorStorage::Legacy(storage) => match self.source.read()? {
                Value::Array(source) | Value::Vec(source) if Rc::ptr_eq(storage, &source) => {}
                _ => return Err("iterator source has been replaced".into()),
            },
            IndexedIteratorStorage::Native { ledger, .. } => {
                let current = self
                    .source
                    .with_native_view(|view| view.sequence_borrows())??;
                if !Rc::ptr_eq(ledger, &current) {
                    return Err("iterator source has been replaced".into());
                }
            }
        }
        let index = self.index.get();
        if index >= self.length {
            return Ok(None);
        }
        if self.map_entries {
            let IndexedIteratorStorage::Native { .. } = &self.storage else {
                return Err("map iterator has no native entry storage".into());
            };
            let fields = (0..2)
                .map(|field| {
                    let entry = Rc::new(
                        self.source
                            .project_native_index_with_codec(index, self.native_codec.clone())?
                            .ok_or("map entry has no native path")?,
                    );
                    let reference = entry
                        .project_native_field(&field.to_string())?
                        .ok_or("map entry field has no native path")?;
                    reference
                        .reborrow(false)
                        .map(|reference| Value::Reference(Rc::new(reference)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let types = match self.item_type() {
                Type::Tuple(types) => types,
                _ => unreachable!(),
            };
            let slots = fields
                .into_iter()
                .zip(types)
                .map(|(value, type_annotation)| FieldSlot::new(type_annotation, value))
                .collect();
            self.index.set(index + 1);
            return Ok(Some(Value::Tuple(Rc::new(IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: RefCell::new(slots),
                element_type: RefCell::new(None),
            }))));
        }
        let reference = match &self.storage {
            IndexedIteratorStorage::Legacy(storage) => ReferenceValue::new_guarded_indexed_element(
                storage.clone(),
                index,
                false,
                Some(self.source.clone()),
            )?,
            IndexedIteratorStorage::Native { .. } => self
                .source
                .project_native_index_with_codec(index, self.native_codec.clone())?
                .ok_or("iterator element has no native path")?
                .reborrow(false)?,
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
                FieldSlot::new(types[0].clone(), key_ref),
                FieldSlot::new(types[1].clone(), value_ref),
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
