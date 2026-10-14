use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn variant_constructor_value(
        &mut self,
        expression: &Expr,
        type_id: TypeId,
        variant: String,
        span: Span,
    ) -> Result<HirExpression, CompileError> {
        let return_type = self.constructor_type(expression, type_id)?;
        let HirTypeDefinition::Enum {
            variants,
            generic_parameters,
            ..
        } = &self.type_definitions[type_id]
        else {
            return Err(CompileError::new("expected an enum constructor", span));
        };
        let Some(EnumVariant::Tuple { fields, .. }) = variants
            .iter()
            .find(|entry| matches!(entry, EnumVariant::Tuple { name, .. } if name == &variant))
        else {
            return Err(CompileError::new(
                "expected a tuple variant constructor",
                span,
            ));
        };
        let Type::Named { arguments, .. } = &return_type else {
            unreachable!()
        };
        let bindings = generic_parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, ty)| (parameter.name.clone(), ty.clone()))
            .collect();
        let parameters: Vec<_> = fields
            .iter()
            .map(|ty| self.signature_type(ty).substitute(&bindings))
            .collect();
        let function = self.generated.next_id.get();
        self.generated.next_id.set(function + 1);
        let count = parameters.len();
        let body = HirExpression::ConstructTupleVariant {
            type_id,
            expected: return_type.clone(),
            variant: variant.clone(),
            fields: (0..count)
                .map(|local| HirExpression::Local { local, span })
                .collect(),
            span,
        };
        self.generated.functions.borrow_mut().push((
            function,
            HirFunction {
                name: format!("<constructor {variant}>"),
                exported: false,
                parameter_count: count,
                parameter_types: parameters.into_iter().map(Some).collect(),
                return_type: Some(return_type),
                capture_count: 0,
                local_count: count,
                local_mutability: vec![false; count],
                statements: vec![HirStatement::Return {
                    value: Some(body),
                    span,
                }],
                span,
            },
        ));
        Ok(HirExpression::Function { function, span })
    }

    pub(super) fn sum_constructor(
        &mut self,
        expression: &Expr,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<HirExpression, CompileError> {
        let [argument] = arguments else {
            return Err(CompileError::unsupported(
                format!("`{name}` expects exactly one argument"),
                span,
            ));
        };
        let value = Box::new(self.expression(argument)?);
        Ok(match name {
            "Some" => HirExpression::OptionSome {
                value,
                item_type: self.option_item_type(expression)?,
                span,
            },
            "Ok" => HirExpression::ResultOk {
                value,
                result_type: self.result_type(expression)?,
                span,
            },
            "Err" => HirExpression::ResultErr {
                value,
                result_type: self.result_type(expression)?,
                span,
            },
            _ => unreachable!("frontend only resolves declared sum constructors"),
        })
    }

    pub(super) fn aggregate_type(&self, expression: &Expr) -> Result<Type, CompileError> {
        self.expression_type(expression)
            .map(|ty| self.signature_type(&ty))
            .filter(Type::is_type_witness)
            .ok_or_else(|| {
                CompileError::new(
                    "cannot infer the complete aggregate type",
                    expression.span(),
                )
            })
    }

    pub(super) fn result_type(&self, expression: &Expr) -> Result<Type, CompileError> {
        if let Some(ty @ Type::Result(_, _)) = self
            .expression_type(expression)
            .map(|ty| self.signature_type(&ty))
            && ty.is_type_witness()
        {
            return Ok(ty);
        }
        Err(CompileError::unsupported(
            "cannot infer the complete Result type",
            expression.span(),
        ))
    }

    pub(super) fn option_item_type(&self, expression: &Expr) -> Result<Type, CompileError> {
        if let Some(Type::Option(inner)) = self
            .expression_type(expression)
            .map(|ty| self.signature_type(&ty))
            && inner.is_type_witness()
        {
            return Ok(*inner);
        }
        Err(CompileError::unsupported(
            "cannot infer the concrete Option item type",
            expression.span(),
        ))
    }

    pub(super) fn constructor_type(
        &self,
        expression: &Expr,
        type_id: TypeId,
    ) -> Result<Type, CompileError> {
        let (name, parameters) = match &self.type_definitions[type_id] {
            HirTypeDefinition::Struct {
                name,
                generic_parameters,
                ..
            }
            | HirTypeDefinition::Enum {
                name,
                generic_parameters,
                ..
            } => (name, generic_parameters),
        };
        if let Some(Type::Named {
            name: actual,
            arguments,
        }) = self
            .expression_type(expression)
            .map(|ty| self.signature_type(&ty))
            && actual == *name
            && arguments.len() == parameters.len()
        {
            let ty = Type::Named {
                name: actual,
                arguments,
            };
            if ty.is_type_witness() {
                return Ok(ty);
            }
        }
        if parameters.is_empty() {
            return Ok(Type::named(name));
        }
        Err(CompileError::unsupported(
            format!("cannot infer the instance type for constructor `{name}`"),
            expression.span(),
        ))
    }
}
