//! Type-complete identities for compatibility collection lookup. No owned
//! payload conversion is needed, even for nested legacy aggregate wrappers.
use super::*;
use std::rc::Rc;

pub(crate) fn identity(
    value: &Value,
    expected: Option<&Type>,
    ordered: bool,
) -> Result<(Type, NativeKey), String> {
    read_identity(value, expected, ordered, true)
}
fn read_identity(
    value: &Value,
    expected: Option<&Type>,
    ordered: bool,
    dereference: bool,
) -> Result<(Type, NativeKey), String> {
    let codec = crate::value::borrowed::native_codec(value)?;
    with_read(value, dereference, |read| match read {
        Read::View(view) => {
            let ty = concrete(view.layout()?.rils_type().clone(), expected)?;
            let key = native(
                view,
                codec.as_deref().unwrap_or(&NativeRecordCodec::default()),
                ordered,
            )?;
            Ok((ty, key))
        }
        Read::Leaf(leaf) => {
            let ty = concrete(leaf.rils_type().clone(), expected)?;
            let registry = rils_stdlib::native::registry();
            let key = if ordered {
                registry.ordered_key_leaf(&leaf)
            } else {
                registry.key_leaf(&leaf)
            }?;
            Ok((ty, key))
        }
        Read::Legacy(value) => legacy_identity(value, expected, ordered),
    })?
}
fn concrete(actual: Type, expected: Option<&Type>) -> Result<Type, String> {
    let ty = if let Some(expected) = expected {
        crate::types::merge_types(expected, &actual)
            .ok_or_else(|| format!("collection key expects {expected}, found {actual}"))?
    } else {
        actual
    };
    if !ty.is_concrete_type() {
        return Err(format!(
            "collection key requires a complete type witness, found {ty}"
        ));
    }
    Ok(ty)
}
fn child(value: &Value, expected: &Type) -> Result<NativeKey, String> {
    read_identity(value, Some(expected), false, false).map(|(_, key)| key)
}
fn legacy_identity(
    value: &Value,
    expected: Option<&Type>,
    ordered: bool,
) -> Result<(Type, NativeKey), String> {
    // Read type witnesses under the same guards used for payload traversal.
    if ordered {
        return Err("composite key has no registered native key ordering".into());
    }
    let (ty, key) = match value {
        Value::Option {
            value,
            element_type,
        } => {
            let actual = Type::Option(Box::new(type_hint(
                element_type.as_ref(),
                value.as_deref(),
                expected.is_some_and(Type::is_concrete_type),
            )?));
            let ty = concrete(actual, expected)?;
            let Type::Option(item) = &ty else {
                unreachable!()
            };
            let key = match value {
                Some(value) => NativeKey::Some(Box::new(child(value, item)?)),
                None => NativeKey::None,
            };
            (ty, key)
        }
        Value::Result {
            value,
            ok_type,
            error_type,
        } => {
            let ty = concrete(
                Type::Result(
                    Box::new(type_hint(
                        ok_type.as_ref(),
                        value.as_ref().ok().map(Rc::as_ref),
                        expected.is_some_and(Type::is_concrete_type),
                    )?),
                    Box::new(type_hint(
                        error_type.as_ref(),
                        value.as_ref().err().map(Rc::as_ref),
                        expected.is_some_and(Type::is_concrete_type),
                    )?),
                ),
                expected,
            )?;
            let Type::Result(ok, error) = &ty else {
                unreachable!()
            };
            let (index, key) = match value {
                Ok(value) => (0, child(value, ok)?),
                Err(value) => (1, child(value, error)?),
            };
            (ty, NativeKey::Variant(index, Box::new(key)))
        }
        Value::Tuple(sequence) | Value::Array(sequence) => {
            let elements = sequence
                .elements
                .try_borrow()
                .map_err(|_| "key sequence is already mutably accessed")?;
            let actual = if matches!(value, Value::Tuple(_)) {
                Type::Tuple(
                    elements
                        .iter()
                        .map(|slot| slot.type_annotation.clone())
                        .collect(),
                )
            } else {
                let element = sequence
                    .element_type
                    .try_borrow()
                    .map_err(|_| "key sequence type is already mutably accessed")?;
                Type::Array {
                    element: Box::new(element.clone().unwrap_or(Type::Unknown)),
                    length: elements.len(),
                }
            };
            let ty = concrete(actual, expected)?;
            let mut keys = Vec::with_capacity(elements.len());
            for (index, slot) in elements.iter().enumerate() {
                let expected = match &ty {
                    Type::Tuple(items) => &items[index],
                    Type::Array { element, .. } => element,
                    _ => unreachable!(),
                };
                super::check_annotation(expected, Some(&slot.type_annotation))?;
                keys.push(child(
                    slot.value.as_ref().ok_or("moved key element")?,
                    expected,
                )?);
            }
            (ty, NativeKey::Fields(keys))
        }
        _ => return Err("value has no registered native key identity".into()),
    };
    Ok((ty, key))
}

fn type_hint(
    annotation: Option<&Type>,
    payload: Option<&Value>,
    has_expected: bool,
) -> Result<Type, String> {
    if let Some(ty) = annotation {
        return Ok(ty.clone());
    }
    if !has_expected && let Some(value) = payload {
        return read_identity(value, None, false, false).map(|(ty, _)| ty);
    }
    Ok(Type::Unknown)
}
