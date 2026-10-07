use super::*;

impl FunctionLowerer<'_> {
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
