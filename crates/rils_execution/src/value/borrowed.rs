//! Borrow readers retain original storage guards across recursive inspections.

use rils_value::{DynamicValueRef, NativeLeafRef};

use super::reference::BorrowedTarget;
use super::{Type, Value};

pub(crate) fn native_codec(
    value: &Value,
) -> Result<Option<std::rc::Rc<super::record_codec::NativeRecordCodec>>, String> {
    match value {
        Value::Dynamic(object) => Ok(object.descriptor().metadata()),
        Value::Reference(reference) => reference.native_codec(),
        _ => Ok(None),
    }
}

pub(crate) enum Read<'a> {
    View(DynamicValueRef<'a>),
    Leaf(NativeLeafRef<'a>),
    Legacy(&'a Value),
}

pub(crate) fn with_read<R>(
    value: &Value,
    dereference: bool,
    callback: impl for<'a> FnOnce(Read<'a>) -> R,
) -> Result<R, String> {
    if dereference && let Value::Reference(reference) = value {
        return reference.with_borrowed_target(|target| match target {
            BorrowedTarget::Value(value) => with_read(value, false, callback),
            BorrowedTarget::Native(view) => Ok(callback(Read::View(view))),
        })?;
    }
    macro_rules! scalar {
        ($item:expr) => {
            Ok(callback(Read::Leaf(NativeLeafRef::from_rust(
                $item,
                Type::of_value(value).expect("scalar has a type"),
            ))))
        };
    }
    match value {
        Value::Dynamic(object) => object.with(|value| callback(Read::View(value.view()))),
        Value::Native(object) => object.with_leaf(|leaf| callback(Read::Leaf(leaf))),
        Value::Unit => scalar!(&()),
        Value::Bool(item) => scalar!(item),
        Value::I16(item) => scalar!(item),
        Value::I64(item) => scalar!(item),
        Value::I128(item) => scalar!(item),
        Value::Isize(item) => scalar!(item),
        Value::U8(item) => scalar!(item),
        Value::U16(item) => scalar!(item),
        Value::U32(item) => scalar!(item),
        Value::U64(item) => scalar!(item),
        Value::U128(item) => scalar!(item),
        Value::Usize(item) => scalar!(item),
        Value::F32(item) => scalar!(item),
        Value::F64(item) => scalar!(item),
        Value::Char(item) => scalar!(item),
        value => Ok(callback(Read::Legacy(value))),
    }
}
