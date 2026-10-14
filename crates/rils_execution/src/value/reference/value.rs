//! References use the same native leaf layout standalone and inside aggregates.
use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use super::{ReferenceValue, Value};
use crate::Type;

impl Value {
    pub fn from_reference(reference: Rc<ReferenceValue>) -> Self {
        // A place may temporarily have no readable payload (for example after
        // a move). The lease still exists and subsequent access reports that
        // error; inspecting or copying a handle must not dereference it.
        let ty = Type::Reference {
            mutable: reference.mutable,
            inner: Box::new(reference.referent_type().unwrap_or(Type::Unknown)),
        };
        let layout = DynamicLayout::copy_handle_of::<Rc<ReferenceValue>>(ty);
        let value = DynamicValue::from_rust(layout.clone(), reference)
            .expect("reference layout matches its lease handle");
        Self::Dynamic(
            super::super::DynamicObject::new(Rc::new(DynamicType::new(layout)), value)
                .expect("reference descriptor matches its native payload"),
        )
    }

    /// Copy the lexical lease handle, never the referent or its native bytes.
    pub fn as_reference(&self) -> Option<Rc<ReferenceValue>> {
        let Self::Dynamic(object) = self else {
            return None;
        };
        if !object
            .descriptor()
            .layout()
            .is_rust_type::<Rc<ReferenceValue>>()
        {
            return None;
        }
        object
            .with(|value| value.view().with_rust::<Rc<ReferenceValue>, _>(Rc::clone))
            .ok()?
            .ok()
    }
}
