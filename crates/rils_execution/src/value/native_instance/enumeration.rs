//! Enum metadata and variant fields retain paths into the original native owner.

use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicPathStep};

use super::{NativeInstancePlace, Value, value_definition};
use crate::{Type, ast::EnumVariant, value::EnumType};

pub struct NativeEnumVariant {
    pub definition: Rc<EnumType>,
    pub index: usize,
}

impl NativeEnumVariant {
    pub fn declaration(&self) -> &EnumVariant {
        &self.definition.variants[self.index]
    }

    pub fn name(&self) -> &str {
        crate::value::enum_variant_name(self.declaration())
    }

    pub fn field_names(&self) -> Vec<String> {
        match self.declaration() {
            EnumVariant::Unit { .. } => vec![],
            EnumVariant::Tuple { fields, .. } => {
                (0..fields.len()).map(|index| index.to_string()).collect()
            }
            EnumVariant::Record { fields, .. } => {
                fields.iter().map(|field| field.name.clone()).collect()
            }
        }
    }
}

pub(crate) fn validate_layout(layout: &DynamicLayout, definition: &EnumType) -> Result<(), String> {
    let Type::Named { arguments, .. } = layout.rils_type() else {
        return Err("enum requires a nominal layout".into());
    };
    if arguments.len() != definition.generic_parameters.len() {
        return Err(format!(
            "enum `{}` has wrong type argument count",
            definition.name
        ));
    }
    let alternatives = layout
        .variant_alternatives()
        .ok_or("enum has no variant layout")?;
    if alternatives.len() != definition.variants.len() {
        return Err(format!(
            "enum `{}` has wrong variant count",
            definition.name
        ));
    }
    for (index, (declaration, payload)) in definition.variants.iter().zip(alternatives).enumerate()
    {
        let name = crate::value::enum_variant_name(declaration);
        if layout.variant_name(index) != Some(name) {
            return Err(format!(
                "enum `{}` variant order differs from its declaration",
                definition.name
            ));
        }
        let expected = match declaration {
            EnumVariant::Unit { .. } => {
                if payload.rils_type() != &Type::Unit {
                    return Err(format!("unit variant `{name}` has a payload layout"));
                }
                continue;
            }
            EnumVariant::Tuple { fields, .. } => (0..fields.len())
                .map(|index| index.to_string())
                .collect::<Vec<_>>(),
            EnumVariant::Record { fields, .. } => {
                fields.iter().map(|field| field.name.clone()).collect()
            }
        };
        let fields = payload
            .record_fields()
            .ok_or_else(|| format!("variant `{name}` has no aggregate layout"))?;
        if fields.len() != expected.len()
            || fields
                .iter()
                .zip(expected)
                .any(|(field, name)| field.name() != name)
        {
            return Err(format!("variant `{name}` has wrong field order"));
        }
    }
    Ok(())
}

pub fn enum_variant(value: &Value) -> Result<Option<NativeEnumVariant>, String> {
    if !matches!(value, Value::Dynamic(_) | Value::Reference(_)) {
        return Ok(None);
    }
    let Some(Value::EnumType(definition)) = value_definition(value)? else {
        return Ok(None);
    };
    let index = match value {
        Value::Dynamic(object) => object.with(|payload| payload.variant_index())??,
        Value::Reference(reference) => {
            reference.with_native_view(|view| view.variant_index())??
        }
        _ => unreachable!(),
    };
    if index >= definition.variants.len() {
        return Err(format!(
            "enum `{}` has no variant at index {index}",
            definition.name
        ));
    }
    Ok(Some(NativeEnumVariant { definition, index }))
}

pub fn borrow_variant_field(value: &Value, index: usize, name: &str) -> Result<Value, String> {
    let reference = match value {
        Value::Dynamic(object) => NativeInstancePlace::new(object.clone())?
            .project(DynamicPathStep::Variant(index))?
            .field(name)?
            .borrow(false, None)?,
        Value::Reference(reference) => {
            let payload = Rc::new(
                reference
                    .project_native_variant(index)?
                    .ok_or("enum reference has no native payload")?,
            );
            payload
                .project_native_field(name)?
                .ok_or_else(|| format!("variant has no field `{name}`"))?
                .reborrow(false)?
        }
        _ => return Err("value is not a native enum".into()),
    };
    Ok(Value::Reference(Rc::new(reference)))
}

pub(crate) fn equal(left: &Value, right: &Value) -> Option<bool> {
    let variant = enum_variant(left)
        .ok()
        .flatten()
        .or_else(|| enum_variant(right).ok().flatten())?;
    let ty = |value: &Value| match value {
        Value::Reference(reference) => reference
            .native_layout()
            .ok()
            .flatten()
            .map(|layout| layout.rils_type().clone()),
        _ => Type::of_value(value),
    };
    let active = |value: &Value| {
        enum_variant(value)
            .ok()
            .flatten()
            .map(|variant| variant.name().to_owned())
    };
    if ty(left) != ty(right) || active(left) != active(right) {
        return Some(false);
    }
    Some(variant.field_names().iter().all(|name| {
        let field = |value: &Value| borrow_variant_field(value, variant.index, name).ok();
        let (Some(left), Some(right)) = (field(left), field(right)) else {
            return false;
        };
        if let Some(equal) = super::records_equal(&left, &right).or_else(|| equal(&left, &right)) {
            return equal;
        }
        let read = |value: Value| match value {
            Value::Reference(reference) => reference.read(),
            value => Ok(value),
        };
        matches!((read(left), read(right)), (Ok(left), Ok(right)) if left == right)
    }))
}
