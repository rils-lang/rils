use super::*;

impl FunctionLowerer<'_> {
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
