//! Borrow the original compatibility key; never reconstruct a temporary Value.
use super::HashKey;
use crate::{
    IntegerType, Type,
    value::{
        borrowed::{Read, with_read},
        record_codec::NativeRecordCodec,
    },
};
use rils_value::NativeLeafRef;
use std::rc::Rc;

impl HashKey {
    pub(crate) fn with_native_view<R>(
        &self,
        callback: impl FnOnce(rils_value::DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        match self {
            Self::Composite(value) => crate::host_value::with_native_value(&value.value, callback),
            _ => Err("key has no composed native layout view".into()),
        }
    }

    pub(crate) fn with_read<R>(
        &self,
        callback: impl for<'a> FnOnce(Read<'a>) -> R,
    ) -> Result<R, String> {
        macro_rules! leaf {
            ($value:expr, $ty:expr) => {
                Ok(callback(Read::Leaf(NativeLeafRef::from_rust($value, $ty))))
            };
        }
        macro_rules! integer {
            ($value:expr, $kind:ident) => {
                leaf!($value, Type::Integer(IntegerType::$kind))
            };
        }
        match self {
            Self::Unit => leaf!(&(), Type::Unit),
            Self::Bool(value) => leaf!(value, Type::Bool),
            Self::Char(value) => leaf!(value, Type::Char),
            Self::I8(value) => integer!(value, I8),
            Self::I16(value) => integer!(value, I16),
            Self::I32(value) => integer!(value, I32),
            Self::I64(value) => integer!(value, I64),
            Self::I128(value) => integer!(value, I128),
            Self::Isize(value) => integer!(value, Isize),
            Self::U8(value) => integer!(value, U8),
            Self::U16(value) => integer!(value, U16),
            Self::U32(value) => integer!(value, U32),
            Self::U64(value) => integer!(value, U64),
            Self::U128(value) => integer!(value, U128),
            Self::Usize(value) => integer!(value, Usize),
            Self::String(value) => leaf!(value.as_ref(), Type::String),
            Self::Composite(value) => with_read(&value.value, false, callback),
        }
    }

    pub(crate) fn native_codec(&self) -> Result<Option<Rc<NativeRecordCodec>>, String> {
        match self {
            Self::Composite(value) => crate::value::borrowed::native_codec(&value.value),
            _ => Ok(None),
        }
    }
}
