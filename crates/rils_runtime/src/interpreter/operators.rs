use super::*;

impl Interpreter {
    pub(super) fn unary(
        &self,
        operator: UnaryOp,
        value: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match (operator, value) {
            (UnaryOp::Not, value) => Ok(Value::from(rils_execution::value::native_bool(
                !self.condition_value(&value, span)?,
            ))),
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
            return Ok(Value::from(rils_execution::value::native_bool(
                if operator == Equal { equal } else { !equal },
            )));
        }

        if operator == Add
            && let (Some(left), Some(right)) = (
                left.get_cloned::<String>().ok(),
                right.get_cloned::<String>().ok(),
            )
        {
            return Ok(Value::from(rils_execution::value::native_string(format!(
                "{left}{right}"
            ))));
        }

        crate::numeric::binary(left, operator, right)
            .map_err(|message| RuntimeError::new(message, span))
    }
}
