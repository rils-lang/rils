use super::*;

impl VirtualMachine<'_> {
    pub(super) fn call_native_symbol(
        &mut self,
        symbol: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let formatter_member =
            rils_builtins::native_member_owner(symbol).and_then(|(owner, member)| {
                rils_builtins::builtin("Formatter")
                    .filter(|formatter| std::ptr::eq(owner, *formatter))
                    .map(|_| member.name)
            });
        match formatter_member {
            Some("write_str") => {
                let [buffer, value] = arguments else {
                    return Err(BytecodeError::new(
                        "Formatter::write_str expects a receiver and string",
                        span,
                    ));
                };
                let buffer = crate::formatting::buffer_from_value(buffer)
                    .map_err(|message| BytecodeError::new(message, span))?;
                let value = value.as_string().ok_or_else(|| {
                    BytecodeError::new("Formatter::write_str expects string", span)
                })?;
                buffer.write_str(&value);
                return Ok(super::super::formatting::format_ok());
            }
            Some("write_derived_debug") => {
                let [buffer, value] = arguments else {
                    return Err(BytecodeError::new(
                        "Formatter::write_derived_debug expects two arguments",
                        span,
                    ));
                };
                return self.write_derived_debug_builtin(buffer, value, span);
            }
            _ => {}
        }
        crate::runtime_builtins::call_native_symbol(symbol, arguments)
            .ok_or_else(|| {
                BytecodeError::new(format!("native method `{symbol}` is unavailable"), span)
            })?
            .map_err(|message| BytecodeError::new(message, span))
    }

    pub(super) fn call_native_owned_symbol(
        &mut self,
        symbol: &str,
        arguments: Vec<Value>,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let context = self.native_context.clone();
        let result = crate::runtime_builtins::call_native_symbol_with_callback(
            symbol,
            arguments,
            &context,
            expected,
            &mut |function, values| self.invoke_native_callback(function, values, span),
        )
        .ok_or_else(|| {
            BytecodeError::new(format!("native method `{symbol}` is unavailable"), span)
        })?;
        result.map_err(|error| match error {
            crate::runtime_builtins::NativeCallError::Bridge(message) => {
                BytecodeError::new(message, span)
            }
            crate::runtime_builtins::NativeCallError::Callback(error) => error,
        })
    }

    fn invoke_native_callback(
        &mut self,
        function: &Value,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let Value::BytecodeFunction(function) = function else {
            return Err(BytecodeError::new(
                format!("{} is not callable", function.type_name()),
                span,
            ));
        };
        if arguments.len() != function.parameter_count {
            return Err(BytecodeError::new(
                format!(
                    "function `{}` expects {} arguments, found {}",
                    function.name,
                    function.parameter_count,
                    arguments.len()
                ),
                span,
            ));
        }
        self.ensure_call_capacity(span)?;
        let callee = &self.module.functions[function.function];
        if function.captures.len() != callee.capture_count
            || function.bound_arguments.len() + arguments.len() != callee.parameter_count
        {
            return Err(BytecodeError::new(
                "closure environment does not match function layout",
                span,
            ));
        }
        let mut call_arguments = function.bound_arguments.clone();
        call_arguments.extend(arguments);
        let mut locals = new_local_storage(callee);
        let mut type_bindings = function.type_bindings.clone();
        type_bindings.extend(returns::resolve_type_bindings(
            callee,
            call_arguments.iter(),
        ));
        let return_type = callee
            .return_type
            .as_ref()
            .map(|ty| ty.substitute(&type_bindings));
        for (local, capture) in locals.iter_mut().zip(&function.captures) {
            *local = capture.clone();
        }
        for (local, argument) in locals.iter().skip(callee.capture_count).zip(call_arguments) {
            local.borrow_mut().initialize(argument);
        }
        let active_depth = self.frames.len() - usize::from(self.root_is_module_entry);
        let mut nested = VirtualMachine {
            module: self.module,
            imports: self.imports.clone(),
            host_value_formatter: self.host_value_formatter.clone(),
            native_context: self.native_context.clone(),
            frames: vec![Frame {
                function: function.function,
                registers: vec![None; callee.register_count],
                locals,
                instruction: 0,
                return_action: ReturnAction::Complete,
                return_type,
                type_bindings,
            }],
            steps: self.steps,
            max_steps: self.max_steps,
            max_call_depth: self.max_call_depth - active_depth,
            root_is_module_entry: false,
        };
        let result = nested.execute();
        self.steps = nested.steps;
        result
    }

    pub(super) fn iterator_methods(&self, value: &Value) -> Option<BytecodeIteratorMethods> {
        let ty = Type::of_value(value)?;
        let name = match &ty {
            Type::Named { name, .. } => name,
            _ => return None,
        };
        self.module.iterators.get(name).cloned()
    }

    pub(super) fn script_iterator(&self, value: Value, span: Span) -> Result<Value, BytecodeError> {
        let methods = self.iterator_methods(&value).ok_or_else(|| {
            BytecodeError::new(
                format!("{} does not implement Iterator", value.type_name()),
                span,
            )
        })?;
        let next_function = methods.next.ok_or_else(|| {
            BytecodeError::new(
                format!("{} does not implement Iterator", value.type_name()),
                span,
            )
        })?;
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        storage.borrow_mut().initialize(value);
        Ok(Value::BytecodeIterator(Rc::new(BytecodeIteratorValue {
            storage,
            next_function,
        })))
    }

    pub(super) fn push_script_call(
        &mut self,
        function: usize,
        arguments: Vec<Value>,
        return_action: ReturnAction,
        span: Span,
    ) -> Result<(), BytecodeError> {
        self.ensure_call_capacity(span)?;
        let callee = &self.module.functions[function];
        if callee.capture_count != 0 || callee.parameter_count != arguments.len() {
            return Err(BytecodeError::new("invalid iterator method layout", span));
        }
        let locals = new_local_storage(callee);
        let type_bindings = returns::resolve_type_bindings(callee, arguments.iter());
        let return_type = callee
            .return_type
            .as_ref()
            .map(|ty| ty.substitute(&type_bindings));
        for (local, argument) in locals.iter().zip(arguments) {
            local.borrow_mut().initialize(argument);
        }
        self.frames.push(Frame {
            function,
            registers: vec![None; callee.register_count],
            locals,
            instruction: 0,
            return_action,
            return_type,
            type_bindings,
        });
        Ok(())
    }

    pub(super) fn ensure_call_capacity(&self, span: Span) -> Result<(), BytecodeError> {
        let call_depth = self.frames.len() - usize::from(self.root_is_module_entry);
        if call_depth >= self.max_call_depth {
            return Err(BytecodeError::new(
                format!(
                    "call stack exceeded the {} frame limit",
                    self.max_call_depth
                ),
                span,
            ));
        }
        Ok(())
    }

    pub(super) fn finish_return(
        &mut self,
        value: Value,
        span: Span,
    ) -> Result<Option<Value>, BytecodeError> {
        let frame = self.frames.pop().expect("return has an active frame");
        let value = if let Some(expected) = frame.return_type {
            self.native_context
                .storage()
                .apply_declared(value, &expected)
                .map_err(|message| BytecodeError::new(message, span))?
        } else {
            value
        };
        match frame.return_action {
            ReturnAction::Complete => Ok(Some(value)),
            ReturnAction::Register(destination) => {
                self.frame_mut().registers[destination] = Some(value);
                Ok(None)
            }
            ReturnAction::IntoIterator { destination } => {
                let iterator = self.script_iterator(value, span)?;
                self.frame_mut().registers[destination] = Some(iterator);
                Ok(None)
            }
            ReturnAction::IteratorNext {
                destination,
                some_target,
                none_target,
            } => {
                let value = crate::value::dynamic_option::take_owned_with_definitions(
                    value,
                    &self.native_context.structs,
                    &self.native_context.enums,
                )
                .map_err(|message| BytecodeError::new(message, span))?;
                if let Some(value) = value {
                    self.frame_mut().registers[destination] = Some(value);
                    self.frame_mut().instruction = some_target;
                } else {
                    self.frame_mut().instruction = none_target;
                }
                Ok(None)
            }
        }
    }
}
