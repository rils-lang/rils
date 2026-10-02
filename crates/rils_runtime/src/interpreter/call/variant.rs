use super::*;

impl Interpreter {
    pub(super) fn construct_tuple_variant(
        &self,
        constructor: Rc<VariantConstructor>,
        arguments: Vec<Value>,
        span: Span,
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
        let (structs, enums) = constructor.environment.borrow().visible_type_definitions();
        let storage = crate::value::storage::TypedStorageContext::new(&structs, &enums);
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
        Ok(Value::Enum(Rc::new(EnumInstance {
            type_definition: constructor.type_definition.clone(),
            variant: constructor.variant.clone(),
            payload: EnumPayload::Tuple(values),
            type_arguments: generic_arguments(
                &constructor.type_definition.generic_parameters,
                &substitutions,
            ),
        })))
    }
}
