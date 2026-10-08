//! Collection queries inspect the original key without creating an owned payload.

use rils_native::NativeKey;
use rils_value::{DynamicLayout, DynamicValueRef};

use super::{
    Value,
    borrowed::{Read, with_read},
    record_codec::NativeRecordCodec,
};
use crate::Type;

pub(crate) fn query(
    value: &Value,
    layout: &DynamicLayout,
    ordered: bool,
) -> Result<NativeKey, String> {
    read(value, layout, ordered, true)
}

fn read(
    value: &Value,
    layout: &DynamicLayout,
    ordered: bool,
    dereference: bool,
) -> Result<NativeKey, String> {
    let codec = super::borrowed::native_codec(value)?.unwrap_or_default();
    with_read(value, dereference, |value| {
        let registry = rils_stdlib::native::registry();
        match value {
            Read::View(view) => {
                let actual = view.layout()?;
                if !layout.compatible_with(&actual) {
                    return Err(format!(
                        "collection key expects {}, found {}",
                        layout.rils_type(),
                        actual.rils_type()
                    ));
                }
                native(view, &codec, ordered)
            }
            Read::Leaf(leaf) => {
                if leaf.rils_type() != layout.rils_type() {
                    return Err(format!(
                        "collection key expects {}, found {}",
                        layout.rils_type(),
                        leaf.rils_type()
                    ));
                }
                if ordered {
                    registry.ordered_key_leaf(&leaf)
                } else {
                    registry.key_leaf(&leaf)
                }
            }
            Read::Legacy(value) => legacy(value, layout, ordered),
        }
    })?
}

/// Execution declarations enforce nominal key traits; the native registry owns
/// leaf identity extraction and has no dependency on execution metadata.
pub(crate) fn native(
    view: DynamicValueRef<'_>,
    codec: &NativeRecordCodec,
    ordered: bool,
) -> Result<NativeKey, String> {
    let registry = rils_stdlib::native::registry();
    if ordered {
        return registry.ordered_key(view);
    }
    validate(&view, codec)?;
    registry.key(view)
}

fn validate(view: &DynamicValueRef<'_>, codec: &NativeRecordCodec) -> Result<(), String> {
    let layout = view.layout()?;
    if matches!(layout.rils_type(), Type::Named { .. }) {
        let supported = match codec.nominal_definition(layout.rils_type()) {
            Some(Value::StructType(definition)) => {
                let traits = definition.implemented_traits.borrow();
                traits.contains("Eq") && traits.contains("Hash")
            }
            Some(Value::EnumType(definition)) => {
                let traits = definition.implemented_traits.borrow();
                traits.contains("Eq") && traits.contains("Hash")
            }
            _ => false,
        };
        if !supported {
            return Err(format!(
                "{} cannot be used as a hash collection key: requires Eq + Hash",
                layout.rils_type()
            ));
        }
    }
    if layout.option_item().is_some() {
        if view.option_is_some()? {
            validate(&view.option_item()?, codec)?;
        }
    } else if layout.variant_alternatives().is_some() {
        let payload = view.variant_payload()?;
        // A named enum's payload record is part of the enum declaration,
        // not a separately declared nominal type with its own trait table.
        if matches!(layout.rils_type(), Type::Named { .. }) {
            validate_fields(&payload, codec)?;
        } else {
            validate(&payload, codec)?;
        }
    } else {
        validate_fields(view, codec)?;
    }
    Ok(())
}

fn validate_fields(view: &DynamicValueRef<'_>, codec: &NativeRecordCodec) -> Result<(), String> {
    if let Some(fields) = view.layout()?.record_fields() {
        for index in 0..fields.len() {
            validate(&view.field(index)?, codec)?;
        }
    }
    Ok(())
}

fn legacy(value: &Value, layout: &DynamicLayout, ordered: bool) -> Result<NativeKey, String> {
    let ty = layout.rils_type();
    if ordered {
        return Err(format!("{ty} has no registered native key ordering"));
    }
    match (ty, value) {
        (
            Type::Option(item),
            Value::Option {
                value,
                element_type,
            },
        ) => {
            check_annotation(item, element_type.as_ref())?;
            match value {
                Some(value) => read(
                    value,
                    layout.option_item().ok_or("missing key item layout")?,
                    false,
                    false,
                )
                .map(|key| NativeKey::Some(Box::new(key))),
                None => Ok(NativeKey::None),
            }
        }
        (
            Type::Result(ok, error),
            Value::Result {
                value,
                ok_type,
                error_type,
            },
        ) => {
            check_annotation(ok, ok_type.as_ref())?;
            check_annotation(error, error_type.as_ref())?;
            let (index, value) = match value {
                Ok(value) => (0, value),
                Err(value) => (1, value),
            };
            let alternatives = layout
                .variant_alternatives()
                .ok_or("missing key alternatives")?;
            let key = read(value, &alternatives[index], false, false)?;
            Ok(NativeKey::Variant(index, Box::new(key)))
        }
        (Type::Tuple(_), Value::Tuple(sequence)) | (Type::Array { .. }, Value::Array(sequence)) => {
            let elements = sequence
                .elements
                .try_borrow()
                .map_err(|_| "key sequence is already mutably accessed")?;
            let fields = layout.record_fields().ok_or("missing key fields")?;
            if elements.len() != fields.len() {
                return Err("collection key length mismatch".into());
            }
            if let Type::Array { element, .. } = ty {
                let annotation = sequence
                    .element_type
                    .try_borrow()
                    .map_err(|_| "key sequence type is already mutably accessed")?;
                check_annotation(element, annotation.as_ref())?;
            }
            elements
                .iter()
                .zip(fields)
                .map(|(slot, field)| {
                    check_annotation(field.layout().rils_type(), Some(&slot.type_annotation))?;
                    read(
                        slot.value.as_ref().ok_or("moved key element")?,
                        field.layout(),
                        false,
                        false,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NativeKey::Fields)
        }
        _ => Err(format!(
            "{} cannot be used as a hash collection key",
            value.type_name()
        )),
    }
}

fn check_annotation(expected: &Type, actual: Option<&Type>) -> Result<(), String> {
    if let Some(actual) = actual
        && crate::types::merge_types(expected, actual).as_ref() != Some(expected)
    {
        return Err(format!("collection key expects {expected}, found {actual}"));
    }
    Ok(())
}
