//! Conversions at the boundary between payload storage and runtime values.
use std::rc::Rc;

use super::{DynamicObject, HostObject, NativeObject, Value};

impl From<NativeObject> for Value {
    fn from(value: NativeObject) -> Self {
        Self::Native(value)
    }
}

impl From<DynamicObject> for Value {
    fn from(value: DynamicObject) -> Self {
        Self::Dynamic(value)
    }
}

impl From<Rc<HostObject>> for Value {
    fn from(value: Rc<HostObject>) -> Self {
        Self::HostObject(value)
    }
}
