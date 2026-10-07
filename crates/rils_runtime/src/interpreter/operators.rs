use super::*;

impl Interpreter {
    pub(super) fn unary(
        &self,
        operator: UnaryOp,
        value: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match (operator, value) {
            (UnaryOp::Not, value) => Ok(Value::Bool(!self.condition_value(&value, span)?)),
            (UnaryOp::Negate, value) => {
                crate::numeric::negate(value).map_err(|message| RuntimeError::new(message, span))
            }
            (UnaryOp::Dereference, _) => unreachable!("dereference is handled during evaluation"),
        }
    }

    pub(super) fn binary(
        &self,
        left: Value,
        operator: BinaryOp,
        right: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;

        if matches!(operator, Equal | NotEqual) {
            let equal = left
                .try_equal(&right)
                .map_err(|message| RuntimeError::new(message, span))?;
            return Ok(Value::Bool(if operator == Equal { equal } else { !equal }));
        }

        if operator == Add
            && let (Some(left), Some(right)) = (left.as_string(), right.as_string())
        {
            return Ok(rils_execution::value::native_string(format!(
                "{left}{right}"
            )));
        }

        crate::numeric::binary(left, operator, right)
            .map_err(|message| RuntimeError::new(message, span))
    }
}
