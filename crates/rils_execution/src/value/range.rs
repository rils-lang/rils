//! Conversion of range syntax into concrete standard-library iterators.

use std::rc::Rc;

use crate::{IntegerType, Type};

use super::{NativeObject, NativeType, Value, native_ops};

pub fn native_range(start: Value, end: Value) -> Result<Value, String> {
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
                        Ok(Value::from(crate::value::native_bool(equal)))
                    })
                    .register_method(native_ops::DISPLAY, |context| {
                        Ok(Value::from(super::native_string(context.receiver::<Range, _>(ToString::to_string)?)))
                    })
                    .register_method(native_ops::NEXT, |context| {
                        let value = context.receiver_mut::<Range, _>(Iterator::next)?;
                        super::storage::TypedStorageContext::new(&[], &[]).construct_option(
                            &Type::Option(Box::new($type)),
                            value.map(crate::runtime_builtins::native_value::NativeValue::into_value),
                        )
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
    if let (Some(start), Some(end)) = (
        crate::numeric::i16_payload(&start),
        crate::numeric::i16_payload(&end),
    ) {
        return native_range!(start, end, i16, Type::Integer(IntegerType::I16));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::i32_payload(&start),
        crate::numeric::i32_payload(&end),
    ) {
        return native_range!(start, end, i32, Type::Integer(IntegerType::I32));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::i64_payload(&start),
        crate::numeric::i64_payload(&end),
    ) {
        return native_range!(start, end, i64, Type::Integer(IntegerType::I64));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::i128_payload(&start),
        crate::numeric::i128_payload(&end),
    ) {
        return native_range!(start, end, i128, Type::Integer(IntegerType::I128));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::isize_payload(&start),
        crate::numeric::isize_payload(&end),
    ) {
        return native_range!(start, end, isize, Type::Integer(IntegerType::Isize));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::u8_payload(&start),
        crate::numeric::u8_payload(&end),
    ) {
        return native_range!(start, end, u8, Type::Integer(IntegerType::U8));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::u16_payload(&start),
        crate::numeric::u16_payload(&end),
    ) {
        return native_range!(start, end, u16, Type::Integer(IntegerType::U16));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::u32_payload(&start),
        crate::numeric::u32_payload(&end),
    ) {
        return native_range!(start, end, u32, Type::Integer(IntegerType::U32));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::u64_payload(&start),
        crate::numeric::u64_payload(&end),
    ) {
        return native_range!(start, end, u64, Type::Integer(IntegerType::U64));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::u128_payload(&start),
        crate::numeric::u128_payload(&end),
    ) {
        return native_range!(start, end, u128, Type::Integer(IntegerType::U128));
    }
    if let (Some(start), Some(end)) = (
        crate::numeric::usize_payload(&start),
        crate::numeric::usize_payload(&end),
    ) {
        return native_range!(start, end, usize, Type::Integer(IntegerType::Usize));
    }
    Err("range bounds must have the same integer type".into())
}
