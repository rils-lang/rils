//! Results follow language parameter indices, never entry field positions.
use super::call::Call;
use crate::{
    Value,
    value::{OwnedIteratorValue, storage::PreparedOption},
};
use rils_stdlib::storage::OwnedSequence;
use rils_value::DynamicValue;
use std::rc::Rc;
#[derive(Clone, Copy)]
pub(crate) enum ResultShape {
    Unit,
    Bool,
    Usize,
    Element(usize),
    Optional(usize),
    Owned,
    Iterator(usize),
}
pub(crate) trait StorageOutput {
    fn into_value(self, shape: ResultShape, call: &Call<'_>) -> Result<Value, String>;
}
macro_rules! scalar {
    ($ty:ty, $shape:ident, $native:ident) => {
        impl StorageOutput for $ty {
            fn into_value(self, shape: ResultShape, _: &Call<'_>) -> Result<Value, String> {
                if !matches!(shape, ResultShape::$shape) {
                    return Err("storage result shape mismatch".into());
                }
                Ok(crate::value::$native(self).into())
            }
        }
    };
}
scalar!(bool, Bool, native_bool);
scalar!(usize, Usize, native_usize);
impl StorageOutput for () {
    fn into_value(self, shape: ResultShape, _: &Call<'_>) -> Result<Value, String> {
        if !matches!(shape, ResultShape::Unit) {
            return Err("storage result shape mismatch".into());
        }
        Ok(crate::value::native_unit().into())
    }
}
impl StorageOutput for DynamicValue {
    fn into_value(self, shape: ResultShape, call: &Call<'_>) -> Result<Value, String> {
        let ResultShape::Element(parameter) = shape else {
            return Err("storage result shape mismatch".into());
        };
        let layout = call.parameter_layout(parameter)?;
        if !layout.compatible_with(self.descriptor()) {
            return Err("sequence method returned a different element layout".into());
        }
        call.services.0.from_native(self)
    }
}
impl StorageOutput for Option<DynamicValue> {
    fn into_value(self, shape: ResultShape, call: &Call<'_>) -> Result<Value, String> {
        let ResultShape::Optional(parameter) = shape else {
            return Err("storage result shape mismatch".into());
        };
        let layout = call.parameter_layout(parameter)?;
        PreparedOption::from_layout(layout, call.services.0.clone())?.construct_native(self)
    }
}
impl StorageOutput for OwnedSequence {
    fn into_value(self, shape: ResultShape, call: &Call<'_>) -> Result<Value, String> {
        if !matches!(shape, ResultShape::Owned) {
            return Err("storage result shape mismatch".into());
        }
        let payload = DynamicValue::sequence(
            call.receiver.descriptor().layout_handle(),
            self.into_elements(),
        )?;
        crate::value::native_instance::from_native(payload, Rc::new(call.services.0.clone()))
    }
}
impl StorageOutput for Vec<DynamicValue> {
    fn into_value(self, shape: ResultShape, call: &Call<'_>) -> Result<Value, String> {
        let ResultShape::Iterator(parameter) = shape else {
            return Err("storage result shape mismatch".into());
        };
        let layout = call.parameter_layout(parameter)?;
        Ok(Value::from_owned_iterator(OwnedIteratorValue::from_native(
            self,
            layout,
            call.services.0.clone(),
        )?))
    }
}
