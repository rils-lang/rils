//! Execution-owned native leaf registrations for references, calls and hosts.

use std::rc::Rc;

use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

use crate::{Type, environment::EnvironmentRef};

use super::{HostObject, HostType, ReferenceValue, Value, record_layout::NativeLayoutProvider};

mod callable;
use callable::Callable;

/// Compatibility read of a borrowed native value. Host leaves retain their
/// opaque identity; this does not mark a non-Copy host object as Copy.
pub fn clone_borrowed_view(view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
    rils_stdlib::native::registry().clone_borrowed_view_with(view, &mut |leaf| {
        let layout = match leaf.layout() {
            Ok(layout) => layout,
            Err(error) => return Some(Err(error)),
        };
        if !layout.is_rust_type::<Rc<HostObject>>() {
            return None;
        }
        Some(
            leaf.with_rust::<Rc<HostObject>, _>(Rc::clone)
                .and_then(|object| encode_host(object, layout)),
        )
    })
}

pub fn clone_borrowed_element(value: &DynamicValue) -> Result<DynamicValue, String> {
    clone_borrowed_view(value.view())
}

/// Preserve host subtype checks through native sums and indexed aggregates,
/// inspecting the registered host leaf without materializing its payload.
pub(crate) fn accepts_view(view: DynamicValueRef<'_>, expected: &Type) -> Result<bool, String> {
    let layout = view.layout()?;
    if crate::types::merge_types(expected, layout.rils_type()).is_some() {
        return Ok(true);
    }
    match (expected, layout.rils_type()) {
        (Type::Named { name, arguments }, _) if layout.is_rust_type::<Rc<HostObject>>() => view
            .with_rust::<Rc<HostObject>, _>(|object| {
                arguments.is_empty()
                    && (object.type_definition.name == *name
                        || object.type_definition.base_types.contains(name))
            }),
        (Type::Option(expected), Type::Option(_)) if view.option_is_some()? => {
            accepts_view(view.option_item()?, expected)
        }
        (Type::Result(ok, error), Type::Result(actual_ok, actual_error)) => {
            let (expected, inactive, actual_inactive) = if view.variant_index()? == 0 {
                (ok, error, actual_error)
            } else {
                (error, ok, actual_ok)
            };
            Ok(
                crate::types::merge_types(inactive, actual_inactive).is_some()
                    && accepts_view(view.variant_payload()?, expected)?,
            )
        }
        (Type::Tuple(expected), Type::Tuple(actual)) if expected.len() == actual.len() => {
            for (index, expected) in expected.iter().enumerate() {
                if !accepts_view(view.field(index)?, expected)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            Type::Array { element, length },
            Type::Array {
                length: actual_length,
                ..
            },
        ) if length == actual_length => {
            for index in 0..*length {
                if !accepts_view(view.field(index)?, element)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Layouts for execution handles are registered here, independently of
/// standard-library payloads. A reference leaf owns its lexical lease and a
/// callable leaf owns its call target; neither stores a `Value` data wrapper.
pub(super) fn layout(ty: &Type) -> Option<Rc<DynamicLayout>> {
    match ty {
        Type::Reference { .. } => Some(DynamicLayout::copy_handle_of::<Rc<ReferenceValue>>(
            ty.clone(),
        )),
        Type::Function { .. } => Some(DynamicLayout::copy_handle_of::<Callable>(ty.clone())),
        _ => None,
    }
}

/// Registered host declarations supply their own Copy policy. Unknown named
/// types are left unresolved rather than being treated as opaque host values.
pub struct HostLayoutProvider<'a> {
    definitions: &'a [Rc<HostType>],
}

impl<'a> HostLayoutProvider<'a> {
    pub fn new(definitions: &'a [Rc<HostType>]) -> Self {
        Self { definitions }
    }
}

impl NativeLayoutProvider for HostLayoutProvider<'_> {
    fn layout(
        &self,
        ty: &Type,
        _resolve_child: &mut dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String>,
    ) -> Option<Result<Rc<DynamicLayout>, String>> {
        let Type::Named { name, arguments } = ty else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        let mut definitions = self
            .definitions
            .iter()
            .filter(|definition| definition.name == *name);
        let definition = definitions.next()?;
        if definitions.next().is_some() {
            return Some(Err(format!("ambiguous host declaration for {ty}")));
        }
        Some(Ok(if definition.copy {
            DynamicLayout::copy_handle_of::<Rc<HostObject>>(ty.clone())
        } else {
            DynamicLayout::of::<Rc<HostObject>>(ty.clone())
        }))
    }
}

pub(super) fn encode_reference(
    value: Rc<ReferenceValue>,
    layout: Rc<DynamicLayout>,
) -> Result<DynamicValue, String> {
    let actual = Type::of_value(&Value::Reference(value.clone()));
    if actual.is_none_or(|actual| crate::types::merge_types(layout.rils_type(), &actual).is_none())
    {
        return Err(format!("reference does not match {}", layout.rils_type()));
    }
    DynamicValue::from_rust(layout, value)
}

pub(super) fn encode_callable(
    value: Value,
    layout: Rc<DynamicLayout>,
) -> Result<DynamicValue, String> {
    if !layout.rils_type().accepts(&value) {
        return Err(format!("callable does not match {}", layout.rils_type()));
    }
    DynamicValue::from_rust(layout, Callable::from_value(value)?)
}

pub(super) fn encode_host(
    value: Rc<HostObject>,
    layout: Rc<DynamicLayout>,
) -> Result<DynamicValue, String> {
    let Type::Named { name, arguments } = layout.rils_type() else {
        return Err("host object requires a named declaration".into());
    };
    if !arguments.is_empty()
        || (name != &value.type_definition.name && !value.type_definition.base_types.contains(name))
        || layout.is_copy() != value.type_definition.copy
    {
        return Err("host object differs from its registered declaration".into());
    }
    DynamicValue::from_rust(layout, value)
}

pub(super) fn is_execution_leaf(layout: &DynamicLayout) -> bool {
    layout.is_rust_type::<Rc<ReferenceValue>>()
        || layout.is_rust_type::<Callable>()
        || layout.is_rust_type::<Rc<HostObject>>()
}

pub(super) fn decode(value: DynamicValue) -> Result<Value, String> {
    let layout = value.descriptor();
    if layout.is_rust_type::<Rc<ReferenceValue>>() {
        value
            .into_rust::<Rc<ReferenceValue>>()
            .map(Value::Reference)
            .map_err(|error| error.1)
    } else if layout.is_rust_type::<Callable>() {
        value
            .into_rust::<Callable>()
            .map(Callable::into_value)
            .map_err(|error| error.1)
    } else if layout.is_rust_type::<Rc<HostObject>>() {
        value
            .into_rust::<Rc<HostObject>>()
            .map(Value::HostObject)
            .map_err(|error| error.1)
    } else {
        Err("native payload is not a registered execution handle".into())
    }
}

/// Inspect leases directly through all active composed fields and branches.
pub(super) fn contains_reference(view: DynamicValueRef<'_>) -> Result<bool, String> {
    view.any_leaf(&mut |leaf| {
        let layout = leaf.layout()?;
        if layout.is_rust_type::<Rc<ReferenceValue>>() {
            Ok(true)
        } else if layout.is_rust_type::<Callable>() {
            leaf.with_rust::<Callable, _>(Callable::contains_reference)
        } else {
            Ok(false)
        }
    })
}

pub(super) fn contains_local_reference(
    view: DynamicValueRef<'_>,
    environment: &EnvironmentRef,
) -> Result<bool, String> {
    view.any_leaf(&mut |leaf| {
        let layout = leaf.layout()?;
        if layout.is_rust_type::<Rc<ReferenceValue>>() {
            leaf.with_rust::<Rc<ReferenceValue>, _>(|value| value.is_local_to(environment))
        } else if layout.is_rust_type::<Callable>() {
            leaf.with_rust::<Callable, _>(|value| value.contains_local_reference(environment))
        } else {
            Ok(false)
        }
    })
}
