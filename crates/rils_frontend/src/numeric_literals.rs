use crate::{
    ast::Literal,
    source::Span,
    types::{FloatType, IntegerType, Type},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericLiteralError {
    pub message: String,
    pub span: Span,
}

/// Materializes an untyped numeric syntax literal from its semantic type.
///
/// The returned value is detached from the AST so interpreters and lowerers
/// can consume the same type decision without rewriting source syntax.
pub fn concretize_numeric_literal(
    literal: &Literal,
    inferred: Option<&Type>,
    span: Span,
) -> Result<Literal, NumericLiteralError> {
    match literal {
        Literal::Integer(value) => {
            let value = *value;
            let ty = match inferred {
                Some(Type::Integer(ty)) => *ty,
                _ => IntegerType::I32,
            };
            integer_literal(value, ty).ok_or_else(|| NumericLiteralError {
                message: format!("integer literal `{value}` is outside the `{ty}` range"),
                span,
            })
        }
        Literal::Float(value) => Ok(match inferred {
            Some(Type::Float(FloatType::F32)) => Literal::F32(*value as f32),
            _ => Literal::F64(*value),
        }),
        literal => Ok(literal.clone()),
    }
}

fn integer_literal(value: i128, ty: IntegerType) -> Option<Literal> {
    macro_rules! integer {
        ($variant:ident, $type:ty) => {
            <$type>::try_from(value).ok().map(Literal::$variant)
        };
    }
    match ty {
        IntegerType::I8 => integer!(I8, i8),
        IntegerType::I16 => integer!(I16, i16),
        IntegerType::I32 => integer!(I32, i32),
        IntegerType::I64 => integer!(I64, i64),
        IntegerType::I128 => Some(Literal::I128(value)),
        IntegerType::Isize => integer!(Isize, isize),
        IntegerType::U8 => integer!(U8, u8),
        IntegerType::U16 => integer!(U16, u16),
        IntegerType::U32 => integer!(U32, u32),
        IntegerType::U64 => integer!(U64, u64),
        IntegerType::U128 => integer!(U128, u128),
        IntegerType::Usize => integer!(Usize, usize),
    }
}
