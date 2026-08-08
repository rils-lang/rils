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
            (UnaryOp::Negate, Value::Integer(value)) => value
                .checked_neg()
                .map(Value::Integer)
                .ok_or_else(|| RuntimeError::new("integer overflow", span)),
            (UnaryOp::Negate, Value::Float(value)) => Ok(Value::Float(-value)),
            (UnaryOp::Negate, value) => Err(RuntimeError::new(
                format!("unary `-` expects a number, found {}", value.type_name()),
                span,
            )),
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

        match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                self.binary_integers(left, operator, right, span)
            }
            (Value::Integer(left), Value::Float(right)) => {
                self.binary_floats(left as f64, operator, right, span)
            }
            (Value::Float(left), Value::Integer(right)) => {
                self.binary_floats(left, operator, right as f64, span)
            }
            (Value::Float(left), Value::Float(right)) => {
                self.binary_floats(left, operator, right, span)
            }
            (left, right) => Err(RuntimeError::new(
                format!(
                    "operator expects compatible numbers, found {} and {}",
                    left.type_name(),
                    right.type_name()
                ),
                span,
            )),
        }
    }

    pub(super) fn binary_integers(
        &self,
        left: i64,
        operator: BinaryOp,
        right: i64,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;
        let checked = |value: Option<i64>| {
            value
                .map(Value::Integer)
                .ok_or_else(|| RuntimeError::new("integer overflow", span))
        };
        match operator {
            Add => checked(left.checked_add(right)),
            Subtract => checked(left.checked_sub(right)),
            Multiply => checked(left.checked_mul(right)),
            Divide if right == 0 => Err(RuntimeError::new("division by zero", span)),
            Divide => checked(left.checked_div(right)),
            Remainder if right == 0 => Err(RuntimeError::new("division by zero", span)),
            Remainder => checked(left.checked_rem(right)),
            Greater => Ok(Value::Bool(left > right)),
            GreaterEqual => Ok(Value::Bool(left >= right)),
            Less => Ok(Value::Bool(left < right)),
            LessEqual => Ok(Value::Bool(left <= right)),
            Equal | NotEqual => unreachable!(),
        }
    }

    pub(super) fn binary_floats(
        &self,
        left: f64,
        operator: BinaryOp,
        right: f64,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;
        match operator {
            Add => Ok(Value::Float(left + right)),
            Subtract => Ok(Value::Float(left - right)),
            Multiply => Ok(Value::Float(left * right)),
            Divide if right == 0.0 => Err(RuntimeError::new("division by zero", span)),
            Divide => Ok(Value::Float(left / right)),
            Remainder if right == 0.0 => Err(RuntimeError::new("division by zero", span)),
            Remainder => Ok(Value::Float(left % right)),
            Greater => Ok(Value::Bool(left > right)),
            GreaterEqual => Ok(Value::Bool(left >= right)),
            Less => Ok(Value::Bool(left < right)),
            LessEqual => Ok(Value::Bool(left <= right)),
            Equal | NotEqual => unreachable!(),
        }
    }
}
