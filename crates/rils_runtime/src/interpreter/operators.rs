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
            let equal = left == right;
            return Ok(Value::Bool(if operator == Equal { equal } else { !equal }));
        }

        if operator == Add
            && let (Value::String(left), Value::String(right)) = (&left, &right)
        {
            return Ok(Value::String(Rc::from(format!("{left}{right}"))));
        }

        crate::numeric::binary(left, operator, right)
            .map_err(|message| RuntimeError::new(message, span))
    }
}
