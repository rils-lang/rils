//! Preserve registered native identities and inspect their composed children.
use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicValueRef};

use super::NativeRecordCodec;
use crate::{
    Type,
    value::{NativeType, Value},
};

impl NativeRecordCodec {
    pub(crate) fn registered_type(
        &self,
        layout: &DynamicLayout,
    ) -> Result<Option<Rc<NativeType>>, String> {
        let descriptor = self
            .declarations
            .borrow()
            .native_types
            .get(layout.rils_type())
            .cloned();
        let Some(descriptor) = descriptor else {
            return Ok(None);
        };
        if !descriptor.layout_handle().compatible_with(layout) {
            return Err("composed native leaf has a conflicting layout".into());
        }
        Ok(Some(descriptor))
    }

    pub(super) fn resolve_registered_layout(
        &self,
        ty: &Type,
        hosts: &[Rc<crate::value::HostType>],
    ) -> Result<Rc<DynamicLayout>, String> {
        let provider = RegisteredLayouts {
            codec: self,
            hosts: crate::value::runtime_layouts::HostLayoutProvider::new(hosts),
        };
        let structs = self
            .declarations
            .borrow()
            .structs
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let enums = self
            .declarations
            .borrow()
            .enums
            .values()
            .cloned()
            .collect::<Vec<_>>();
        crate::value::record_layout::RecordLayoutResolver::with_provider(
            &structs,
            &enums,
            Some(&provider),
        )
        .resolve(ty)
    }
    pub fn register_native_type(&mut self, descriptor: Rc<NativeType>) -> Result<(), String> {
        let ty = descriptor.rils_type();
        if !ty.is_concrete_type() {
            return Err(format!(
                "native registration requires a concrete type: {ty}"
            ));
        }
        let declarations = self.declarations.get_mut();
        if let Some(existing) = declarations.native_types.get(ty) {
            if !Rc::ptr_eq(existing, &descriptor) && !existing.same_registration(&descriptor) {
                return Err(format!("conflicting native registration for {ty}"));
            }
        } else {
            declarations.native_types.insert(ty.clone(), descriptor);
        }
        Ok(())
    }

    pub(crate) fn retain_value(&mut self, value: &Value) -> Result<(), String> {
        match value {
            Value::Native(object) => self.register_native_type(object.descriptor_handle()),
            Value::Dynamic(object) => {
                if let Some(source) = object.descriptor().metadata::<Self>() {
                    self.retain_layout_declarations(&source, object.descriptor().layout())?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn layout_for_value(
        &mut self,
        value: &Value,
        expected: &Type,
        hosts: &[Rc<crate::value::HostType>],
    ) -> Result<Rc<DynamicLayout>, String> {
        if !expected.accepts(value) {
            return Err(format!(
                "expected native {expected}, found {}",
                value.type_name()
            ));
        }
        if value.is_partially_moved() || value.has_active_references() {
            return Err("cannot compose a referenced or partially moved value".into());
        }
        let layout = match value {
            Value::Native(object) => {
                // Standard-library declarations may project a wrapper to a
                // different canonical leaf (Number<i32> -> i32, for example).
                // Keep that declared representation and its generated codec.
                let native = object.descriptor().layout_handle();
                let mut declarations = self.clone();
                declarations.register_native_type(object.descriptor_handle())?;
                let layout = declarations.resolve_layout(expected, hosts)?;
                if layout.compatible_with(&native) {
                    self.register_native_type(object.descriptor_handle())?;
                }
                layout
            }
            Value::Dynamic(object) => {
                self.retain_value(value)?;
                object.descriptor().layout_handle()
            }
            _ => return self.resolve_layout(expected, hosts),
        };
        if layout.rils_type() != expected {
            return self.resolve_layout(expected, hosts);
        }
        Ok(layout)
    }

    pub(crate) fn any_native_child(
        &self,
        view: DynamicValueRef<'_>,
        predicate: &mut dyn FnMut(&Value) -> bool,
        state: NativeChildState,
    ) -> Result<bool, String> {
        if !self
            .declarations
            .borrow()
            .native_types
            .values()
            .any(|descriptor| descriptor.has_children())
        {
            return Ok(false);
        }
        view.any_leaf(&mut |view| {
            let layout = view.layout()?;
            let descriptor = self
                .declarations
                .borrow()
                .native_types
                .get(layout.rils_type())
                .cloned();
            let Some(descriptor) = descriptor else {
                return Ok(false);
            };
            if !descriptor.layout_handle().compatible_with(&layout) {
                return Err("composed native leaf has a conflicting layout".into());
            }
            let leaf = view.leaf()?;
            let mut found = match state {
                NativeChildState::None => false,
                NativeChildState::References => descriptor.leaf_has_active_references(&leaf)?,
                NativeChildState::Partial => descriptor.leaf_is_partially_moved(&leaf)?,
            };
            descriptor.visit_leaf_children(&leaf, &mut |value| found |= predicate(value))?;
            Ok(found)
        })
    }
}

struct RegisteredLayouts<'a> {
    codec: &'a NativeRecordCodec,
    hosts: crate::value::runtime_layouts::HostLayoutProvider<'a>,
}
impl crate::value::record_layout::NativeLayoutProvider for RegisteredLayouts<'_> {
    fn layout(
        &self,
        ty: &Type,
        resolve: &mut dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String>,
    ) -> Option<Result<Rc<DynamicLayout>, String>> {
        let descriptor = self
            .codec
            .declarations
            .borrow()
            .native_types
            .get(ty)
            .cloned();
        descriptor
            .map(|descriptor| Ok(descriptor.layout_handle()))
            .or_else(|| self.hosts.layout(ty, resolve))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum NativeChildState {
    None,
    References,
    Partial,
}

pub(crate) fn any_native_child(
    object: &crate::value::DynamicObject,
    mut predicate: impl FnMut(&Value) -> bool,
    state: NativeChildState,
) -> bool {
    let Some(codec) = object.descriptor().metadata::<NativeRecordCodec>() else {
        return false;
    };
    object
        .with(|value| codec.any_native_child(value.view(), &mut predicate, state))
        .and_then(|result| result)
        .unwrap_or(true)
}
