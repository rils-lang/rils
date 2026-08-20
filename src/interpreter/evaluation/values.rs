use super::super::*;

impl Interpreter {
    pub(super) fn evaluate_sequence(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Tuple { elements, span } => {
                let mut slots = Vec::with_capacity(elements.len());
                for element in elements {
                    let value = self.evaluate(element, environment.clone())?;
                    if value.contains_reference() {
                        return Err(RuntimeError::new(
                            "tuple values cannot own local references",
                            *span,
                        ));
                    }
                    let ty = Type::of_value(&value).unwrap_or(Type::Unknown);
                    slots.push(FieldSlot {
                        value: Some(value),
                        type_annotation: ty,
                        references: 0,
                    });
                }
                Ok(Value::Tuple(Rc::new(SequenceValue {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(None),
                })))
            }
            Expr::Array {
                elements,
                repeat,
                span,
            } => {
                let mut values = Vec::new();
                if let Some(count) = repeat {
                    let value = self.evaluate(&elements[0], environment.clone())?;
                    if value.contains_reference() {
                        return Err(RuntimeError::new(
                            "arrays cannot own local references",
                            *span,
                        ));
                    }
                    if !value.is_copy() {
                        return Err(RuntimeError::new(
                            "array repeat syntax requires a Copy value",
                            *span,
                        ));
                    }
                    let count = self.evaluate(count, environment.clone())?;
                    let Value::Usize(count) = count else {
                        return Err(RuntimeError::new("array repeat count must be usize", *span));
                    };
                    for _ in 0..count {
                        values.push(
                            value
                                .clone_owned()
                                .map_err(|message| RuntimeError::new(message, *span))?,
                        );
                    }
                } else {
                    for element in elements {
                        let value = self.evaluate(element, environment.clone())?;
                        if value.contains_reference() {
                            return Err(RuntimeError::new(
                                "arrays cannot own local references",
                                *span,
                            ));
                        }
                        values.push(value);
                    }
                }
                let mut element_type = Type::Unknown;
                for value in &values {
                    let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                    element_type = merge_types(&element_type, &actual).ok_or_else(|| {
                        RuntimeError::new(
                            format!(
                                "array elements must have one type, found `{element_type}` and `{actual}`"
                            ),
                            *span,
                        )
                    })?;
                }
                let slots = values
                    .into_iter()
                    .map(|value| FieldSlot {
                        value: Some(value),
                        type_annotation: element_type.clone(),
                        references: 0,
                    })
                    .collect();
                Ok(Value::Array(Rc::new(SequenceValue {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(Some(element_type)),
                })))
            }
            _ => unreachable!("sequence evaluator received a non-sequence expression"),
        }
    }
}
