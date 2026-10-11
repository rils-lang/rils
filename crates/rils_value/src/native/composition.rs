//! Ownership conversion between registered Rust leaves and composed layouts.

use std::rc::Rc;

use rils_syntax::Type;

use super::{NativeObject, NativeType};
use crate::{DynamicLayout, DynamicValue};

type Encode<V> = fn(NativeObject<V>) -> Result<DynamicValue, Box<(NativeObject<V>, String)>>;
type Decode<V> =
    fn(Rc<NativeType<V>>, DynamicValue) -> Result<NativeObject<V>, Box<(DynamicValue, String)>>;

pub(super) struct NativeComposition<V> {
    pub(super) layout: Rc<DynamicLayout>,
    encode: Encode<V>,
    decode: Decode<V>,
}

impl<V: 'static> NativeComposition<V> {
    pub(super) fn new<T: 'static>(ty: Type) -> Self {
        Self {
            layout: DynamicLayout::of::<T>(ty),
            encode: |object| {
                let layout = object.descriptor.composition.layout.clone();
                let payload = object.into_rust::<T>()?;
                Ok(DynamicValue::from_rust(layout, payload)
                    .expect("registered layout and native payload have the same Rust type"))
            },
            decode: |descriptor, value| {
                let payload = value.into_rust::<T>().map_err(Box::new)?;
                Ok(NativeObject::new(descriptor, payload)
                    .expect("registered decoder and native descriptor have the same Rust type"))
            },
        }
    }
}

impl<V: 'static> NativeType<V> {
    /// Physical layout registered by `new::<T>`, including the explicit Copy
    /// policy. This stores T itself, without a NativeObject or V wrapper.
    /// Operation and child visitor registrations remain on this NativeType;
    /// callers composing leaves must retain that descriptor alongside layouts.
    pub fn layout_handle(&self) -> Rc<DynamicLayout> {
        self.composition.layout.clone()
    }

    /// Recover a native handle after moving its leaf out of composed storage.
    /// A failed layout check returns the original owner unchanged. The Rils
    /// identity, Rust representation and Copy policy must all match.
    pub fn from_dynamic(
        self: &Rc<Self>,
        value: DynamicValue,
    ) -> Result<NativeObject<V>, Box<(DynamicValue, String)>> {
        if !self.composition.layout.compatible_with(value.descriptor()) {
            return Err(Box::new((
                value,
                format!(
                    "native layout does not match descriptor for {}",
                    self.rils_type
                ),
            )));
        }
        (self.composition.decode)(self.clone(), value)
    }
}

impl<V: 'static> NativeObject<V> {
    /// Retain operations and child visitors while the payload is composed.
    pub fn descriptor_handle(&self) -> Rc<NativeType<V>> {
        self.descriptor.clone()
    }

    /// Move the actual Rust payload into a composable leaf, without Clone.
    /// Shared Copy backing is detached; shared non-Copy backing is returned
    /// unchanged. Partially moved payloads and active child references also
    /// reject the move without consuming the original owner.
    pub fn into_dynamic(self) -> Result<DynamicValue, Box<(Self, String)>> {
        if self.has_active_references() {
            return Err(Box::new((
                self,
                "cannot move a referenced native payload".into(),
            )));
        }
        if self.is_partially_moved() {
            return Err(Box::new((
                self,
                "cannot move a partially moved native payload".into(),
            )));
        }
        let object = self.into_unique()?;
        (object.descriptor.composition.encode)(object)
    }
}
