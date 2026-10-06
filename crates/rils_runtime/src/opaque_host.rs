use std::collections::HashSet;
use std::rc::Rc;

use crate::value::{EnumInstance, EnumPayload, HostObject, HostType};
use crate::{HostEnumDefinition, Value};

use rils_execution::value::host_declarations::{HOST_FLAGS_RAW_VARIANT, enum_definition};

/// Payload carried by the portable host-handle ABI value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueHostHandle {
    pub object_id: i64,
    pub generation: u32,
    pub type_id: u32,
}

/// Canonical 16-byte payload for a manifest-declared inline host value.
/// Individual fields are explicitly packed by the host according to the
/// manifest layout; this never represents a native Rust or managed struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineHostValue {
    pub bytes: [u8; 16],
}

/// Construct a host object value with the stable `HostHandle` runtime type.
pub fn opaque_host_value(handle: OpaqueHostHandle) -> Value {
    opaque_host_value_typed(handle, "HostHandle", HashSet::new())
}

/// Construct a host object with a manifest-provided nominal type and lineage.
pub fn opaque_host_value_typed(
    handle: OpaqueHostHandle,
    type_name: impl Into<String>,
    base_types: HashSet<String>,
) -> Value {
    Value::HostObject(Rc::new(HostObject {
        type_definition: Rc::new(HostType {
            name: type_name.into(),
            base_types,
            copy: true,
            methods: Default::default(),
        }),
        payload: Rc::new(handle),
    }))
}

/// Read a portable host-handle payload from a Rils value.
pub fn opaque_host_handle(value: &Value) -> Option<OpaqueHostHandle> {
    let Value::HostObject(object) = value else {
        return None;
    };
    object.payload.downcast_ref::<OpaqueHostHandle>().copied()
}

pub fn inline_host_value_typed(bytes: [u8; 16], type_name: impl Into<String>) -> Value {
    Value::HostObject(Rc::new(HostObject {
        type_definition: Rc::new(HostType {
            name: type_name.into(),
            base_types: HashSet::new(),
            copy: true,
            methods: Default::default(),
        }),
        payload: Rc::new(InlineHostValue { bytes }),
    }))
}

pub fn inline_host_value(value: &Value) -> Option<InlineHostValue> {
    let Value::HostObject(object) = value else {
        return None;
    };
    object.payload.downcast_ref::<InlineHostValue>().copied()
}

/// Constructs a real Rils enum value from a host ABI discriminant.
pub fn host_enum_value(
    type_name: impl Into<String>,
    definition: &HostEnumDefinition,
    raw: u128,
) -> Result<Value, String> {
    let type_name = type_name.into();
    let variant = definition
        .variants
        .iter()
        .find_map(|(name, value)| (*value == raw).then(|| name.clone()))
        .or_else(|| definition.flags.then(|| HOST_FLAGS_RAW_VARIANT.to_owned()))
        .ok_or_else(|| {
            format!("host enum `{type_name}` returned unknown discriminant 0x{raw:x}")
        })?;
    let payload = if variant == HOST_FLAGS_RAW_VARIANT {
        EnumPayload::Tuple(vec![Value::from_u128(raw)])
    } else {
        EnumPayload::Unit
    };
    Ok(Value::Enum(Rc::new(EnumInstance {
        type_definition: enum_definition(type_name, definition),
        variant,
        payload,
        type_arguments: Vec::new(),
    })))
}

/// Extracts the canonical host ABI discriminant from a matching Rils enum value.
pub fn host_enum_raw(
    value: &Value,
    type_name: &str,
    definition: &HostEnumDefinition,
) -> Result<u128, String> {
    let Value::Enum(instance) = value else {
        return Err(format!("expected host enum `{type_name}`"));
    };
    if instance.type_definition.name != type_name {
        return Err(format!(
            "expected host enum `{type_name}`, found `{}`",
            instance.type_definition.name
        ));
    }
    if instance.variant == HOST_FLAGS_RAW_VARIANT {
        if definition.flags
            && let EnumPayload::Tuple(values) = &instance.payload
            && let [raw] = values.as_slice()
            && let Some(raw) = raw.as_u128()
        {
            return Ok(raw);
        }
        return Err(format!(
            "host enum `{type_name}` contains invalid flags payload"
        ));
    }
    if !matches!(instance.payload, EnumPayload::Unit) {
        return Err(format!(
            "host enum `{type_name}` variants cannot carry payloads"
        ));
    }
    definition
        .variants
        .get(&instance.variant)
        .copied()
        .ok_or_else(|| {
            format!(
                "host enum `{type_name}` has unknown variant `{}`",
                instance.variant
            )
        })
}
