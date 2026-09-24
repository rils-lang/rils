use crate::{IntegerType, Type};

use super::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct RangeValue {
    pub(super) current: Box<Value>,
    pub(super) end: Box<Value>,
    element_type: Type,
}

impl RangeValue {
    pub fn new(current: Value, end: Value) -> Result<Self, String> {
        let element_type = match (&current, &end) {
            (Value::I8(_), Value::I8(_)) => Type::Integer(IntegerType::I8),
            (Value::I16(_), Value::I16(_)) => Type::Integer(IntegerType::I16),
            (Value::I32(_), Value::I32(_)) => Type::I32,
            (Value::I64(_), Value::I64(_)) => Type::Integer(IntegerType::I64),
            (Value::I128(_), Value::I128(_)) => Type::Integer(IntegerType::I128),
            (Value::Isize(_), Value::Isize(_)) => Type::Integer(IntegerType::Isize),
            (Value::U8(_), Value::U8(_)) => Type::Integer(IntegerType::U8),
            (Value::U16(_), Value::U16(_)) => Type::Integer(IntegerType::U16),
            (Value::U32(_), Value::U32(_)) => Type::Integer(IntegerType::U32),
            (Value::U64(_), Value::U64(_)) => Type::Integer(IntegerType::U64),
            (Value::U128(_), Value::U128(_)) => Type::Integer(IntegerType::U128),
            (Value::Usize(_), Value::Usize(_)) => Type::USIZE,
            _ => return Err("range bounds must have the same integer type".into()),
        };
        Ok(Self {
            current: Box::new(current),
            end: Box::new(end),
            element_type,
        })
    }

    pub fn element_type(&self) -> Type {
        self.element_type.clone()
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Value>, String> {
        fn advance<T: rils_stdlib::stdlib::range::RangeStep>(
            current: &mut T,
            end: &T,
        ) -> Option<T> {
            let mut range = rils_stdlib::stdlib::range::Range::from_bounds(*current, *end);
            let item = range.next();
            *current = range.current();
            match item {
                rils_stdlib::stdlib::prelude::Option::Some(value) => Some(value),
                rils_stdlib::stdlib::prelude::Option::None => None,
            }
        }
        match (self.current.as_mut(), self.end.as_ref()) {
            (Value::I8(a), Value::I8(b)) => Ok(advance(a, b).map(Value::I8)),
            (Value::I16(a), Value::I16(b)) => Ok(advance(a, b).map(Value::I16)),
            (Value::I32(a), Value::I32(b)) => Ok(advance(a, b).map(Value::I32)),
            (Value::I64(a), Value::I64(b)) => Ok(advance(a, b).map(Value::I64)),
            (Value::I128(a), Value::I128(b)) => Ok(advance(a, b).map(Value::I128)),
            (Value::Isize(a), Value::Isize(b)) => Ok(advance(a, b).map(Value::Isize)),
            (Value::U8(a), Value::U8(b)) => Ok(advance(a, b).map(Value::U8)),
            (Value::U16(a), Value::U16(b)) => Ok(advance(a, b).map(Value::U16)),
            (Value::U32(a), Value::U32(b)) => Ok(advance(a, b).map(Value::U32)),
            (Value::U64(a), Value::U64(b)) => Ok(advance(a, b).map(Value::U64)),
            (Value::U128(a), Value::U128(b)) => Ok(advance(a, b).map(Value::U128)),
            (Value::Usize(a), Value::Usize(b)) => Ok(advance(a, b).map(Value::Usize)),
            _ => Err("range bounds have incompatible types".into()),
        }
    }
}
