//! Conversion of range syntax into concrete standard-library iterators.

use std::rc::Rc;

use crate::{IntegerType, Type};

use super::{NativeObject, NativeType, Value, native_ops};

pub fn native_range(start: Value, end: Value) -> Result<Value, String> {
    if matches!((&start, &end), (Value::Native(_), Value::Native(_)))
        && let (Some(start), Some(end)) = (
            crate::numeric::i32_payload(&start),
            crate::numeric::i32_payload(&end),
        )
    {
        return native_range(Value::I32(start), Value::I32(end));
    }
    if matches!((&start, &end), (Value::Native(_), Value::Native(_)))
        && let (Some(start), Some(end)) = (start.as_usize(), end.as_usize())
    {
        return native_range(Value::Usize(start), Value::Usize(end));
    }
    macro_rules! native_range {
        ($start:expr, $end:expr, $rust:ty, $type:expr) => {{
            type Range = rils_stdlib::stdlib::range::Range<$rust>;
            std::thread_local! {
                static DESCRIPTOR: Rc<NativeType> = Rc::new(
                    NativeType::new::<Range>(Type::Named {
                        name: "Range".into(),
                        arguments: vec![$type],
                    })
                    .register_method(native_ops::CLONE, |context| {
                        let clone = context.receiver::<Range, _>(Clone::clone)?;
                        Ok(Value::Native(context.new_object(clone)?))
                    })
                    .register_method(native_ops::EQUAL, |context| {
                        let [Value::Native(other)] = context.arguments() else {
                            return Err("Range equality expects one native range".into());
                        };
                        let equal = context.receiver::<Range, _>(|range| {
                            other.with::<Range, _>(|other| range == other)
                        })??;
                        Ok(Value::Bool(equal))
                    })
                    .register_method(native_ops::DISPLAY, |context| {
                        Ok(super::native_string(context.receiver::<Range, _>(ToString::to_string)?))
                    })
                    .register_method(native_ops::NEXT, |context| {
                        let value = context.receiver_mut::<Range, _>(Iterator::next)?;
                        Ok(Value::Option {
                            value: value.map(|item| Rc::new(
                                crate::runtime_builtins::native_value::NativeValue::into_value(item)
                            )),
                            element_type: Some($type),
                        })
                    }),
                );
            }
            Ok(Value::Native(NativeObject::new(
                DESCRIPTOR.with(Rc::clone),
                Range::from_bounds($start, $end),
            )?))
        }};
    }

    if let (Some(start), Some(end)) = (
        crate::numeric::i8_payload(&start),
        crate::numeric::i8_payload(&end),
    ) {
        return native_range!(start, end, i8, Type::Integer(IntegerType::I8));
    }

    match (start, end) {
        (Value::I16(start), Value::I16(end)) => {
            native_range!(start, end, i16, Type::Integer(IntegerType::I16))
        }
        (Value::I32(start), Value::I32(end)) => native_range!(start, end, i32, Type::I32),
        (Value::I64(start), Value::I64(end)) => {
            native_range!(start, end, i64, Type::Integer(IntegerType::I64))
        }
        (Value::I128(start), Value::I128(end)) => {
            native_range!(start, end, i128, Type::Integer(IntegerType::I128))
        }
        (Value::Isize(start), Value::Isize(end)) => {
            native_range!(start, end, isize, Type::Integer(IntegerType::Isize))
        }
        (Value::U8(start), Value::U8(end)) => {
            native_range!(start, end, u8, Type::Integer(IntegerType::U8))
        }
        (Value::U16(start), Value::U16(end)) => {
            native_range!(start, end, u16, Type::Integer(IntegerType::U16))
        }
        (Value::U32(start), Value::U32(end)) => {
            native_range!(start, end, u32, Type::Integer(IntegerType::U32))
        }
        (Value::U64(start), Value::U64(end)) => {
            native_range!(start, end, u64, Type::Integer(IntegerType::U64))
        }
        (Value::U128(start), Value::U128(end)) => {
            native_range!(start, end, u128, Type::Integer(IntegerType::U128))
        }
        (Value::Usize(start), Value::Usize(end)) => {
            native_range!(start, end, usize, Type::USIZE)
        }
        _ => Err("range bounds must have the same integer type".into()),
    }
}
