use super::*;

impl Interpreter {
    pub(in crate::interpreter) fn construct_tuple_variant(
        &self,
        constructor: Rc<VariantConstructor>,
        arguments: Vec<Value>,
        span: Span,
        expected: Option<&Type>,
    ) -> Result<Value, RuntimeError> {
        let variant = constructor
            .type_definition
            .variants
            .iter()
            .find(|variant| enum_variant_name(variant) == constructor.variant)
            .expect("constructor refers to declared variant");
        let EnumVariant::Tuple { fields, .. } = variant else {
            return Err(RuntimeError::new(
                format!(
                    "{}::{} must be constructed with named fields",
                    constructor.type_definition.name, constructor.variant
                ),
                span,
            ));
        };
        check_arity(
            &format!(
                "{}::{}",
                constructor.type_definition.name, constructor.variant
            ),
            fields.len(),
            fields.len(),
            arguments.len(),
            span,
        )?;
        let mut substitutions =
            generic_substitutions(&constructor.type_definition.generic_parameters);
        super::super::construction::nominal::seed_arguments(
            expected,
            &constructor.type_definition.name,
            &constructor.type_definition.generic_parameters,
            &mut substitutions,
        );
        for (field_type, value) in fields.iter().zip(&arguments) {
            infer_type_from_value(field_type, value, &mut substitutions)
                .map_err(|message| RuntimeError::new(message, span))?;
        }
        validate_generic_bounds(
            &constructor.type_definition.generic_parameters,
            &substitutions,
            None,
            &constructor.environment,
            span,
        )?;
        let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
            &constructor.environment.borrow(),
        );
        let storage = context.storage();
        let values = fields
            .iter()
            .zip(arguments)
            .enumerate()
            .map(|(index, (field_type, value))| {
                let expected = field_type.substitute(&substitutions);
                let value = apply_type_owned(
                    Some(&expected),
                    value,
                    span,
                    &format!("variant field {index}"),
                )?;
                storage
                    .apply_declared(value, &expected)
                    .map_err(|message| RuntimeError::new(message, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ty = Type::Named {
            name: constructor.type_definition.name.clone(),
            arguments: generic_arguments(
                &constructor.type_definition.generic_parameters,
                &substitutions,
            ),
        };
        storage
            .construct_tuple_variant(&ty, &constructor.variant, values)
            .map_err(|message| RuntimeError::new(message, span))
    }
}
