//! Referents are inspected while their original storage stays borrowed.

use super::super::borrowed::{Read, with_raw_read, with_referent};
use super::{ReferenceTarget, ReferenceValue};

impl super::Value {
    /// Inspect a host declaration through storage or references without cloning its value.
    pub fn host_type_definition(
        &self,
    ) -> Result<Option<std::rc::Rc<super::super::HostType>>, String> {
        with_referent(self, host_type)?
    }
}

fn host_type(value: Read<'_>) -> Result<Option<std::rc::Rc<super::super::HostType>>, String> {
    use super::super::HostObject;
    use std::rc::Rc;
    match value {
        Read::Host(object) => Ok(Some(object.type_definition.clone())),
        Read::View(view) if view.layout()?.is_rust_type::<Rc<HostObject>>() => {
            view.with_rust::<Rc<HostObject>, _>(|object| Some(object.type_definition.clone()))
        }
        Read::Leaf(leaf) if leaf.is_rust_type::<Rc<HostObject>>() => {
            leaf.with_rust::<Rc<HostObject>, _>(|object| Some(object.type_definition.clone()))
        }
        _ => Ok(None),
    }
}

impl ReferenceValue {
    pub(crate) fn referent_type(&self) -> Result<crate::Type, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) => Ok(view.layout()?.rils_type().clone()),
            Read::Leaf(leaf) => Ok(leaf.rils_type().clone()),
            Read::Host(object) => Ok(crate::Type::named(&object.type_definition.name)),
        })?
    }

    pub(crate) fn accepts_referent(&self, expected: &crate::Type) -> Result<bool, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) => super::super::runtime_layouts::accepts_view(view, expected),
            Read::Leaf(leaf) => Ok(crate::types::merge_types(expected, leaf.rils_type()).is_some()),
            Read::Host(object) => Ok(crate::runtime_type::accepts_host(expected, object)),
        })?
    }

    /// Inspect one reference target, preserving any reference stored inside it.
    pub(crate) fn with_borrowed_target<R>(
        &self,
        callback: impl for<'a> FnOnce(Read<'a>) -> R,
    ) -> Result<R, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| with_raw_read(value, callback)),
            ReferenceTarget::DynamicIndexedElement { .. }
            | ReferenceTarget::DynamicCell { .. }
            | ReferenceTarget::DynamicField(_) => {
                self.with_native_view(|view| callback(Read::View(view)))
            }
        }
    }
}
