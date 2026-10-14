//! Owned bridge for collection values with a declaration-derived sequence layout.

use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicPathStep, DynamicType, DynamicValue};

use crate::Type;

use super::{
    DynamicObject, EnumType, StructType, Value, record_codec, record_layout::RecordLayoutResolver,
};

/// Construct an empty collection from its concrete declared type.
pub fn empty_with_definitions(
    ty: &Type,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Value, String> {
    let mut resolver = RecordLayoutResolver::with_enums(structs, enums);
    let layout = resolver.resolve(ty)?;
    empty_with_layout(layout)
}

pub(crate) fn empty_with_layout(layout: Rc<DynamicLayout>) -> Result<Value, String> {
    if layout.sequence_item().is_none() {
        return Err(format!(
            "{} has no native collection layout",
            layout.rils_type()
        ));
    }
    let payload = DynamicValue::sequence(layout.clone(), Vec::new())?;
    Ok(Value::Dynamic(DynamicObject::new(
        Rc::new(DynamicType::new(layout)),
        payload,
    )?))
}

pub fn copy_item(object: &DynamicObject, index: usize) -> Result<Value, String> {
    let item = object.with(|payload| payload.copy_path(&[DynamicPathStep::Index(index)]))??;
    record_codec::from_native(item)
}

pub fn replace_item(object: &DynamicObject, index: usize, value: Value) -> Result<(), String> {
    let layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("native Vec has no item layout")?
        .clone();
    if !layout.rils_type().accepts(&value) {
        return Err(format!(
            "value is incompatible with element type {}",
            layout.rils_type()
        ));
    }
    let item = record_codec::into_native(value, layout)?;
    object.with_mut(|payload| {
        payload.sequence_borrows()?.check_element_replace(index)?;
        payload.replace_sequence_item(index, item).map(|_| ())
    })??;
    Ok(())
}
