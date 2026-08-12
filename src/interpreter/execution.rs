use super::*;

impl Interpreter {
    pub(super) fn execute_statements(
        &mut self,
        statements: &[Stmt],
        environment: EnvironmentRef,
    ) -> Result<Flow, RuntimeError> {
        for statement in statements {
            if let Stmt::TypeAlias {
                name,
                generic_parameters,
                target,
                span,
                ..
            } = unwrap_public(statement)
            {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                environment.borrow_mut().define(
                    name.clone(),
                    Value::TypeAlias(Rc::new(TypeAliasType {
                        name: name.clone(),
                        generic_parameters: generic_parameters.clone(),
                        target: target.clone(),
                    })),
                    false,
                    None,
                );
            }
        }
        let mut result = Value::Unit;
        for statement in statements {
            self.tick(statement_span(statement))?;
            match self.execute_statement(statement, environment.clone())? {
                Flow::Value(value) => result = value,
                flow @ (Flow::Return(_) | Flow::Break(_) | Flow::Continue) => return Ok(flow),
            }
            if let Some(value) = self.pending_return.take() {
                return Ok(Flow::Return(value));
            }
            if let Some(flow) = self.pending_loop_flow.take() {
                return Ok(flow);
            }
        }
        Ok(Flow::Value(result))
    }

    pub(super) fn execute_statement(
        &mut self,
        statement: &Stmt,
        environment: EnvironmentRef,
    ) -> Result<Flow, RuntimeError> {
        match statement {
            Stmt::Public { statement, .. } => self.execute_statement(statement, environment),
            Stmt::Module {
                name,
                statements,
                span,
                ..
            } => {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                let Some(statements) = statements else {
                    return Err(RuntimeError::new(
                        format!(
                            "external module `{name}` has not been loaded; use Engine::eval_file or an inline module"
                        ),
                        *span,
                    ));
                };
                let members = Environment::module_child(environment.clone());
                self.execute_statements(statements, members.clone())?;
                let public = statements
                    .iter()
                    .filter_map(public_name)
                    .collect::<std::collections::HashSet<_>>();
                environment.borrow_mut().define(
                    name.clone(),
                    Value::Module(Rc::new(ModuleValue {
                        name: name.clone(),
                        members,
                        public: RefCell::new(public),
                    })),
                    false,
                    None,
                );
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Use {
                path, alias, span, ..
            } => {
                let value = resolve_visible_path(path, &environment, *span)?;
                let name = alias
                    .clone()
                    .unwrap_or_else(|| path.last().expect("use path is non-empty").clone());
                if environment.borrow().contains_local(&name) {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                environment.borrow_mut().define(name, value, false, None);
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Let {
                name,
                mutable,
                type_annotation,
                initializer,
                span,
                ..
            } => {
                let type_annotation = type_annotation
                    .as_ref()
                    .map(|ty| expand_type_aliases(ty, &environment, *span))
                    .transpose()?;
                let value = self.evaluate(initializer, environment.clone())?;
                if matches!(value, Value::Reference(_)) {
                    if Rc::ptr_eq(&environment, &self.globals) {
                        return Err(RuntimeError::new(
                            "references cannot be stored in global bindings",
                            *span,
                        ));
                    }
                    if environment.borrow().has_local_function() {
                        return Err(RuntimeError::new(
                            "a reference cannot be introduced after a closure in the same scope",
                            *span,
                        ));
                    }
                }
                if value.contains_reference() && !matches!(value, Value::Reference(_)) {
                    return Err(RuntimeError::new(
                        "references cannot be stored inside owned values",
                        *span,
                    ));
                }
                if type_annotation.is_none()
                    && matches!(
                        &value,
                        Value::Option {
                            element_type: None,
                            ..
                        }
                    )
                {
                    return Err(RuntimeError::new(
                        format!(
                            "cannot infer the element type of `{name}`; declare it as `Option<T>`"
                        ),
                        *span,
                    ));
                }
                let value = apply_type(type_annotation.as_ref(), &value, *span, name)?;
                environment
                    .borrow_mut()
                    .define(name.clone(), value, *mutable, type_annotation);
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Function {
                name,
                generic_parameters,
                parameters,
                return_type,
                body,
                ..
            } => {
                if environment.borrow().has_visible_reference() {
                    return Err(RuntimeError::new(
                        "functions cannot capture local references",
                        body.span,
                    ));
                }
                let mut parameters = parameters.clone();
                for parameter in &mut parameters {
                    if let Some(annotation) = &parameter.type_annotation {
                        parameter.type_annotation = Some(expand_type_aliases(
                            annotation,
                            &environment,
                            parameter.span,
                        )?);
                    }
                }
                let return_type = return_type
                    .as_ref()
                    .map(|ty| expand_type_aliases(ty, &environment, body.span))
                    .transpose()?;
                let function = Value::Function(Rc::new(UserFunction {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    parameters,
                    return_type,
                    body: body.clone(),
                    closure: environment.clone(),
                }));
                environment
                    .borrow_mut()
                    .define(name.clone(), function, false, None);
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Struct {
                name,
                generic_parameters,
                fields,
                span,
                ..
            } => {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                let fields = fields
                    .iter()
                    .map(|field| {
                        let mut field = field.clone();
                        field.type_annotation =
                            expand_type_aliases(&field.type_annotation, &environment, field.span)?;
                        Ok(field)
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                let definition = StructType {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    fields,
                    methods: Default::default(),
                    trait_methods: Default::default(),
                    implemented_traits: Default::default(),
                    associated_types: Default::default(),
                };
                environment.borrow_mut().define(
                    name.clone(),
                    Value::StructType(Rc::new(definition)),
                    false,
                    None,
                );
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Enum {
                name,
                generic_parameters,
                variants,
                span,
                ..
            } => {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                let variants = variants
                    .iter()
                    .map(|variant| match variant {
                        EnumVariant::Unit { .. } => Ok(variant.clone()),
                        EnumVariant::Tuple { name, fields, span } => Ok(EnumVariant::Tuple {
                            name: name.clone(),
                            fields: fields
                                .iter()
                                .map(|field| expand_type_aliases(field, &environment, *span))
                                .collect::<Result<Vec<_>, _>>()?,
                            span: *span,
                        }),
                        EnumVariant::Record { name, fields, span } => Ok(EnumVariant::Record {
                            name: name.clone(),
                            fields: fields
                                .iter()
                                .map(|field| {
                                    let mut field = field.clone();
                                    field.type_annotation = expand_type_aliases(
                                        &field.type_annotation,
                                        &environment,
                                        field.span,
                                    )?;
                                    Ok(field)
                                })
                                .collect::<Result<Vec<_>, RuntimeError>>()?,
                            span: *span,
                        }),
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                let definition = EnumType {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    variants,
                    methods: Default::default(),
                    trait_methods: Default::default(),
                    implemented_traits: Default::default(),
                    associated_types: Default::default(),
                };
                environment.borrow_mut().define(
                    name.clone(),
                    Value::EnumType(Rc::new(definition)),
                    false,
                    None,
                );
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::TypeAlias { .. } => Ok(Flow::Value(Value::Unit)),
            Stmt::Trait {
                name,
                associated_types,
                methods,
                span,
                ..
            } => {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                for method in methods {
                    if let Some(self_index) = method
                        .parameters
                        .iter()
                        .position(|parameter| parameter.name == "self")
                        && self_index != 0
                    {
                        return Err(RuntimeError::new(
                            "`self` must be the first trait method parameter",
                            method.span,
                        ));
                    }
                }
                let associated_types = associated_types
                    .iter()
                    .map(|associated| {
                        let mut associated = associated.clone();
                        associated.value = associated
                            .value
                            .as_ref()
                            .map(|value| expand_type_aliases(value, &environment, associated.span))
                            .transpose()?;
                        Ok(associated)
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                let methods = methods
                    .iter()
                    .map(|method| {
                        let mut method = method.clone();
                        for parameter in &mut method.parameters {
                            if let Some(annotation) = &parameter.type_annotation {
                                parameter.type_annotation = Some(expand_type_aliases(
                                    annotation,
                                    &environment,
                                    parameter.span,
                                )?);
                            }
                        }
                        method.return_type = method
                            .return_type
                            .as_ref()
                            .map(|value| expand_type_aliases(value, &environment, method.span))
                            .transpose()?;
                        Ok(method)
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                environment.borrow_mut().define(
                    name.clone(),
                    Value::TraitType(Rc::new(TraitType {
                        name: name.clone(),
                        associated_types,
                        methods,
                    })),
                    false,
                    None,
                );
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Impl {
                generic_parameters,
                trait_name,
                target,
                associated_types,
                methods,
                span,
            } => {
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
                        environment
                            .borrow()
                            .get(trait_name)
                            .ok_or_else(|| {
                                RuntimeError::new(format!("unknown trait `{trait_name}`"), *span)
                            })
                            .and_then(|value| match value {
                                Value::TraitType(definition) => Ok(definition),
                                _ => Err(RuntimeError::new(
                                    format!("`{trait_name}` is not a trait"),
                                    *span,
                                )),
                            })
                    })
                    .transpose()?;
                let associated_type_values = if let Some(definition) = &trait_definition {
                    let mut values = HashMap::new();
                    for required in &definition.associated_types {
                        let implementation = associated_types
                            .iter()
                            .find(|item| item.name == required.name);
                        let (generic_parameters, value, value_span) = if let Some(implementation) =
                            implementation
                        {
                            if implementation.generic_parameters.len()
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
                        } else {
                            return Err(RuntimeError::new(
                                format!(
                                    "impl of trait `{}` is missing associated type `{}`",
                                    definition.name, required.name
                                ),
                                *span,
                            ));
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
                    if let Some(extra) = associated_types.iter().find(|item| {
                        !definition
                            .associated_types
                            .iter()
                            .any(|required| required.name == item.name)
                    }) {
                        return Err(RuntimeError::new(
                            format!(
                                "associated type `{}` is not a member of trait `{}`",
                                extra.name, definition.name
                            ),
                            extra.span,
                        ));
                    }
                    values
                } else {
                    HashMap::new()
                };
                if let Some(definition) = &trait_definition {
                    if generic_parameters
                        .iter()
                        .any(|parameter| !parameter.bounds.is_empty())
                    {
                        return Err(RuntimeError::new(
                            "conditional trait impl bounds are not supported yet",
                            *span,
                        ));
                    }
                    validate_trait_implementation(
                        definition,
                        &associated_type_values,
                        methods,
                        target,
                        *span,
                    )?;
                    if definition.name == "Copy"
                        && !type_implements_trait(target, "Copy", &environment)
                    {
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
                    if implemented.borrow().contains(&definition.name) {
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
                        body: method.body.clone(),
                        closure: environment.clone(),
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
            Stmt::While {
                condition,
                body,
                span,
            } => {
                while {
                    let value = self.evaluate(condition, environment.clone())?;
                    self.condition_value(&value, *span)?
                } {
                    match self.execute_block(body, environment.clone())? {
                        Flow::Value(_) => {}
                        returned @ Flow::Return(_) => return Ok(returned),
                        Flow::Break(value) => return Ok(Flow::Value(value)),
                        Flow::Continue => continue,
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Loop { body, .. } => loop {
                match self.execute_block(body, environment.clone())? {
                    Flow::Value(_) | Flow::Continue => {}
                    returned @ Flow::Return(_) => return Ok(returned),
                    Flow::Break(value) => return Ok(Flow::Value(value)),
                }
            },
            Stmt::For {
                binding,
                iterable,
                body,
                span,
                ..
            } => {
                let value = self.evaluate(iterable, environment.clone())?;
                let iterator = if matches!(&value, Value::Range(_)) {
                    value
                } else if Type::of_value(&value)
                    .is_some_and(|ty| type_implements_trait(&ty, "IntoIterator", &environment))
                {
                    let method = self.resolve_member(value, "into_iter", *span)?;
                    self.call(method, &[], *span)?
                } else {
                    value
                };
                let iterator_type = Type::of_value(&iterator).ok_or_else(|| {
                    RuntimeError::new("for-loop value has no runtime type", *span)
                })?;
                if !type_implements_trait(&iterator_type, "Iterator", &environment) {
                    return Err(RuntimeError::new(
                        format!("type `{iterator_type}` does not implement Iterator"),
                        *span,
                    ));
                }

                if let Value::Range(range) = &iterator {
                    let mut range = range.clone();
                    while let Some(current) = range
                        .next()
                        .map_err(|message| RuntimeError::new(message, *span))?
                    {
                        self.tick(*span)?;
                        let iteration_environment = Environment::child(environment.clone());
                        iteration_environment.borrow_mut().define(
                            binding.clone(),
                            current,
                            false,
                            Some(range.element_type()),
                        );
                        match self.execute_block(body, iteration_environment)? {
                            Flow::Value(_) => {}
                            returned @ Flow::Return(_) => return Ok(returned),
                            Flow::Break(value) => return Ok(Flow::Value(value)),
                            Flow::Continue => continue,
                        }
                    }
                    return Ok(Flow::Value(Value::Unit));
                }

                let loop_environment = Environment::child(environment.clone());
                let iterator_name = "#rils_for_iterator";
                loop_environment.borrow_mut().define(
                    iterator_name,
                    iterator,
                    true,
                    Some(iterator_type),
                );

                loop {
                    self.tick(*span)?;
                    let slot = loop_environment
                        .borrow()
                        .slot(iterator_name)
                        .expect("for-loop iterator slot exists");
                    let receiver =
                        Value::Reference(Rc::new(ReferenceValue::new_storage(slot, true)));
                    let method = self.resolve_member(receiver, "next", *span)?;
                    let next = self.call(method, &[], *span)?;
                    let item = match next {
                        Value::Option { value: None, .. } => break,
                        Value::Option {
                            value: Some(value), ..
                        } => match Rc::try_unwrap(value) {
                            Ok(value) => value,
                            Err(value) => value
                                .clone_owned()
                                .map_err(|message| RuntimeError::new(message, *span))?,
                        },
                        value => {
                            return Err(RuntimeError::new(
                                format!(
                                    "Iterator::next must return Option, found {}",
                                    value.type_name()
                                ),
                                *span,
                            ));
                        }
                    };

                    let iteration_environment = Environment::child(loop_environment.clone());
                    iteration_environment
                        .borrow_mut()
                        .define(binding.clone(), item, false, None);
                    match self.execute_block(body, iteration_environment)? {
                        Flow::Value(_) => {}
                        returned @ Flow::Return(_) => return Ok(returned),
                        Flow::Break(value) => return Ok(Flow::Value(value)),
                        Flow::Continue => continue,
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Return { value, span } => {
                if self.function_depth == 0 {
                    return Err(RuntimeError::new(
                        "`return` can only be used inside a function",
                        *span,
                    ));
                }
                let value = value
                    .as_ref()
                    .map(|expression| self.evaluate(expression, environment))
                    .transpose()?
                    .unwrap_or(Value::Unit);
                if value.contains_reference() {
                    return Err(RuntimeError::new(
                        "references cannot be returned from functions",
                        *span,
                    ));
                }
                Ok(Flow::Return(value))
            }
            Stmt::Break { value, span } => {
                let value = value
                    .as_ref()
                    .map(|expression| self.evaluate(expression, environment))
                    .transpose()?
                    .unwrap_or(Value::Unit);
                if value.contains_reference() {
                    return Err(RuntimeError::new(
                        "references cannot escape a loop through `break`",
                        *span,
                    ));
                }
                Ok(Flow::Break(value))
            }
            Stmt::Continue { .. } => Ok(Flow::Continue),
            Stmt::Expr {
                expression,
                terminated,
            } => {
                let value = self.evaluate(expression, environment)?;
                Ok(Flow::Value(if *terminated { Value::Unit } else { value }))
            }
        }
    }
}

fn unwrap_public(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}

fn public_name(statement: &Stmt) -> Option<String> {
    let Stmt::Public { statement, .. } = statement else {
        return None;
    };
    match statement.as_ref() {
        Stmt::Function { name, .. }
        | Stmt::Struct { name, .. }
        | Stmt::Enum { name, .. }
        | Stmt::TypeAlias { name, .. }
        | Stmt::Trait { name, .. }
        | Stmt::Module { name, .. } => Some(name.clone()),
        Stmt::Use { path, alias, .. } => alias.clone().or_else(|| path.last().cloned()),
        _ => None,
    }
}

pub(super) fn resolve_visible_path(
    path: &[String],
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Value, RuntimeError> {
    let (environment, path) = anchored_environment(path, environment, span)?;
    let Some((first, rest)) = path.split_first() else {
        return Err(RuntimeError::new("empty path", span));
    };
    let mut value = environment
        .borrow()
        .get(first)
        .ok_or_else(|| RuntimeError::new(format!("undefined name `{first}`"), span))?;
    for segment in rest {
        let Value::Module(module) = value else {
            return Err(RuntimeError::new(
                format!("`{segment}` cannot be selected from {}", value.type_name()),
                span,
            ));
        };
        if !module.public.borrow().contains(segment) {
            return Err(RuntimeError::new(
                format!("module `{}` has no public member `{segment}`", module.name),
                span,
            ));
        }
        value = module.members.borrow().get(segment).ok_or_else(|| {
            RuntimeError::new(
                format!("module `{}` is missing member `{segment}`", module.name),
                span,
            )
        })?;
    }
    Ok(value)
}

pub(super) fn anchored_environment<'a>(
    path: &'a [String],
    environment: &EnvironmentRef,
    span: Span,
) -> Result<(EnvironmentRef, &'a [String]), RuntimeError> {
    let mut target = environment.clone();
    let mut index = 0;
    while let Some(segment) = path.get(index) {
        match segment.as_str() {
            "crate" => {
                target = Environment::root(environment);
                index += 1;
            }
            "self" => {
                target = Environment::current_module(environment)
                    .unwrap_or_else(|| Environment::root(environment));
                index += 1;
            }
            "super" => {
                target = Environment::parent_module(&target).ok_or_else(|| {
                    RuntimeError::new("`super` cannot escape the crate root", span)
                })?;
                index += 1;
            }
            _ => break,
        }
    }
    Ok((target, &path[index..]))
}
