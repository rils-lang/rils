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
        if !value.descriptor().compatible_with(descriptor.layout()) {
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

    /// Keep one shared storage identity even when the enclosed layout is Copy.
    /// Interior-mutable wrappers use this so borrowed views see later writes.
    pub fn new_shared(descriptor: Rc<DynamicType<V>>, value: DynamicValue) -> Result<Self, String> {
        if !value.descriptor().compatible_with(descriptor.layout()) {
            return Err("dynamic value layout does not match its type".into());
        }
        Ok(Self {
            descriptor,
            storage: Storage::Shared(Rc::new(RefCell::new(value))),
        })
    }

    /// Share an erased payload with a runtime that supplies another method table.
    pub fn rebind_shared<U>(&self) -> Result<DynamicObject<U>, String> {
        let Storage::Shared(value) = &self.storage else {
            return Err("dynamic value has no shared storage".into());
        };
        Ok(DynamicObject {
            descriptor: Rc::new(DynamicType::new(self.descriptor.layout_handle())),
            storage: Storage::Shared(value.clone()),
        })
    }

    pub fn descriptor(&self) -> &DynamicType<V> {
        &self.descriptor
    }

    pub fn is_inline(&self) -> bool {
        matches!(self.storage, Storage::Inline(_))
    }

    /// Whether two handles refer to the same mutable native payload.
    pub fn same_storage(&self, other: &Self) -> bool {
        match (&self.storage, &other.storage) {
            (Storage::Shared(left), Storage::Shared(right)) => Rc::ptr_eq(left, right),
            (Storage::Inline(left), Storage::Inline(right)) => std::ptr::eq(left, right),
            _ => false,
        }
    }

    pub fn copy_owned(&self) -> Result<Self, String> {
        let value = self.with(DynamicValue::copy_owned)??;
        Self::new(self.descriptor.clone(), value)
    }

    /// Consume a uniquely owned handle and transfer its composed bytes.
    /// Shared handles are returned intact so callers can retain the original
    /// value without silently cloning a non-Copy payload.
    pub fn into_value(self) -> Result<DynamicValue, Box<(Self, String)>> {
        let Self {
            descriptor,
            storage,
        } = self;
        match storage {
            Storage::Inline(value) => Ok(value.into_inner()),
            Storage::Shared(value) => match Rc::try_unwrap(value) {
                Ok(value) => Ok(value.into_inner()),
                Err(value) => Err(Box::new((
                    Self {
                        descriptor,
                        storage: Storage::Shared(value),
                    },
                    "cannot move a shared dynamic value".into(),
                ))),
            },
        }
    }

    /// Move out an exact Rust leaf while retaining the original handle when
    /// the payload is shared or has a different Rust type.
    pub fn into_rust<T: 'static>(self) -> Result<T, Box<(Self, String)>> {
        let descriptor = self.descriptor.clone();
        let value = self.into_value()?;
        value.into_rust::<T>().map_err(|(value, message)| {
            Box::new((
                Self::new(descriptor, value).expect("restored payload has its original layout"),
                message,
            ))
        })
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

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut DynamicValue) -> R) -> Result<R, String> {
        let value: &RefCell<DynamicValue> = match &self.storage {
            Storage::Inline(value) => value,
            Storage::Shared(value) => value.as_ref(),
        };
        value
            .try_borrow_mut()
            .map(|mut value| f(&mut value))
            .map_err(|_| "dynamic value is already accessed".to_owned())
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
