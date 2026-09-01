use super::*;

pub(super) fn lower_literal(value: &Literal) -> HirLiteral {
    match value {
        Literal::Unit => HirLiteral::Unit,
        Literal::Bool(value) => HirLiteral::Bool(*value),
        Literal::I8(value) => HirLiteral::I8(*value),
        Literal::I16(value) => HirLiteral::I16(*value),
        Literal::I32(value) => HirLiteral::I32(*value),
        Literal::I64(value) => HirLiteral::I64(*value),
        Literal::I128(value) => HirLiteral::I128(*value),
        Literal::Isize(value) => HirLiteral::Isize(*value),
        Literal::U8(value) => HirLiteral::U8(*value),
        Literal::U16(value) => HirLiteral::U16(*value),
        Literal::U32(value) => HirLiteral::U32(*value),
        Literal::U64(value) => HirLiteral::U64(*value),
        Literal::U128(value) => HirLiteral::U128(*value),
        Literal::Usize(value) => HirLiteral::Usize(*value),
        Literal::F32(value) => HirLiteral::F32(*value),
        Literal::F64(value) => HirLiteral::F64(*value),
        Literal::Char(value) => HirLiteral::Char(*value),
        Literal::Integer(value) => HirLiteral::I32(
            i32::try_from(*value).expect("unresolved integer literal must fit the i32 default"),
        ),
        Literal::Float(value) => HirLiteral::F64(*value),
        Literal::String(value) => HirLiteral::String(value.clone()),
    }
}

pub(super) fn lower_expression_literal(
    value: &Literal,
    inferred: &Type,
    span: Span,
) -> Result<HirLiteral, CompileError> {
    let overflow = |value: i128, ty: crate::types::IntegerType| {
        CompileError::new(
            format!("integer literal `{value}` is outside the `{ty}` range"),
            span,
        )
    };
    let Literal::Integer(value) = value else {
        if let Literal::Float(value) = value {
            return Ok(match inferred {
                Type::Float(crate::types::FloatType::F32) => HirLiteral::F32(*value as f32),
                _ => HirLiteral::F64(*value),
            });
        }
        return Ok(lower_literal(value));
    };
    let ty = match inferred {
        Type::Integer(ty) => *ty,
        _ => crate::types::IntegerType::I32,
    };
    macro_rules! signed {
        ($variant:ident, $type:ty) => {
            <$type>::try_from(*value)
                .map(HirLiteral::$variant)
                .map_err(|_| overflow(*value, ty))
        };
    }
    macro_rules! unsigned {
        ($variant:ident, $type:ty) => {
            <$type>::try_from(*value)
                .map(HirLiteral::$variant)
                .map_err(|_| overflow(*value, ty))
        };
    }
    match ty {
        crate::types::IntegerType::I8 => signed!(I8, i8),
        crate::types::IntegerType::I16 => signed!(I16, i16),
        crate::types::IntegerType::I32 => signed!(I32, i32),
        crate::types::IntegerType::I64 => signed!(I64, i64),
        crate::types::IntegerType::I128 => Ok(HirLiteral::I128(*value)),
        crate::types::IntegerType::Isize => signed!(Isize, isize),
        crate::types::IntegerType::U8 => unsigned!(U8, u8),
        crate::types::IntegerType::U16 => unsigned!(U16, u16),
        crate::types::IntegerType::U32 => unsigned!(U32, u32),
        crate::types::IntegerType::U64 => unsigned!(U64, u64),
        crate::types::IntegerType::U128 => unsigned!(U128, u128),
        crate::types::IntegerType::Usize => unsigned!(Usize, usize),
    }
}

pub(super) fn builtin_default_hir(
    ty: &Type,
    span: Span,
) -> Result<Option<HirExpression>, CompileError> {
    use rils_frontend::default::DefaultPlan;

    let Some(plan) = rils_frontend::default::default_plan(ty) else {
        return Err(CompileError::unsupported(
            format!("type `{ty}` does not implement Default"),
            span,
        ));
    };
    fn lower(plan: &DefaultPlan, span: Span) -> Result<Option<HirExpression>, CompileError> {
        let literal = |value| HirExpression::Literal { value, span };
        Ok(Some(match plan {
            DefaultPlan::Unit => literal(HirLiteral::Unit),
            DefaultPlan::Bool => literal(HirLiteral::Bool(false)),
            DefaultPlan::Integer(crate::types::IntegerType::I8) => literal(HirLiteral::I8(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I16) => literal(HirLiteral::I16(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I32) => literal(HirLiteral::I32(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I64) => literal(HirLiteral::I64(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I128) => literal(HirLiteral::I128(0)),
            DefaultPlan::Integer(crate::types::IntegerType::Isize) => literal(HirLiteral::Isize(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U8) => literal(HirLiteral::U8(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U16) => literal(HirLiteral::U16(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U32) => literal(HirLiteral::U32(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U64) => literal(HirLiteral::U64(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U128) => literal(HirLiteral::U128(0)),
            DefaultPlan::Integer(crate::types::IntegerType::Usize) => literal(HirLiteral::Usize(0)),
            DefaultPlan::Float(crate::types::FloatType::F32) => literal(HirLiteral::F32(0.0)),
            DefaultPlan::Float(crate::types::FloatType::F64) => literal(HirLiteral::F64(0.0)),
            DefaultPlan::Char => literal(HirLiteral::Char('\0')),
            DefaultPlan::String => literal(HirLiteral::String(String::new())),
            DefaultPlan::Tuple(elements) => HirExpression::Tuple {
                elements: elements
                    .iter()
                    .map(|element| {
                        lower(element, span)?.ok_or_else(|| {
                            CompileError::unsupported(
                                "nested type does not implement Default",
                                span,
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
                span,
            },
            DefaultPlan::Array {
                element, length, ..
            } => HirExpression::Array {
                elements: (0..*length)
                    .map(|_| {
                        lower(element, span)?.ok_or_else(|| {
                            CompileError::unsupported(
                                "array element does not implement Default",
                                span,
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
                repeat: None,
                span,
            },
            DefaultPlan::Option(_) => HirExpression::OptionNone { span },
            DefaultPlan::EmptyCollection { name, .. } => {
                let (name, signature) = collection_import_signature(&format!("{name}::new"))
                    .expect("default collection has a constructor import");
                HirExpression::CallImport {
                    name: name.into(),
                    signature,
                    capability: "core".into(),
                    arguments: Vec::new(),
                    span,
                }
            }
            DefaultPlan::TraitCall(_) => return Ok(None),
        }))
    }
    lower(&plan, span)
}

pub(super) fn integer_constant_literal(
    target: crate::types::IntegerType,
    constant: rils_builtins::IntegerConstantId,
) -> HirLiteral {
    use crate::types::IntegerType::*;
    use rils_builtins::IntegerConstantId::*;
    if constant == Bits {
        return HirLiteral::U32(target.bits());
    }
    match (target, constant) {
        (I8, Min) => HirLiteral::I8(i8::MIN),
        (I8, Max) => HirLiteral::I8(i8::MAX),
        (I16, Min) => HirLiteral::I16(i16::MIN),
        (I16, Max) => HirLiteral::I16(i16::MAX),
        (I32, Min) => HirLiteral::I32(i32::MIN),
        (I32, Max) => HirLiteral::I32(i32::MAX),
        (I64, Min) => HirLiteral::I64(i64::MIN),
        (I64, Max) => HirLiteral::I64(i64::MAX),
        (I128, Min) => HirLiteral::I128(i128::MIN),
        (I128, Max) => HirLiteral::I128(i128::MAX),
        (Isize, Min) => HirLiteral::Isize(isize::MIN),
        (Isize, Max) => HirLiteral::Isize(isize::MAX),
        (U8, Min) => HirLiteral::U8(u8::MIN),
        (U8, Max) => HirLiteral::U8(u8::MAX),
        (U16, Min) => HirLiteral::U16(u16::MIN),
        (U16, Max) => HirLiteral::U16(u16::MAX),
        (U32, Min) => HirLiteral::U32(u32::MIN),
        (U32, Max) => HirLiteral::U32(u32::MAX),
        (U64, Min) => HirLiteral::U64(u64::MIN),
        (U64, Max) => HirLiteral::U64(u64::MAX),
        (U128, Min) => HirLiteral::U128(u128::MIN),
        (U128, Max) => HirLiteral::U128(u128::MAX),
        (Usize, Min) => HirLiteral::Usize(usize::MIN),
        (Usize, Max) => HirLiteral::Usize(usize::MAX),
        (_, Bits) => unreachable!(),
    }
}

pub(super) fn float_constant_literal(
    target: crate::types::FloatType,
    constant: rils_builtins::FloatConstantId,
) -> HirLiteral {
    use crate::types::FloatType::*;
    use rils_builtins::FloatConstantId::*;
    match (target, constant) {
        (F32, Min) => HirLiteral::F32(f32::MIN),
        (F32, Max) => HirLiteral::F32(f32::MAX),
        (F32, Epsilon) => HirLiteral::F32(f32::EPSILON),
        (F32, MinPositive) => HirLiteral::F32(f32::MIN_POSITIVE),
        (F32, Nan) => HirLiteral::F32(f32::NAN),
        (F32, Infinity) => HirLiteral::F32(f32::INFINITY),
        (F32, NegInfinity) => HirLiteral::F32(f32::NEG_INFINITY),
        (F64, Min) => HirLiteral::F64(f64::MIN),
        (F64, Max) => HirLiteral::F64(f64::MAX),
        (F64, Epsilon) => HirLiteral::F64(f64::EPSILON),
        (F64, MinPositive) => HirLiteral::F64(f64::MIN_POSITIVE),
        (F64, Nan) => HirLiteral::F64(f64::NAN),
        (F64, Infinity) => HirLiteral::F64(f64::INFINITY),
        (F64, NegInfinity) => HirLiteral::F64(f64::NEG_INFINITY),
    }
}

pub(super) fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Module { span, .. }
        | Stmt::Use { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Impl { span, .. }
        | Stmt::Trait { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. } => *span,
        Stmt::Expr { expression, .. } => expression.span(),
    }
}
