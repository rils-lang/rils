//! Owned iteration keeps native elements and a complete output layout until exhaustion.
use std::{cell::RefCell, rc::Rc};

use rils_value::{DynamicLayout, DynamicValue};

use crate::{
    Type, Value,
    value::{
        NativeChildren,
        record_codec::{NativeRecordCodec, native::NativeChildState},
        storage::PreparedOption,
    },
};

enum Source {
    Native(RefCell<std::vec::IntoIter<DynamicValue>>),
    // Only execution-owned, reference-free standard-library generators use this path.
    Generated(RefCell<Box<dyn Iterator<Item = Result<Value, String>>>>),
}

pub struct OwnedIteratorValue {
    source: Source,
    codec: NativeRecordCodec,
    item_layout: Rc<DynamicLayout>,
    pub element_type: Type,
    pub iterator_type: Option<Type>,
}

impl OwnedIteratorValue {
    pub fn from_native(
        items: Vec<DynamicValue>,
        item_layout: Rc<DynamicLayout>,
        codec: NativeRecordCodec,
    ) -> Result<Self, String> {
        if !item_layout.rils_type().is_concrete_type() {
            return Err("native iterator requires a complete element layout".into());
        }
        for item in &items {
            if !item.descriptor().compatible_with(&item_layout) {
                return Err("iterator element has a different native layout".into());
            }
            if item.view().is_partially_moved()? {
                return Err("cannot iterate partially moved native elements".into());
            }
        }
        Ok(Self {
            element_type: item_layout.rils_type().clone(),
            item_layout,
            codec,
            source: Source::Native(RefCell::new(items.into_iter())),
            iterator_type: None,
        })
    }

    pub(crate) fn from_generator(
        items: impl Iterator<Item = Result<Value, String>> + 'static,
        element_type: Type,
    ) -> Result<Self, String> {
        let codec = NativeRecordCodec::default();
        let item_layout = codec.resolve_layout(&element_type, &[])?;
        Ok(Self {
            source: Source::Generated(RefCell::new(Box::new(items))),
            codec,
            item_layout,
            element_type,
            iterator_type: None,
        })
    }

    pub fn with_iterator_type(mut self, ty: Type) -> Self {
        self.iterator_type = Some(ty);
        self
    }

    pub fn next_option(&self) -> Result<Value, String> {
        let output = PreparedOption::from_layout(self.item_layout.clone(), self.codec.clone())?;
        match &self.source {
            Source::Native(items) => output.construct_native(
                items
                    .try_borrow_mut()
                    .map_err(|_| "iterator is already advancing")?
                    .next(),
            ),
            Source::Generated(items) => output.construct(
                items
                    .try_borrow_mut()
                    .map_err(|_| "iterator is already advancing")?
                    .next()
                    .transpose()?,
            ),
        }
    }

    pub fn next(&self) -> Result<Option<Value>, String> {
        match &self.source {
            Source::Native(items) => items
                .try_borrow_mut()
                .map_err(|_| "iterator is already advancing")?
                .next()
                .map(|item| self.codec.from_native(item))
                .transpose(),
            Source::Generated(items) => items
                .try_borrow_mut()
                .map_err(|_| "iterator is already advancing")?
                .next()
                .transpose(),
        }
    }
}

impl NativeChildren<Value> for OwnedIteratorValue {
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        if let Source::Native(items) = &self.source {
            for item in items.borrow().as_slice() {
                crate::value::runtime_layouts::visit_execution_handles(item.view(), visit)
                    .expect("iterator retains valid native storage");
                self.codec
                    .any_native_child(
                        item.view(),
                        &mut |value| {
                            visit(value);
                            false
                        },
                        NativeChildState::None,
                    )
                    .expect("iterator retains registered native children");
            }
        }
    }

    fn has_active_references(&self) -> bool {
        if let Source::Native(items) = &self.source {
            return items.borrow().as_slice().iter().any(|item| {
                item.has_path_references()
                    || self
                        .codec
                        .any_native_child(
                            item.view(),
                            &mut Value::has_active_references,
                            NativeChildState::References,
                        )
                        .unwrap_or(true)
            });
        }
        false
    }

    fn is_partially_moved(&self) -> bool {
        if let Source::Native(items) = &self.source {
            return items.borrow().as_slice().iter().any(|item| {
                item.view().is_partially_moved().unwrap_or(true)
                    || self
                        .codec
                        .any_native_child(
                            item.view(),
                            &mut Value::is_partially_moved,
                            NativeChildState::Partial,
                        )
                        .unwrap_or(true)
            });
        }
        false
    }
}
