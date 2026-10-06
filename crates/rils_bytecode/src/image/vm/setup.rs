use super::*;

impl<'a> VirtualMachine<'a> {
    pub(super) fn type_definitions(
        &self,
    ) -> (
        Vec<Rc<rils_execution::value::StructType>>,
        Vec<Rc<rils_execution::value::EnumType>>,
    ) {
        (
            self.native_context.structs.clone(),
            self.native_context.enums.clone(),
        )
    }

    pub(in crate::image) fn new(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        mut native_context: crate::runtime_builtins::NativeOwnedContext,
        limits: crate::ExecutionLimits,
    ) -> Result<Self, BytecodeError> {
        register_type_traits(module);
        extend_module_types(&mut native_context, module);
        validate_copy_types(module, &native_context)?;
        let entry = &module.functions[module.entry];
        Ok(Self {
            module,
            imports,
            host_value_formatter,
            native_context,
            frames: vec![Frame {
                function: module.entry,
                registers: vec![None; entry.register_count],
                locals: new_local_storage(entry),
                instruction: 0,
                return_action: ReturnAction::Complete,
                return_type: None,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: true,
        })
    }

    pub(in crate::image) fn new_call(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        mut native_context: crate::runtime_builtins::NativeOwnedContext,
        limits: crate::ExecutionLimits,
        function: usize,
        arguments: Vec<Value>,
    ) -> Result<Self, BytecodeError> {
        register_type_traits(module);
        extend_module_types(&mut native_context, module);
        validate_copy_types(module, &native_context)?;
        let callee = &module.functions[function];
        if callee.capture_count != 0 {
            return Err(BytecodeError::new(
                format!("function `{}` requires a closure environment", callee.name),
                callee.span,
            ));
        }
        if arguments.len() != callee.parameter_count {
            return Err(BytecodeError::new(
                format!(
                    "function `{}` expects {} arguments, found {}",
                    callee.name,
                    callee.parameter_count,
                    arguments.len()
                ),
                callee.span,
            ));
        }
        let locals = new_local_storage(callee);
        let return_type = returns::resolve_return_type(callee, arguments.iter());
        for (local, argument) in locals.iter().zip(arguments) {
            local.borrow_mut().initialize(argument);
        }
        Ok(Self {
            module,
            imports,
            host_value_formatter,
            native_context,
            frames: vec![Frame {
                function,
                registers: vec![None; callee.register_count],
                locals,
                instruction: 0,
                return_action: ReturnAction::Complete,
                return_type,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: false,
        })
    }
}

fn register_type_traits(module: &BytecodeModule) {
    for implementation in &module.trait_implementations {
        if !matches!(implementation.trait_name.as_str(), "Eq" | "Hash" | "Copy") {
            continue;
        }
        for ty in &module.types {
            match ty {
                RuntimeType::Struct(definition) if definition.name == implementation.target => {
                    definition
                        .implemented_traits
                        .borrow_mut()
                        .insert(implementation.trait_name.clone());
                }
                RuntimeType::Enum(definition) if definition.name == implementation.target => {
                    definition
                        .implemented_traits
                        .borrow_mut()
                        .insert(implementation.trait_name.clone());
                }
                _ => {}
            }
        }
    }
}

fn extend_module_types(
    context: &mut crate::runtime_builtins::NativeOwnedContext,
    module: &BytecodeModule,
) {
    for definition in &module.types {
        match definition {
            RuntimeType::Struct(definition) => {
                if let Some(existing) = context
                    .structs
                    .iter_mut()
                    .find(|existing| existing.name == definition.name)
                {
                    *existing = definition.clone();
                } else {
                    context.structs.push(definition.clone());
                }
            }
            RuntimeType::Enum(definition) => {
                if let Some(existing) = context
                    .enums
                    .iter_mut()
                    .find(|existing| existing.name == definition.name)
                {
                    *existing = definition.clone();
                } else {
                    context.enums.push(definition.clone());
                }
            }
        }
    }
}

fn validate_copy_types(
    module: &BytecodeModule,
    context: &crate::runtime_builtins::NativeOwnedContext,
) -> Result<(), BytecodeError> {
    for implementation in module
        .trait_implementations
        .iter()
        .filter(|implementation| implementation.trait_name == "Copy")
    {
        let parameters = module
            .types
            .iter()
            .find_map(|ty| match ty {
                RuntimeType::Struct(definition) if definition.name == implementation.target => {
                    Some(&definition.generic_parameters)
                }
                RuntimeType::Enum(definition) if definition.name == implementation.target => {
                    Some(&definition.generic_parameters)
                }
                _ => None,
            })
            .expect("Copy target was verified");
        let ty = Type::Named {
            name: implementation.target.clone(),
            arguments: parameters
                .iter()
                .map(|parameter| Type::Variable(parameter.name.clone()))
                .collect(),
        };
        if !context.copy_fields_eligible(&ty) {
            return Err(BytecodeError::new(
                format!("`{ty}` cannot implement Copy because it contains non-Copy fields"),
                Span::default(),
            ));
        }
    }
    Ok(())
}
