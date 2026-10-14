//! Registered native calls over objects or scoped native projections.

use std::{cell::RefCell, rc::Rc};

use super::{NativeObject, NativeType};
use crate::{DynamicValueMut, DynamicValueRef};

pub(super) enum Receiver<'a, V> {
    Object(&'a NativeObject<V>),
    View(DynamicValueRef<'a>),
    Mutable(RefCell<DynamicValueMut<'a>>),
}

/// The receiver and arguments visible to one registered native operation.
pub struct NativeCallContext<'a, V> {
    pub(super) receiver: Receiver<'a, V>,
    pub(super) descriptor: &'a Rc<NativeType<V>>,
    pub(super) arguments: &'a [V],
}

impl<V: 'static> NativeCallContext<'_, V> {
    pub fn arguments(&self) -> &[V] {
        self.arguments
    }

    pub fn receiver<T: 'static, R>(&self, callback: impl FnOnce(&T) -> R) -> Result<R, String> {
        match &self.receiver {
            Receiver::Object(object) => object.with(callback),
            Receiver::View(view) => view.with_rust(callback),
            Receiver::Mutable(view) => view
                .try_borrow()
                .map_err(|_| "native receiver is already mutably accessed".to_owned())?
                .view()
                .with_rust(callback),
        }
    }

    pub fn receiver_mut<T: 'static, R>(
        &self,
        callback: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        match &self.receiver {
            Receiver::Object(object) => object.with_mut(callback),
            Receiver::View(_) => Err("native receiver requires a mutable reference".into()),
            Receiver::Mutable(view) => view
                .try_borrow_mut()
                .map_err(|_| "native receiver is already accessed".to_owned())?
                .with_rust_mut(callback),
        }
    }

    pub fn new_object<T: 'static>(&self, value: T) -> Result<NativeObject<V>, String> {
        NativeObject::new(self.descriptor.clone(), value)
    }
}

impl<V: 'static> NativeType<V> {
    pub fn call_view(
        self: &Rc<Self>,
        receiver: DynamicValueRef<'_>,
        name: &str,
        arguments: &[V],
    ) -> Option<Result<V, String>> {
        let method = self.methods.get(name)?;
        Some((|| {
            self.check_view(&receiver)?;
            method(&NativeCallContext {
                receiver: Receiver::View(receiver),
                descriptor: self,
                arguments,
            })
        })())
    }

    pub fn call_view_mut(
        self: &Rc<Self>,
        receiver: DynamicValueMut<'_>,
        name: &str,
        arguments: &[V],
    ) -> Option<Result<V, String>> {
        let method = self.methods.get(name)?;
        Some((|| {
            self.check_view(&receiver.view())?;
            method(&NativeCallContext {
                receiver: Receiver::Mutable(RefCell::new(receiver)),
                descriptor: self,
                arguments,
            })
        })())
    }

    fn check_view(&self, receiver: &DynamicValueRef<'_>) -> Result<(), String> {
        if !self
            .composition
            .layout
            .compatible_with(receiver.layout()?.as_ref())
        {
            return Err("native receiver layout does not match its registered type".into());
        }
        Ok(())
    }
}
