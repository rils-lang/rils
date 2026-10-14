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
    let codec = super::borrowed::native_codec(value)?;
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
                native(
                    view,
                    codec.as_deref().unwrap_or(&NativeRecordCodec::default()),
                    ordered,
                )
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
            Read::Host(object) => Err(format!(
                "{} cannot be used as a hash collection key",
                object.type_definition.name
            )),
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
            Some(crate::value::Declaration::StructType(definition)) => {
                let traits = definition.implemented_traits.borrow();
                traits.contains("Eq") && traits.contains("Hash")
            }
            Some(crate::value::Declaration::EnumType(definition)) => {
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
