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
    pub(crate) projection: BorrowedIteratorProjection,
    pub iterator_type: Type,
    pub native_codec: Option<Rc<NativeRecordCodec>>,
}

/// The declaration determines which stored type parameter each reference projects.
pub(crate) enum BorrowedIteratorProjection {
    Reference { path: Vec<usize>, inner: Type },
    Tuple(Vec<Self>),
}

impl BorrowedIteratorProjection {
    pub(crate) fn item_type(&self) -> Type {
        match self {
            Self::Reference { inner, .. } => Type::Reference {
                mutable: false,
                inner: Box::new(inner.clone()),
            },
            Self::Tuple(fields) => Type::Tuple(fields.iter().map(Self::item_type).collect()),
        }
    }

    fn project(&self, entry: &Rc<ReferenceValue>) -> Result<Value, String> {
        match self {
            Self::Reference { path, .. } => {
                let mut source = entry.clone();
                for index in path {
                    source = Rc::new(
                        source
                            .project_native_field(&index.to_string())?
                            .ok_or("iterator field has no native path")?,
                    );
                }
                source
                    .reborrow(false)
                    .map(|reference| Value::from_reference(Rc::new(reference)))
            }
            Self::Tuple(fields) => {
                let values = fields
                    .iter()
                    .map(|field| field.project(entry))
                    .collect::<Result<Vec<_>, _>>()?;
                super::storage::TypedStorageContext::new(&[], &[])
                    .construct_tuple(&self.item_type(), values)
            }
        }
    }
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
        self.projection.item_type()
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
        let entry = Rc::new(
            self.source
                .project_native_index_with_codec(index, self.native_codec.clone())?
                .ok_or("iterator element has no native path")?,
        );
        let item = self.projection.project(&entry)?;
        self.index.set(index + 1);
        Ok(Some(item))
    }
}
