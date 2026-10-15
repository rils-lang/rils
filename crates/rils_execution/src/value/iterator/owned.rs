//! Owned iteration keeps native elements and a complete output layout until exhaustion.
use std::{cell::RefCell, rc::Rc};

use rils_value::{DynamicLayout, DynamicValue, OwnedElements};

use crate::{
    Type, Value,
    value::{
        NativeChildren,
        record_codec::{NativeRecordCodec, native::NativeChildState},
        storage::PreparedOption,
    },
};

enum Source {
    Native(RefCell<OwnedElements>),
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
        Self::from_cursor(OwnedElements::from_values(item_layout, items)?, codec)
    }

    pub fn from_cursor(items: OwnedElements, codec: NativeRecordCodec) -> Result<Self, String> {
        let item_layout = items.item_layout().clone();
        if !item_layout.rils_type().is_concrete_type() {
            return Err("native iterator requires a complete element layout".into());
        }
        Ok(Self {
            element_type: item_layout.rils_type().clone(),
            item_layout,
            codec,
            source: Source::Native(RefCell::new(items)),
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
                    .next()
                    .transpose()?,
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
                .transpose()?
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
            items
                .borrow()
                .any_remaining(|view| {
                    crate::value::runtime_layouts::visit_execution_handles(view.clone(), visit)?;
                    self.codec.any_native_child(
                        view,
                        &mut |value| {
                            visit(value);
                            false
                        },
                        NativeChildState::None,
                    )?;
                    Ok(false)
                })
                .expect("iterator retains valid native storage");
        }
    }

    fn has_active_references(&self) -> bool {
        if let Source::Native(items) = &self.source {
            let items = items.borrow();
            return items.has_active_references()
                || items
                    .any_remaining(|view| {
                        self.codec.any_native_child(
                            view,
                            &mut Value::has_active_references,
                            NativeChildState::References,
                        )
                    })
                    .unwrap_or(true);
        }
        false
    }

    fn is_partially_moved(&self) -> bool {
        if let Source::Native(items) = &self.source {
            return items
                .borrow()
                .any_remaining(|view| {
                    Ok(view.is_partially_moved()?
                        || self.codec.any_native_child(
                            view,
                            &mut Value::is_partially_moved,
                            NativeChildState::Partial,
                        )?)
                })
                .unwrap_or(true);
        }
        false
    }
}
