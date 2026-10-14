//! Explicit structural cloning retains the registered operations of native leaves.

use rils_value::{DynamicValue, DynamicValueRef};

use super::{NativeRecordCodec, Value};

impl NativeRecordCodec {
    pub(crate) fn clone_view(&self, view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        if super::super::runtime_layouts::contains_noncopy_callable(view.clone())? {
            return Err("cannot clone a callable with a non-Copy bound receiver".into());
        }
        if view.is_partially_moved()?
            || self.any_native_child(
                view.clone(),
                &mut Value::is_partially_moved,
                super::native::NativeChildState::Partial,
            )?
        {
            return Err("cannot clone a partially moved native value".into());
        }
        super::super::runtime_layouts::clone_borrowed_view_with(view, &mut |leaf| {
            let descriptor = match leaf
                .layout()
                .and_then(|layout| self.registered_type(&layout))
            {
                Ok(Some(descriptor)) => descriptor,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            Some((|| {
                let cloned = descriptor
                    .call_view(leaf.clone(), super::super::native_ops::CLONE, &[])
                    .ok_or_else(|| {
                        format!(
                            "native type {} does not implement Clone",
                            descriptor.rils_type()
                        )
                    })??;
                let Value::Native(cloned) = cloned else {
                    return Err("native Clone must return a native value".into());
                };
                if !descriptor.same_registration(cloned.descriptor()) {
                    return Err("native Clone returned a different registered type".into());
                }
                cloned.into_dynamic().map_err(|error| error.1)
            })())
        })
    }
}
