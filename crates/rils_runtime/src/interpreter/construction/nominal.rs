use super::*;

pub(in crate::interpreter) fn seed_arguments(
    expected: Option<&Type>,
    name: &str,
    parameters: &[crate::ast::GenericParameter],
    bindings: &mut HashMap<String, Type>,
) {
    if let Some(Type::Named {
        name: owner,
        arguments,
    }) = expected
        && owner == name
    {
        bindings.extend(
            parameters
                .iter()
                .zip(arguments)
                .map(|(parameter, argument)| (parameter.name.clone(), argument.clone())),
        );
    }
}

impl Interpreter {
    pub(in crate::interpreter) fn constructor_type(
        &self,
        expression: &Expr,
        environment: &EnvironmentRef,
    ) -> Result<Option<Type>, RuntimeError> {
        self.semantic_expression_ids
            .as_ref()
            .and_then(|ids| ids.get(expression))
            .and_then(|id| self.typeck_results.as_ref()?.expression_type(id))
            .map(|ty| {
                expand_type_aliases(
                    &ty.substitute(&environment.borrow().type_bindings()),
                    environment,
                    expression.span(),
                )
            })
            .transpose()
    }

    pub(in crate::interpreter) fn materialize_unit_constructor(
        &self,
        value: Value,
        expression: &Expr,
        environment: &EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        let Value::VariantConstructor(constructor) = &value else {
            return Ok(value);
        };
        if !constructor.type_definition.variants.iter().any(|variant| matches!(variant, EnumVariant::Unit { name, .. } if name == &constructor.variant)) {
            return Ok(value);
        }
        let span = expression.span();
        let expected = self.constructor_type(expression, environment)?;
        let mut substitutions =
            generic_substitutions(&constructor.type_definition.generic_parameters);
        seed_arguments(
            expected.as_ref(),
            &constructor.type_definition.name,
            &constructor.type_definition.generic_parameters,
            &mut substitutions,
        );
        let ty = Type::Named {
            name: constructor.type_definition.name.clone(),
            arguments: generic_arguments(
                &constructor.type_definition.generic_parameters,
                &substitutions,
            ),
        };
        let context =
            crate::runtime_builtins::NativeOwnedContext::from_environment(&environment.borrow());
        context
            .storage()
            .construct_unit_variant(&ty, &constructor.variant)
            .map_err(|message| RuntimeError::new(message, span))
    }
}
