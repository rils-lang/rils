//! Host-facing ownership choices for an execution result.

use std::rc::Rc;

use crate::Value;
use rils_value::DynamicValueRef;

pub use rils_stdlib::host_value::RilsHostType;

/// A script result whose payload stays managed by the Rils runtime.
///
/// Access to a Rust value is explicit: borrow during a callback, clone it,
/// or consume an owned result. Only payloads whose native Rust layout is `T`
/// can currently be exposed as `&T`.
pub struct RilsValue {
    value: Value,
}

impl RilsValue {
    pub fn new(value: Value) -> Self {
        Self { value }
    }

    pub fn is_unit(&self) -> bool {
        self.value.is_unit()
    }

    /// Borrow one field of a script-defined struct by declaration index.
    /// The returned handle keeps the original record alive.
    pub fn field(&self, index: usize) -> Result<Self, String> {
        if let Value::Dynamic(object) = &self.value
            && matches!(
                crate::value::native_instance::definition(object),
                Some(crate::value::Declaration::StructType(_))
            )
        {
            let name = self
                .field_name(index)
                .ok_or_else(|| format!("record field index {index} is out of bounds"))?;
            let reference =
                crate::value::native_instance::NativeInstancePlace::new(object.clone())?
                    .field(&name)?
                    .borrow(false, None)?;
            return Ok(Self::new(Value::from_reference(Rc::new(reference))));
        }
        if let Some(reference) = &self.value.as_reference()
            && let Some(crate::value::Declaration::StructType(definition)) =
                reference.native_type_definition()?
        {
            let name = definition
                .fields
                .get(index)
                .ok_or_else(|| format!("record field index {index} is out of bounds"))?
                .name
                .as_str();
            let projected = reference
                .project_native_field(name)?
                .ok_or("result has no native record field")?;
            return Ok(Self::new(Value::from_reference(Rc::new(projected))));
        }
        Err("result is not a script struct".into())
    }

    pub fn struct_name(&self) -> Option<String> {
        self.struct_definition()
            .map(|definition| definition.name.clone())
    }

    pub fn field_name(&self, index: usize) -> Option<String> {
        self.struct_definition()
            .and_then(|definition| definition.fields.get(index).map(|field| field.name.clone()))
    }

    fn struct_definition(&self) -> Option<Rc<crate::value::StructType>> {
        match &self.value {
            __reference_value if __reference_value.as_reference().is_some() => {
                let reference = __reference_value
                    .as_reference()
                    .expect("checked reference handle");
                match reference.native_type_definition().ok()? {
                    Some(crate::value::Declaration::StructType(definition)) => Some(definition),
                    _ => None,
                }
            }
            Value::Dynamic(object) => match crate::value::native_instance::definition(object)? {
                crate::value::Declaration::StructType(definition) => Some(definition),
                _ => None,
            },

            _ => None,
        }
    }

    pub fn with_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        with_host_value::<T, _>(&self.value, callback)
    }

    /// Borrow a layout-aware view of a native composite without materializing `Value` children.
    pub fn with_native_view<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        self.value.with_native_view(callback)
    }

    pub fn get_cloned<T: RilsHostType + Clone>(&self) -> Result<T, String> {
        self.with_ref(Clone::clone)
    }

    pub fn into_owned<T: RilsHostType>(self) -> Result<T, Box<(Self, String)>> {
        match self.value {
            Value::Native(object) => {
                object
                    .into_rust::<T::Native>()
                    .map(T::from_native)
                    .map_err(|error| {
                        let (object, message) = *error;
                        Box::new((Self::new(Value::Native(object)), message))
                    })
            }
            Value::Dynamic(object) => {
                if object.descriptor().layout().is_rust_type::<T>() {
                    return object.into_rust::<T>().map_err(|error| {
                        let (object, message) = *error;
                        Box::new((Self::new(Value::Dynamic(object)), message))
                    });
                }
                object
                    .into_rust::<T::Native>()
                    .map(T::from_native)
                    .map_err(|error| {
                        let (object, message) = *error;
                        Box::new((Self::new(Value::Dynamic(object)), message))
                    })
            }
            value => Err(Box::new((
                Self::new(value),
                "result is not an owned native Rust value".into(),
            ))),
        }
    }
}

impl std::fmt::Display for RilsValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(formatter)
    }
}

impl std::fmt::Debug for RilsValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.value, formatter)
    }
}

impl PartialEq for RilsValue {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl PartialEq<Value> for RilsValue {
    fn eq(&self, other: &Value) -> bool {
        self.value == *other
    }
}

impl PartialEq<RilsValue> for Value {
    fn eq(&self, other: &RilsValue) -> bool {
        *self == other.value
    }
}

pub(crate) fn with_rust_value<T: 'static, R>(
    value: &Value,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    if let Some(reference) = value.as_reference() {
        return reference.with_rust(callback);
    }
    match value {
        Value::Native(object) => object.with(callback),
        Value::Dynamic(object) => object.with(|value| value.with(callback))?,
        Value::HostObject(object) => Err(format!(
            "host object {} has no registered Rust borrow view",
            object.type_definition.name
        )),
    }
}

pub(crate) fn with_host_value<T: RilsHostType, R>(
    value: &Value,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    match value {
        __reference_value if __reference_value.as_reference().is_some() => {
            let reference = __reference_value
                .as_reference()
                .expect("checked reference handle");
            reference.with_host_ref(callback)
        }
        Value::Dynamic(object) => {
            object.with(|value| T::with_composed_ref(value.view(), callback))?
        }

        _ => with_rust_value::<T::Native, _>(value, |value| callback(T::as_native_ref(value))),
    }
}

pub(crate) fn with_native_value<R>(
    value: &Value,
    callback: impl FnOnce(DynamicValueRef<'_>) -> R,
) -> Result<R, String> {
    if let Some(reference) = value.as_reference() {
        return reference.with_native_view(callback);
    }
    match value {
        Value::Dynamic(object) => object.with(|value| callback(value.view())),
        Value::Native(object) => Err(format!(
            "{} has no composed layout view",
            object.descriptor().rils_type()
        )),
        Value::HostObject(object) => Err(format!(
            "host object {} has no composed layout view",
            object.type_definition.name
        )),
    }
}
