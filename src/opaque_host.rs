use std::rc::Rc;

use crate::Value;
use crate::value::{HostObject, HostType};

/// Payload carried by the portable host-handle ABI value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueHostHandle {
    pub object_id: i64,
    pub generation: u32,
    pub type_id: u32,
}

/// Construct a host object value with the stable `HostHandle` runtime type.
pub fn opaque_host_value(handle: OpaqueHostHandle) -> Value {
    Value::HostObject(Rc::new(HostObject {
        type_definition: Rc::new(HostType {
            name: "HostHandle".into(),
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
    if object.type_definition.name != "HostHandle" {
        return None;
    }
    object.payload.downcast_ref::<OpaqueHostHandle>().copied()
}
