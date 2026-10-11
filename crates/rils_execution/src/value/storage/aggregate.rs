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
        let layout = self.layout(ty)?;
        let mut codec =
            NativeRecordCodec::with_definitions(self.structs, self.enums).with_owned_conversion();
        let fields = fields
            .into_iter()
            .zip(layout.record_fields().ok_or("tuple has no fields")?)
            .map(|(value, field)| codec.into_native(value, field.layout_handle()))
            .collect::<Result<Vec<_>, _>>()?;
        let payload = DynamicValue::record(layout, fields)?;
        super::super::native_instance::from_native(payload, Rc::new(codec))
    }
}
