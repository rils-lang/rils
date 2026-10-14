use crate::{IntegerType, Type, ast::BinaryOp, value::Value};

mod float_methods;
pub(crate) mod native;
mod operators;

pub use native::float::{f32_payload, f64_payload, native_f32, native_f64};
pub use native::integer::{
    i8_payload, i16_payload, i32_payload, i64_payload, i128_payload, isize_payload, native_i8,
    native_i16, native_i32, native_i64, native_i128, native_isize, native_u8, native_u16,
    native_u32, native_u64, native_u128, native_usize, u8_payload, u16_payload, u32_payload,
    u64_payload, u128_payload, usize_payload,
};

pub fn integer_constant(target: IntegerType, constant: rils_builtins::IntegerConstantId) -> Value {
    native::integer::constant(target, constant)
        .expect("all integer constants have a native definition")
        .expect("native integer constant has no failure path")
}

pub fn float_constant(target: crate::FloatType, constant: rils_builtins::FloatConstantId) -> Value {
    float_methods::constant(target, constant)
}

pub fn cast_integer(value: Value, target: IntegerType) -> Result<Value, String> {
    let Some(Type::Integer(source_type)) = Type::of_value(&value) else {
        return Err(format!(
            "`as` expects an integer, found {}",
            value.type_name()
        ));
    };
    let source = native::integer::integer_value(&value)?;
    if !source_type.can_cast_losslessly_to(target) {
        return Err(format!(
            "cannot cast `{}` to `{target}` because the target type cannot represent the source type's full range",
            source_type
        ));
    }

    let converted = native::integer::convert_integer(&source, target);
    converted.map_err(|_| {
        format!(
            "cannot cast value from `{}` to `{target}` without losing information",
            source_type
        )
    })
}

pub fn execute_integer_intrinsic(
    symbol: &str,
    target: Option<IntegerType>,
    values: &[Value],
) -> Result<Value, String> {
    if symbol == "core::integer::try_from" && target.is_none() {
        return Err("integer try_from is missing its target type".into());
    }
    if target.is_none()
        && let Some(Value::Native(object)) = values.first()
        && matches!(object.descriptor().rils_type(), Type::Integer(_))
        && let Some(declaration) = rils_builtins::intrinsic(symbol)
        && let Some(result) = object.call(
            &format!("core::integer::{}", declaration.name),
            &values[1..],
        )
    {
        return result;
    }
    native::integer::call(symbol, target, values)
        .unwrap_or_else(|| Err("unknown integer intrinsic or receiver type".into()))
}

pub fn execute_intrinsic(
    symbol: &str,
    target: Option<IntegerType>,
    values: &[Value],
) -> Result<Value, String> {
    if float_methods::handles(symbol) {
        if target.is_some() {
            return Err("float intrinsic cannot have an integer target".into());
        }
        return float_methods::execute(symbol, values);
    }
    execute_integer_intrinsic(symbol, target, values)
}

pub fn negate(value: Value) -> Result<Value, String> {
    match Type::of_value(&value) {
        Some(Type::Integer(kind)) => native::integer::negate(kind, &value),
        Some(Type::Float(kind)) => native::float::negate(kind, &value),
        _ => Err(format!(
            "unary `-` expects a signed number, found {}",
            value.type_name()
        )),
    }
}

pub fn binary(left: Value, operator: BinaryOp, right: Value) -> Result<Value, String> {
    let ty = Type::of_value(&left);
    if ty == Type::of_value(&right) {
        match ty {
            Some(Type::Integer(kind)) => {
                return native::integer::binary(kind, &left, operator, &right);
            }
            Some(Type::Float(kind)) => return native::float::binary(kind, &left, operator, &right),
            _ => {}
        }
    }
    Err(format!(
        "operator expects numbers of the same type, found {} and {}",
        left.type_name(),
        right.type_name()
    ))
}

/// Executes an integer operation whose operand type has already been proven by the compiler.
///
/// The VM still validates dynamic values at its trust boundary, but this avoids dispatching
/// across every numeric representation for each integer instruction.
#[inline]
pub fn integer_binary_typed(
    left: Value,
    integer: IntegerType,
    operator: BinaryOp,
    right: Value,
) -> Result<Value, String> {
    if Type::of_value(&left) != Some(Type::Integer(integer))
        || Type::of_value(&right) != Some(Type::Integer(integer))
    {
        return Err(format!(
            "typed integer operator expects {integer}, found {} and {}",
            left.type_name(),
            right.type_name()
        ));
    }
    if matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual) {
        let equal = left.try_equal(&right)?;
        return Ok(Value::from(crate::value::native_bool(
            if operator == BinaryOp::Equal {
                equal
            } else {
                !equal
            },
        )));
    }
    native::integer::binary(integer, &left, operator, &right)
}
