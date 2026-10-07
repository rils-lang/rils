use super::*;

mod implementations;

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
            } = statement
            {
                if environment.borrow().get(name).is_some() {
                    return Err(RuntimeError::new(
                        format!("name `{name}` is already defined"),
                        *span,
                    ));
                }
                let declaration_name = environment.borrow().qualified_type_name(name);
                environment.borrow_mut().define(
                    name.clone(),
                    Value::TypeAlias(Rc::new(TypeAliasType {
                        name: declaration_name,
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
                Ok(Flow::Return(value))
            }
            Stmt::Expr {
                expression,
                terminated,
            } => {
                let value = self.evaluate(expression, environment)?;
                Ok(Flow::Value(if *terminated { Value::Unit } else { value }))
            }
            _ => self.execute_non_expression_statement(statement, environment),
        }
    }

    fn execute_non_expression_statement(
        &mut self,
        statement: &Stmt,
        environment: EnvironmentRef,
    ) -> Result<Flow, RuntimeError> {
        match statement {
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
                let members = Environment::named_module_child(environment.clone(), name);
                self.execute_statements(statements, members.clone())?;
                let mut public = std::collections::HashSet::new();
                for statement in statements {
                    public.extend(public_names(statement, &members)?);
                }
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
            Stmt::Use { imports, .. } => {
                for import in imports {
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let value = resolve_visible_path(&import.path, &environment, import.span)?;
                        let Value::Module(module) = value else {
                            return Err(RuntimeError::new(
                                "glob import target must be a module",
                                import.span,
                            ));
                        };
                        let mut names = module.public.borrow().iter().cloned().collect::<Vec<_>>();
                        names.sort();
                        if let Some(name) = names
                            .iter()
                            .find(|name| environment.borrow().contains_local(name))
                        {
                            return Err(RuntimeError::new(
                                format!("name `{name}` is already defined"),
                                import.span,
                            ));
                        }
                        let values = names
                            .into_iter()
                            .map(|name| {
                                let value =
                                    module.members.borrow().get(&name).ok_or_else(|| {
                                        RuntimeError::new(
                                            format!("module export `{name}` is unavailable"),
                                            import.span,
                                        )
                                    })?;
                                Ok((name, value))
                            })
                            .collect::<Result<Vec<_>, RuntimeError>>()?;
                        for (name, value) in values {
                            environment.borrow_mut().define(name, value, false, None);
                        }
                        continue;
                    }
                    let value = resolve_visible_path(&import.path, &environment, import.span)?;
                    let name = import.binding_name().expect("single import").to_owned();
                    if environment.borrow().contains_local(&name) {
                        return Err(RuntimeError::new(
                            format!("name `{name}` is already defined"),
                            import.span,
                        ));
                    }
                    environment.borrow_mut().define(name, value, false, None);
                }
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
                    .map(|ty| expand_source_type(ty, &environment, *span))
                    .transpose()?;
                let inferred_type = self
                    .semantic_expression_ids
                    .as_ref()
                    .and_then(|ids| ids.get(initializer))
                    .and_then(|id| self.typeck_results.as_ref()?.expression_type(id))
                    .map(|ty| expand_type_aliases(ty, &environment, *span))
                    .transpose()?;
                let contextual_type = type_annotation.as_ref().or(inferred_type.as_ref());
                let value = match contextual_type.and_then(|expected| {
                    self.construct_contextual_empty(initializer, expected, &environment)
                }) {
                    Some(value) => {
                        self.tick(initializer.span())?;
                        value?
                    }
                    None => self.evaluate(initializer, environment.clone())?,
                };
                if value.contains_reference() {
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
                let mut value = apply_type_owned(type_annotation.as_ref(), value, *span, name)?;
                if let Some(expected) = contextual_type {
                    let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                        &environment.borrow(),
                    );
                    value = context
                        .storage()
                        .apply_declared(value, expected)
                        .map_err(|message| RuntimeError::new(message, *span))?;
                }
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
                        parameter.type_annotation = Some(expand_source_type(
                            annotation,
                            &environment,
                            parameter.span,
                        )?);
                    }
                }
                let return_type = return_type
                    .as_ref()
                    .map(|ty| expand_source_type(ty, &environment, body.span))
                    .transpose()?;
                let function_body = body.clone();
                let semantic_expression_ids = self
                    .semantic_expression_ids
                    .as_ref()
                    .map(|ids| ids.for_cloned_block(body, &function_body));
                let function = Value::Function(Rc::new(UserFunction {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    parameters,
                    return_type,
                    body: function_body,
                    closure: environment.clone(),
                    semantic_expression_ids,
                    typeck_results: self.typeck_results.clone(),
                }));
                environment
                    .borrow_mut()
                    .define(name.clone(), function, false, None);
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Struct {
                name,
                attributes,
                generic_parameters,
                fields,
                span,
                ..
            } => {
                let existing = environment.borrow().get(name);
                let shadows_builtin = matches!(&existing, Some(Value::StructType(definition))
                    if definition.opaque_native
                        && definition.fields.is_empty()
                        && rils_builtins::builtin(name).is_some_and(|builtin| builtin.opaque_native));
                if existing.is_some() && !shadows_builtin {
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
                            expand_source_type(&field.type_annotation, &environment, field.span)?;
                        Ok(field)
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                let declaration_name = environment.borrow().qualified_type_name(name);
                let declared_traits = environment
                    .borrow()
                    .declared_value_traits(&declaration_name);
                self.pending_value_traits.extend(
                    declared_traits
                        .iter()
                        .map(|trait_name| (declaration_name.clone(), trait_name.clone())),
                );
                let definition = StructType {
                    field_indices: Default::default(),
                    name: declaration_name,
                    opaque_native: crate::ast::has_compiler_internal_attribute(attributes),
                    generic_parameters: generic_parameters.clone(),
                    fields,
                    methods: Default::default(),
                    trait_methods: Default::default(),
                    implemented_traits: RefCell::new(declared_traits),
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
                                .map(|field| expand_source_type(field, &environment, *span))
                                .collect::<Result<Vec<_>, _>>()?,
                            span: *span,
                        }),
                        EnumVariant::Record { name, fields, span } => Ok(EnumVariant::Record {
                            name: name.clone(),
                            fields: fields
                                .iter()
                                .map(|field| {
                                    let mut field = field.clone();
                                    field.type_annotation = expand_source_type(
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
                let declaration_name = environment.borrow().qualified_type_name(name);
                let declared_traits = environment
                    .borrow()
                    .declared_value_traits(&declaration_name);
                self.pending_value_traits.extend(
                    declared_traits
                        .iter()
                        .map(|trait_name| (declaration_name.clone(), trait_name.clone())),
                );
                let definition = EnumType {
                    host_definition: None,
                    name: declaration_name,
                    generic_parameters: generic_parameters.clone(),
                    variants,
                    methods: Default::default(),
                    trait_methods: Default::default(),
                    implemented_traits: RefCell::new(declared_traits),
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
                generic_parameters,
                bounds,
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
                            .map(|value| expand_source_type(value, &environment, associated.span))
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
                                parameter.type_annotation = Some(expand_source_type(
                                    annotation,
                                    &environment,
                                    parameter.span,
                                )?);
                            }
                        }
                        method.return_type = method
                            .return_type
                            .as_ref()
                            .map(|value| expand_source_type(value, &environment, method.span))
                            .transpose()?;
                        Ok(method)
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?;
                let declaration_name = environment.borrow().qualified_type_name(name);
                environment.borrow_mut().define(
                    name.clone(),
                    Value::TraitType(Rc::new(TraitType {
                        name: declaration_name,
                        generic_parameters: generic_parameters.clone(),
                        bounds: bounds.clone(),
                        associated_types,
                        methods,
                    })),
                    false,
                    None,
                );
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Impl { .. } => self.execute_implementation(statement, environment),
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
                let iterator = if Type::of_value(&value)
                    .is_some_and(|ty| type_implements_trait(&ty, "IntoIterator", &environment))
                {
                    let method = self.resolve_member(value, "into_iter", *span)?;
                    self.call_owned(method, Vec::new(), *span, environment.clone())?
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
                    let (structs, enums) = loop_environment.borrow().visible_type_definitions();
                    let item = rils_execution::value::dynamic_option::take_owned_with_definitions(
                        next, &structs, &enums,
                    )
                    .map_err(|message| RuntimeError::new(message, *span))?;
                    let Some(item) = item else { break };

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
            Stmt::Return { .. } | Stmt::Expr { .. } => {
                unreachable!("return and expression statements use the fast path")
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
        }
    }
}

pub(super) fn public_names(
    statement: &Stmt,
    environment: &EnvironmentRef,
) -> Result<Vec<String>, RuntimeError> {
    if !statement
        .visibility()
        .is_some_and(|visibility| visibility.is_public())
    {
        return Ok(Vec::new());
    }
    Ok(match statement {
        Stmt::Function { name, .. }
        | Stmt::Struct { name, .. }
        | Stmt::Enum { name, .. }
        | Stmt::TypeAlias { name, .. }
        | Stmt::Trait { name, .. }
        | Stmt::Module { name, .. } => vec![name.clone()],
        Stmt::Use { imports, .. } => {
            let mut names = Vec::new();
            for import in imports {
                if let Some(name) = import.binding_name() {
                    names.push(name.to_owned());
                } else {
                    let value = resolve_visible_path(&import.path, environment, import.span)?;
                    let Value::Module(module) = value else {
                        return Err(RuntimeError::new(
                            "glob import target must be a module",
                            import.span,
                        ));
                    };
                    names.extend(module.public.borrow().iter().cloned());
                }
            }
            names
        }
        _ => Vec::new(),
    })
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
