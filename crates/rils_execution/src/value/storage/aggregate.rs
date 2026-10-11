//! Construct indexed aggregates directly from owned children.
use super::{NativeRecordCodec, TypedStorageContext, Value};
use crate::Type;
use rils_value::DynamicValue;
use std::rc::Rc;

impl TypedStorageContext<'_> {
    pub fn construct_tuple(&self, ty: &Type, fields: Vec<Value>) -> Result<Value, String> {
        let Type::Tuple(types) = ty else {
            return Err("tuple construction requires a tuple type".into());
        };
        if !ty.is_concrete_type() {
            return Err(format!("cannot infer the complete native tuple type: {ty}"));
        }
        if types.len() != fields.len() {
            return Err(format!(
                "tuple expects {} fields, found {}",
                types.len(),
                fields.len()
            ));
        }
        self.construct_aggregate(ty, fields)
    }

    pub fn construct_array(&self, ty: &Type, elements: Vec<Value>) -> Result<Value, String> {
        let Type::Array { length, .. } = ty else {
            return Err("array construction requires a fixed array type".into());
        };
        if !ty.is_concrete_type() {
            return Err(format!("cannot infer the complete native array type: {ty}"));
        }
        if *length != elements.len() {
            return Err(format!(
                "array expects {length} elements, found {}",
                elements.len()
            ));
        }
        self.construct_aggregate(ty, elements)
    }

    pub fn construct_repeat_array(
        &self,
        ty: &Type,
        value: Value,
        count: usize,
    ) -> Result<Value, String> {
        if !ty.is_concrete_type() {
            return Err(format!("cannot infer the complete native array type: {ty}"));
        }
        if !matches!(ty, Type::Array { length, .. } if *length == count) {
            return Err("array repeat count does not match its type witness".into());
        }
        if !value.is_copy() {
            return Err("array repeat syntax requires a Copy value".into());
        }
        let values = (0..count)
            .map(|_| value.clone_owned())
            .collect::<Result<Vec<_>, _>>()?;
        self.construct_array(ty, values)
    }

    fn construct_aggregate(&self, ty: &Type, fields: Vec<Value>) -> Result<Value, String> {
        let layout = self.layout(ty)?;
        let mut codec =
            NativeRecordCodec::with_definitions(self.structs, self.enums).with_owned_conversion();
        let fields = fields
            .into_iter()
            .zip(layout.record_fields().ok_or("aggregate has no fields")?)
            .map(|(value, field)| codec.into_native(value, field.layout_handle()))
            .collect::<Result<Vec<_>, _>>()?;
        let payload = DynamicValue::record(layout, fields)?;
        super::super::native_instance::from_native(payload, Rc::new(codec))
    }
}
