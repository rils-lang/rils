//! Host-facing ownership choices for an execution result.

use std::any::Any;
use std::rc::Rc;

use crate::Value;
use crate::value::ReferenceValue;
use crate::value::storage::StoredDataRef;
use rils_stdlib::stdlib::{float::Number as FloatNumber, integer::Number as IntegerNumber};
use rils_value::DynamicValueRef;

/// Maps a Rust host type to the exact Rust payload stored for its Rils type.
pub trait RilsHostType: Sized + 'static {
    type Native: 'static;

    fn from_native(value: Self::Native) -> Self;
    fn as_native_ref(value: &Self::Native) -> &Self;

    /// Borrow a composed leaf. A type may use a different native
    /// representation for an inline field than for its standalone wrapper.
    fn with_composed_ref<R>(
        view: DynamicValueRef<'_>,
        callback: impl FnOnce(&Self) -> R,
    ) -> Result<R, String> {
        view.with_rust::<Self::Native, _>(|value| callback(Self::as_native_ref(value)))
    }
}

macro_rules! direct_host_type {
    ($($ty:ty),* $(,)?) => {$(
        impl RilsHostType for $ty {
            type Native = Self;

            fn from_native(value: Self::Native) -> Self { value }
            fn as_native_ref(value: &Self::Native) -> &Self { value }
        }
    )*};
}

direct_host_type!((), bool, char);

macro_rules! wrapped_host_type {
    ($wrapper:ident; $($ty:ty),* $(,)?) => {$(
        impl RilsHostType for $ty {
            type Native = $wrapper<$ty>;

            fn from_native(value: Self::Native) -> Self { value.0 }
            fn as_native_ref(value: &Self::Native) -> &Self { &value.0 }

            fn with_composed_ref<R>(view: DynamicValueRef<'_>, callback: impl FnOnce(&Self) -> R) -> Result<R, String> {
                if view.layout()?.is_rust_type::<Self>() {
                    view.with_rust(callback)
                } else {
                    view.with_rust::<Self::Native, _>(|value| callback(&value.0))
                }
            }
        }
    )*};
}

wrapped_host_type!(IntegerNumber; i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
wrapped_host_type!(FloatNumber; f32, f64);

impl RilsHostType for String {
    type Native = rils_stdlib::stdlib::string::String;

    fn from_native(value: Self::Native) -> Self {
        value.into()
    }

    fn as_native_ref(value: &Self::Native) -> &Self {
        value.as_ref()
    }
}

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
        matches!(self.value, Value::Unit)
    }

    /// Borrow one field of a script-defined struct by declaration index.
    /// The returned handle keeps the original record alive.
    pub fn field(&self, index: usize) -> Result<Self, String> {
        if let Value::Dynamic(object) = &self.value
            && matches!(
                crate::value::native_instance::definition(object),
                Some(Value::StructType(_))
            )
        {
            let name = self
                .field_name(index)
                .ok_or_else(|| format!("record field index {index} is out of bounds"))?;
            let reference =
                crate::value::native_instance::NativeInstancePlace::new(object.clone())?
                    .field(&name)?
                    .borrow(false, None)?;
            return Ok(Self::new(Value::Reference(Rc::new(reference))));
        }
        if let Value::Reference(reference) = &self.value
            && let Some(Value::StructType(definition)) = reference.native_type_definition()?
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
            return Ok(Self::new(Value::Reference(Rc::new(projected))));
        }
        let (instance, guard) = match &self.value {
            Value::Struct(instance) => (instance.clone(), None),
            Value::Reference(reference) => match reference.read()? {
                Value::Struct(instance) => (instance, Some(reference.clone())),
                _ => return Err("result is not a script struct".into()),
            },
            _ => return Err("result is not a script struct".into()),
        };
        let reference =
            ReferenceValue::new_guarded_struct_field_index(instance, index, false, guard)?;
        Ok(Self::new(Value::Reference(Rc::new(reference))))
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
            Value::Struct(instance) => Some(instance.type_definition.clone()),
            Value::Dynamic(object) => match crate::value::native_instance::definition(object)? {
                Value::StructType(definition) => Some(definition),
                _ => None,
            },
            Value::Reference(reference) => {
                if let Some(Value::StructType(definition)) =
                    reference.native_type_definition().ok()?
                {
                    return Some(definition);
                }
                match reference.read().ok()? {
                    Value::Struct(instance) => Some(instance.type_definition.clone()),
                    _ => None,
                }
            }
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
        with_native_value(&self.value, callback)
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
            value => move_legacy::<T::Native>(value)
                .map(T::from_native)
                .map_err(|error| {
                    let (value, message) = *error;
                    Box::new((Self::new(value), message))
                }),
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
    if let Some(storage) = value.stored_data() {
        return match storage {
            StoredDataRef::Native(object) => object.with(callback),
            StoredDataRef::Dynamic(object) => object.with(|value| value.with(callback))?,
            StoredDataRef::Host(object) => Err(format!(
                "host object {} has no registered Rust borrow view",
                object.type_definition.name
            )),
        };
    }
    match value {
        Value::Reference(reference) => reference.with_rust(callback),
        Value::Unit => borrow_legacy(&(), callback),
        Value::Bool(value) => borrow_legacy(value, callback),
        Value::I16(value) => borrow_legacy(value, callback),
        Value::I64(value) => borrow_legacy(value, callback),
        Value::I128(value) => borrow_legacy(value, callback),
        Value::Isize(value) => borrow_legacy(value, callback),
        Value::U8(value) => borrow_legacy(value, callback),
        Value::U16(value) => borrow_legacy(value, callback),
        Value::U32(value) => borrow_legacy(value, callback),
        Value::U64(value) => borrow_legacy(value, callback),
        Value::U128(value) => borrow_legacy(value, callback),
        Value::Usize(value) => borrow_legacy(value, callback),
        Value::F32(value) => borrow_legacy(value, callback),
        Value::F64(value) => borrow_legacy(value, callback),
        Value::Char(value) => borrow_legacy(value, callback),
        _ => Err(format!("{} has no Rust borrow view", value.type_name())),
    }
}

pub(crate) fn with_host_value<T: RilsHostType, R>(
    value: &Value,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    match value {
        Value::Dynamic(object) => {
            object.with(|value| T::with_composed_ref(value.view(), callback))?
        }
        Value::Reference(reference) => reference.with_host_ref(callback),
        _ => with_rust_value::<T::Native, _>(value, |value| callback(T::as_native_ref(value))),
    }
}

pub(crate) fn with_native_value<R>(
    value: &Value,
    callback: impl FnOnce(DynamicValueRef<'_>) -> R,
) -> Result<R, String> {
    if let Some(storage) = value.stored_data() {
        return match storage {
            StoredDataRef::Dynamic(object) => object.with(|value| callback(value.view())),
            StoredDataRef::Native(object) => Err(format!(
                "{} has no composed layout view",
                object.descriptor().rils_type()
            )),
            StoredDataRef::Host(object) => Err(format!(
                "host object {} has no composed layout view",
                object.type_definition.name
            )),
        };
    }
    match value {
        Value::Reference(reference) => reference.with_native_view(callback),
        _ => Err(format!("{} has no native layout view", value.type_name())),
    }
}

fn borrow_legacy<T: 'static, U: 'static, R>(
    value: &U,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    (value as &dyn Any)
        .downcast_ref::<T>()
        .map(callback)
        .ok_or_else(|| format!("value is not {}", std::any::type_name::<T>()))
}

macro_rules! move_scalar {
    ($item:ident, $variant:ident, $ty:ty) => {
        (Box::new($item) as Box<dyn Any>)
            .downcast::<$ty>()
            .map(|item| *item)
            .map_err(|item| {
                Box::new((
                    Value::$variant(*item.downcast().expect("original variant type")),
                    format!("value is not {}", std::any::type_name::<$ty>()),
                ))
            })
    };
}

fn move_legacy<T: 'static>(value: Value) -> Result<T, Box<(Value, String)>> {
    // Match once so that a failed downcast can put the original payload back.
    match value {
        Value::Unit => (Box::new(()) as Box<dyn Any>)
            .downcast::<T>()
            .map(|item| *item)
            .map_err(|_| {
                Box::new((
                    Value::Unit,
                    format!("value is not {}", std::any::type_name::<T>()),
                ))
            }),
        Value::Bool(item) => move_scalar!(item, Bool, T),
        Value::I16(item) => move_scalar!(item, I16, T),
        Value::I64(item) => move_scalar!(item, I64, T),
        Value::I128(item) => move_scalar!(item, I128, T),
        Value::Isize(item) => move_scalar!(item, Isize, T),
        Value::U8(item) => move_scalar!(item, U8, T),
        Value::U16(item) => move_scalar!(item, U16, T),
        Value::U32(item) => move_scalar!(item, U32, T),
        Value::U64(item) => move_scalar!(item, U64, T),
        Value::U128(item) => move_scalar!(item, U128, T),
        Value::Usize(item) => move_scalar!(item, Usize, T),
        Value::F32(item) => move_scalar!(item, F32, T),
        Value::F64(item) => move_scalar!(item, F64, T),
        Value::Char(item) => move_scalar!(item, Char, T),
        value => Err(Box::new((
            value,
            "result is not an owned native Rust value".into(),
        ))),
    }
}
