//! Declaration-driven constructors write owned children directly into native bytes.

use std::collections::HashMap;
use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicValue};

use super::{NativeRecordCodec, TypedStorageContext};
use crate::{Type, ast::EnumVariant, value::Value};

impl TypedStorageContext<'_> {
    pub fn construct_record(
        &self,
        ty: &Type,
        variant: Option<&str>,
        fields: HashMap<String, Value>,
    ) -> Result<Value, String> {
        let mut codec = self.constructor_codec(ty)?;
        let layout = self.layout(ty)?;
        let payload = if let Some(variant) = variant {
            let (index, declaration, child) = Self::variant_layout(&codec, ty, &layout, variant)?;
            if !matches!(declaration, EnumVariant::Record { .. }) {
                return Err(format!("{ty}::{variant} is not a record variant"));
            }
            let record = encode_fields(fields, child, &mut codec)?;
            DynamicValue::variant(layout, index, record)?
        } else {
            let Some(Value::StructType(definition)) = codec.nominal_definition(ty) else {
                return Err(format!("{ty} is not a struct declaration"));
            };
            if definition.opaque_native {
                return Err(format!("cannot construct opaque type `{ty}` from fields"));
            }
            encode_fields(fields, layout, &mut codec)?
        };
        super::super::native_instance::from_native(payload, Rc::new(codec))
    }

    pub fn construct_tuple_variant(
        &self,
        ty: &Type,
        variant: &str,
        fields: Vec<Value>,
    ) -> Result<Value, String> {
        let mut codec = self.constructor_codec(ty)?;
        let layout = self.layout(ty)?;
        let (index, declaration, child) = Self::variant_layout(&codec, ty, &layout, variant)?;
        let EnumVariant::Tuple {
            fields: declared, ..
        } = declaration
        else {
            return Err(format!("{ty}::{variant} is not a tuple variant"));
        };
        if declared.len() != fields.len() {
            return Err(format!(
                "{ty}::{variant} expects {} fields, found {}",
                declared.len(),
                fields.len()
            ));
        }
        let fields = fields
            .into_iter()
            .enumerate()
            .map(|(index, value)| (index.to_string(), value))
            .collect();
        let payload = encode_fields(fields, child, &mut codec)?;
        let value = DynamicValue::variant(layout, index, payload)?;
        super::super::native_instance::from_native(value, Rc::new(codec))
    }

    pub fn construct_unit_variant(&self, ty: &Type, variant: &str) -> Result<Value, String> {
        let codec = self.constructor_codec(ty)?;
        let layout = self.layout(ty)?;
        let (index, declaration, child) = Self::variant_layout(&codec, ty, &layout, variant)?;
        if !matches!(declaration, EnumVariant::Unit { .. }) {
            return Err(format!("{ty}::{variant} is not a unit variant"));
        }
        let payload = DynamicValue::from_rust(child, ())?;
        let value = DynamicValue::variant(layout, index, payload)?;
        super::super::native_instance::from_native(value, Rc::new(codec))
    }

    fn constructor_codec(&self, ty: &Type) -> Result<NativeRecordCodec, String> {
        if !ty.is_concrete_type() {
            return Err(format!(
                "constructor requires a concrete instance type, found {ty}"
            ));
        }
        let codec =
            NativeRecordCodec::with_definitions(self.structs, self.enums).with_owned_conversion();
        if codec.nominal_definition(ty).is_none() {
            return Err(format!("no user instance declaration for {ty}"));
        }
        Ok(codec)
    }

    fn variant_layout(
        codec: &NativeRecordCodec,
        ty: &Type,
        layout: &DynamicLayout,
        variant: &str,
    ) -> Result<(usize, EnumVariant, Rc<DynamicLayout>), String> {
        let Some(Value::EnumType(definition)) = codec.nominal_definition(ty) else {
            return Err(format!("{ty} is not an enum declaration"));
        };
        let index = definition
            .variants
            .iter()
            .position(|entry| super::super::enum_variant_name(entry) == variant)
            .ok_or_else(|| format!("enum `{ty}` has no variant `{variant}`"))?;
        super::super::native_instance::validate_enum_layout(layout, &definition)?;
        let child = layout
            .variant_alternatives()
            .ok_or("enum has no variant layout")?[index]
            .clone();
        Ok((index, definition.variants[index].clone(), child))
    }
}

fn encode_fields(
    mut values: HashMap<String, Value>,
    layout: Rc<DynamicLayout>,
    codec: &mut NativeRecordCodec,
) -> Result<DynamicValue, String> {
    let fields = layout
        .record_fields()
        .ok_or("record constructor requires a record layout")?;
    if values.len() != fields.len()
        || values
            .keys()
            .any(|name| layout.record_field_index(name).is_none())
    {
        return Err(format!(
            "record `{}` fields do not match its declaration",
            layout.rils_type()
        ));
    }
    let children = fields
        .iter()
        .map(|field| {
            let value = values
                .remove(field.name())
                .ok_or_else(|| format!("record constructor is missing field `{}`", field.name()))?;
            codec.into_native(value, field.layout_handle())
        })
        .collect::<Result<Vec<_>, _>>()?;
    DynamicValue::record(layout, children)
}
