use std::rc::Rc;

use rils_value::SequenceIteratorLease;

use crate::types::Type;

use super::record_codec::NativeRecordCodec;
use super::{ReferenceValue, Value};

#[path = "iterator/native.rs"]
mod native;

#[path = "iterator/owned.rs"]
mod owned;
pub use owned::OwnedIteratorValue;

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
    NativeArray,
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
            IndexedIteratorStorage::NativeArray => {
                self.source.with_native_view(|view| {
                    if !matches!(view.layout()?.rils_type(), Type::Array { element, length } if **element == self.element_type && *length == self.length) {
                        return Err("iterator source has been replaced".into());
                    }
                    Ok::<_, String>(())
                })??;
            }
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
                        .map(|reference| Value::from_reference(Rc::new(reference)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let pair = super::storage::TypedStorageContext::new(&[], &[])
                .construct_tuple(&self.item_type(), fields)?;
            self.index.set(index + 1);
            return Ok(Some(pair));
        }
        let reference = match &self.storage {
            IndexedIteratorStorage::Native { .. } | IndexedIteratorStorage::NativeArray => self
                .source
                .project_native_index_with_codec(index, self.native_codec.clone())?
                .ok_or("iterator element has no native path")?
                .reborrow(false)?,
        };
        self.index.set(index + 1);
        Ok(Some(Value::from_reference(Rc::new(reference))))
    }
}
