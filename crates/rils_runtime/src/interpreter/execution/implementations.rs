use super::*;

impl Interpreter {
    pub(super) fn execute_implementation(
        &mut self,
        statement: &Stmt,
        environment: EnvironmentRef,
    ) -> Result<Flow, RuntimeError> {
        match statement {
            Stmt::Impl {
                generic_parameters,
                trait_name,
                trait_arguments,
                target,
                associated_types,
                methods,
                span,
                ..
            } => {
                for associated in associated_types {
                    if let Some(ty) = &associated.value {
                        expand_source_type(ty, &environment, associated.span)?;
                    }
                }
                let target = expand_source_type(target, &environment, *span)?;
                let target = &target;
                let Type::Named {
                    name: target_name, ..
                } = target
                else {
                    unreachable!("parser validates impl targets");
                };
                let target_value = environment.borrow().get(target_name).ok_or_else(|| {
                    RuntimeError::new(format!("unknown impl target `{target_name}`"), *span)
                })?;
                let trait_definition = trait_name
                    .as_ref()
                    .map(|trait_name| {
                        let path = trait_name
                            .split("::")
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        resolve_visible_path(&path, &environment, *span).and_then(|value| {
                            match value {
                                Value::TraitType(definition) => Ok(definition),
                                _ => Err(RuntimeError::new(
                                    format!("`{trait_name}` is not a trait"),
                                    *span,
                                )),
                            }
                        })
                    })
                    .transpose()?;
                if let Some(definition) = &trait_definition {
                    if rils_stdlib::stdlib::ops::callable_trait_kind(&definition.name).is_some() {
                        return Err(RuntimeError::new(
                            format!(
                                "callable trait `{}` is sealed and cannot be implemented manually",
                                definition.name
                            ),
                            *span,
                        ));
                    }
                    if definition.generic_parameters.len() != trait_arguments.len() {
                        return Err(RuntimeError::new(
                            format!(
                                "trait `{}` expects {} type arguments, found {}",
                                definition.name,
                                definition.generic_parameters.len(),
                                trait_arguments.len()
                            ),
                            *span,
                        ));
                    }
                }
                let frontend_impl_verified = self
                    .frontend_impl_ids
                    .get(span)
                    .is_some_and(|id| self.frontend_verified_trait_impls.contains(id));
                let associated_type_values = if let Some(definition) = &trait_definition {
                    let mut values = HashMap::new();
                    for required in &definition.associated_types {
                        let implementation = associated_types
                            .iter()
                            .find(|item| item.name == required.name);
                        let (generic_parameters, value, value_span) = if let Some(implementation) =
                            implementation
                        {
                            if !frontend_impl_verified
                                && implementation.generic_parameters.len()
                                    != required.generic_parameters.len()
                            {
                                return Err(RuntimeError::new(
                                    format!(
                                        "associated type `{}` has the wrong number of generic parameters",
                                        required.name
                                    ),
                                    implementation.span,
                                ));
                            }
                            (
                                implementation.generic_parameters.clone(),
                                implementation
                                    .value
                                    .as_ref()
                                    .expect("impl associated types require values"),
                                implementation.span,
                            )
                        } else if let Some(default) = &required.value {
                            (required.generic_parameters.clone(), default, required.span)
                        } else if !frontend_impl_verified {
                            return Err(RuntimeError::new(
                                format!(
                                    "impl of trait `{}` is missing associated type `{}`",
                                    definition.name, required.name
                                ),
                                *span,
                            ));
                        } else {
                            unreachable!("frontend verified all required associated types")
                        };
                        values.insert(
                            required.name.clone(),
                            TypeAliasType {
                                name: required.name.clone(),
                                generic_parameters,
                                target: expand_type_aliases(value, &environment, value_span)?,
                            },
                        );
                    }
                    if !frontend_impl_verified
                        && let Some(extra) = associated_types.iter().find(|item| {
                            !definition
                                .associated_types
                                .iter()
                                .any(|required| required.name == item.name)
                        })
                    {
                        return Err(RuntimeError::new(
                            format!(
                                "associated type `{}` is not a member of trait `{}`",
                                extra.name, definition.name
                            ),
                            extra.span,
                        ));
                    }
                    if definition.name == "IntoIterator"
                        && let (Some(item), Some(iterator)) =
                            (values.get("Item"), values.get("IntoIter"))
                    {
                        let iterator = &iterator.target;
                        let expected =
                            rils_frontend::standard_library::builtin_iterator_item_type(iterator)
                                .or_else(|| {
                                    type_implements_trait(iterator, "Iterator", &environment)
                                        .then(|| Type::Associated {
                                            base: Box::new(iterator.clone()),
                                            trait_name: Some("Iterator".into()),
                                            name: "Item".into(),
                                            arguments: Vec::new(),
                                        })
                                        .and_then(|projection| {
                                            expand_type_aliases(&projection, &environment, *span)
                                                .ok()
                                        })
                                });
                        if expected.is_none()
                            && !type_implements_trait(iterator, "Iterator", &environment)
                        {
                            return Err(RuntimeError::new(
                                format!(
                                    "IntoIterator::IntoIter `{iterator}` must implement Iterator"
                                ),
                                *span,
                            ));
                        }
                        if let Some(expected) = expected
                            && item.target != expected
                        {
                            return Err(RuntimeError::new(
                                format!(
                                    "IntoIterator::Item must match IntoIter::Item: expected `{expected}`, found `{}`",
                                    item.target
                                ),
                                *span,
                            ));
                        }
                    }
                    values
                } else {
                    HashMap::new()
                };
                if let Some(definition) = &trait_definition {
                    if definition.name == "BitFlags" {
                        return Err(RuntimeError::new(
                            "BitFlags is reserved for host enums marked as flags",
                            *span,
                        ));
                    }
                    if !self.frontend_semantics_verified
                        && generic_parameters
                            .iter()
                            .any(|parameter| !parameter.bounds.is_empty())
                    {
                        return Err(RuntimeError::new(
                            "conditional trait impl bounds are not supported yet",
                            *span,
                        ));
                    }
                    if !self.frontend_semantics_verified {
                        for bound in &definition.bounds {
                            if !type_implements_trait(target, bound, &environment) {
                                return Err(RuntimeError::new(
                                    format!(
                                        "type `{target}` must implement supertrait `{bound}` before implementing `{}`",
                                        definition.name
                                    ),
                                    *span,
                                ));
                            }
                        }
                    }
                    if !frontend_impl_verified {
                        validate_trait_implementation(
                            definition,
                            trait_arguments,
                            &associated_type_values,
                            methods,
                            target,
                            *span,
                            &environment,
                        )?;
                    }
                    if definition.name == "Copy"
                        && let Type::Named { arguments, .. } = target
                        && {
                            let mut variables = HashSet::new();
                            arguments.iter().any(|argument| match argument {
                                Type::Variable(name) | Type::BoundVariable { name, .. } => {
                                    !variables.insert(name)
                                }
                                _ => true,
                            })
                        }
                    {
                        return Err(RuntimeError::new(
                            "conditional Copy implementations are not supported yet",
                            *span,
                        ));
                    }
                    if definition.name == "Copy" && !copy_fields_eligible(target, &environment) {
                        return Err(RuntimeError::new(
                            format!(
                                "`{target}` cannot implement Copy because it contains non-Copy fields"
                            ),
                            *span,
                        ));
                    }
                    let implemented = implemented_traits(&target_value).ok_or_else(|| {
                        RuntimeError::new(format!("`{target_name}` is not a struct or enum"), *span)
                    })?;
                    let target_type_name = match &target_value {
                        Value::StructType(definition) => &definition.name,
                        Value::EnumType(definition) => &definition.name,
                        _ => unreachable!("implementation target was validated"),
                    };
                    let pending = self
                        .pending_value_traits
                        .remove(&(target_type_name.clone(), definition.name.clone()));
                    if implemented.borrow().contains(&definition.name) && !pending {
                        return Err(RuntimeError::new(
                            format!(
                                "trait `{}` is already implemented for `{target_name}`",
                                definition.name
                            ),
                            *span,
                        ));
                    }
                }
                for method in methods {
                    let method_environment = Environment::child(environment.clone());
                    method_environment.borrow_mut().define(
                        "Self",
                        target_value.clone(),
                        false,
                        None,
                    );
                    let mut parameters = method.parameters.clone();
                    if let Some(self_index) = parameters
                        .iter()
                        .position(|parameter| parameter.name == "self")
                    {
                        if self_index != 0 {
                            return Err(RuntimeError::new(
                                "`self` must be the first method parameter",
                                method.span,
                            ));
                        }
                        if parameters[0].type_annotation.is_none() {
                            parameters[0].type_annotation = Some(target.clone());
                        }
                    }
                    for parameter in &mut parameters {
                        if let Some(annotation) = &mut parameter.type_annotation {
                            expand_source_type(annotation, &environment, parameter.span)?;
                            *annotation = expand_type_aliases(
                                &substitute_associated(annotation, target, &associated_type_values),
                                &environment,
                                parameter.span,
                            )?;
                        }
                    }
                    let return_type = method
                        .return_type
                        .as_ref()
                        .map(|return_type| {
                            expand_source_type(return_type, &environment, method.span)?;
                            expand_type_aliases(
                                &substitute_associated(
                                    return_type,
                                    target,
                                    &associated_type_values,
                                ),
                                &environment,
                                method.span,
                            )
                        })
                        .transpose()?;
                    let mut function_generics = generic_parameters.clone();
                    for generic in &method.generic_parameters {
                        if function_generics
                            .iter()
                            .any(|existing| existing.name == generic.name)
                        {
                            return Err(RuntimeError::new(
                                format!("duplicate generic parameter `{}`", generic.name),
                                method.span,
                            ));
                        }
                        function_generics.push(generic.clone());
                    }
                    let function_body = method.body.clone();
                    let semantic_expression_ids = self
                        .semantic_expression_ids
                        .as_ref()
                        .map(|ids| ids.for_cloned_block(&method.body, &function_body));
                    let function = Rc::new(UserFunction {
                        name: trait_definition.as_ref().map_or_else(
                            || format!("{target_name}::{}", method.name),
                            |definition| {
                                format!("<{target_name} as {}>::{}", definition.name, method.name)
                            },
                        ),
                        generic_parameters: function_generics,
                        parameters,
                        return_type,
                        body: function_body,
                        closure: method_environment,
                        semantic_expression_ids,
                    });
                    let (inherent_methods, trait_methods) = match &target_value {
                        Value::StructType(definition) => {
                            if trait_definition.is_none()
                                && definition
                                    .fields
                                    .iter()
                                    .any(|field| field.name == method.name)
                            {
                                return Err(RuntimeError::new(
                                    format!(
                                        "method `{target_name}::{}` conflicts with a field",
                                        method.name
                                    ),
                                    method.span,
                                ));
                            }
                            (&definition.methods, &definition.trait_methods)
                        }
                        Value::EnumType(definition) => {
                            if trait_definition.is_none()
                                && definition
                                    .variants
                                    .iter()
                                    .any(|variant| enum_variant_name(variant) == method.name)
                            {
                                return Err(RuntimeError::new(
                                    format!(
                                        "method `{target_name}::{}` conflicts with an enum variant",
                                        method.name
                                    ),
                                    method.span,
                                ));
                            }
                            (&definition.methods, &definition.trait_methods)
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                format!("`{target_name}` is not a struct or enum"),
                                *span,
                            ));
                        }
                    };
                    if let Some(trait_definition) = &trait_definition {
                        let mut all_trait_methods = trait_methods.borrow_mut();
                        let trait_table = all_trait_methods
                            .entry(trait_definition.name.clone())
                            .or_default();
                        if trait_table.contains_key(&method.name) {
                            return Err(RuntimeError::new(
                                format!(
                                    "duplicate method `<{target_name} as {}>::{}`",
                                    trait_definition.name, method.name
                                ),
                                method.span,
                            ));
                        }
                        trait_table.insert(method.name.clone(), function);
                    } else {
                        if inherent_methods.borrow().contains_key(&method.name) {
                            return Err(RuntimeError::new(
                                format!("duplicate method `{target_name}::{}`", method.name),
                                method.span,
                            ));
                        }
                        inherent_methods
                            .borrow_mut()
                            .insert(method.name.clone(), function);
                    }
                }
                if let Some(definition) = trait_definition {
                    match &target_value {
                        Value::StructType(target_definition) => {
                            target_definition
                                .associated_types
                                .borrow_mut()
                                .insert(definition.name.clone(), associated_type_values.clone());
                        }
                        Value::EnumType(target_definition) => {
                            target_definition
                                .associated_types
                                .borrow_mut()
                                .insert(definition.name.clone(), associated_type_values.clone());
                        }
                        _ => {}
                    }
                    implemented_traits(&target_value)
                        .expect("validated nominal impl target")
                        .borrow_mut()
                        .insert(definition.name.clone());
                }
                Ok(Flow::Value(Value::Unit))
            }
            _ => unreachable!("implementation handler requires an impl statement"),
        }
    }
}
