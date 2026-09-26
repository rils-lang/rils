//! Shared or inline handle for values with a runtime-composed layout.

use std::{cell::RefCell, rc::Rc};

use super::{DynamicType, DynamicValue};

enum Storage {
    Inline(RefCell<DynamicValue>),
    Shared(Rc<RefCell<DynamicValue>>),
}

/// A dynamically laid-out value with the same shallow-handle semantics as
/// `NativeObject`. Copy layouts stay inline in the handle.
pub struct DynamicObject<V> {
    descriptor: Rc<DynamicType<V>>,
    storage: Storage,
}

impl<V> Clone for DynamicObject<V> {
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            Storage::Inline(value) => Storage::Inline(RefCell::new(
                value
                    .borrow()
                    .copy_owned()
                    .expect("inline dynamic layout is Copy"),
            )),
            Storage::Shared(value) => Storage::Shared(value.clone()),
        };
        Self {
            descriptor: self.descriptor.clone(),
            storage,
        }
    }
}

impl<V> DynamicObject<V> {
    pub fn new(descriptor: Rc<DynamicType<V>>, value: DynamicValue) -> Result<Self, String> {
        if !value.has_layout(&descriptor.layout_handle()) {
            return Err("dynamic value layout does not match its type".into());
        }
        let storage = if descriptor.layout().is_copy() && value.is_inline() {
            Storage::Inline(RefCell::new(value))
        } else {
            Storage::Shared(Rc::new(RefCell::new(value)))
        };
        Ok(Self {
            descriptor,
            storage,
        })
    }

    pub fn descriptor(&self) -> &DynamicType<V> {
        &self.descriptor
    }

    pub fn is_inline(&self) -> bool {
        matches!(self.storage, Storage::Inline(_))
    }

    pub fn copy_owned(&self) -> Result<Self, String> {
        let value = self.with(DynamicValue::copy_owned)??;
        Self::new(self.descriptor.clone(), value)
    }

    pub fn with<R>(&self, f: impl FnOnce(&DynamicValue) -> R) -> Result<R, String> {
        let value: &RefCell<DynamicValue> = match &self.storage {
            Storage::Inline(value) => value,
            Storage::Shared(value) => value.as_ref(),
        };
        value
            .try_borrow()
            .map(|value| f(&value))
            .map_err(|_| "dynamic value is already mutably accessed".to_owned())
    }

    pub fn call(&self, name: &str, arguments: &[V]) -> Option<Result<V, String>> {
        if !self.descriptor.has_method(name) {
            return None;
        }
        let value: &RefCell<DynamicValue> = match &self.storage {
            Storage::Inline(value) => value,
            Storage::Shared(value) => value.as_ref(),
        };
        Some(
            value
                .try_borrow_mut()
                .map_err(|_| "dynamic value is already accessed".to_owned())
                .and_then(|mut value| {
                    self.descriptor
                        .call(&mut value, name, arguments)
                        .expect("method was checked")
                }),
        )
    }
}
