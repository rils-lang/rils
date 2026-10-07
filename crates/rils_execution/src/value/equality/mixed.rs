//! Inspect compatibility containers without reconstructing native children.

use super::{Type, Value};
use rils_value::DynamicValueRef;

pub(super) fn equal(view: &DynamicValueRef<'_>, value: &Value) -> Result<bool, String> {
    let layout = view.layout()?;
    match (layout.rils_type(), value) {
        (Type::Option(_), Value::Option { value, .. }) => match (view.option_is_some()?, value) {
            (false, None) => Ok(true),
            (true, Some(item)) => super::view_equal_value(view.option_item()?, item),
            _ => Ok(false),
        },
        (Type::Result(_, _), Value::Result { value, .. }) => {
            let (index, item) = match value {
                Ok(item) => (0, item),
                Err(item) => (1, item),
            };
            if view.variant_index()? != index {
                return Ok(false);
            }
            super::view_equal_value(view.variant_payload()?, item)
        }
        (Type::Tuple(_), Value::Tuple(sequence)) | (Type::Array { .. }, Value::Array(sequence)) => {
            let elements = sequence.elements.borrow();
            let fields = layout
                .record_fields()
                .ok_or("indexed aggregate has no native fields")?;
            if fields.len() != elements.len() {
                return Ok(false);
            }
            for (index, slot) in elements.iter().enumerate() {
                let item = slot
                    .value
                    .as_ref()
                    .ok_or("cannot compare a moved element")?;
                if !super::view_equal_value(view.field(index)?, item)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (_, Value::Vec(sequence))
            if super::super::native_layouts::vec::matches(layout.rils_type()) =>
        {
            let elements = sequence.elements.borrow();
            if view.sequence_len()? != elements.len() {
                return Ok(false);
            }
            for (index, slot) in elements.iter().enumerate() {
                let item = slot
                    .value
                    .as_ref()
                    .ok_or("cannot compare a moved element")?;
                if !super::view_equal_value(view.sequence_item(index)?, item)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
