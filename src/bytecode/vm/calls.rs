use super::*;

impl VirtualMachine<'_> {
    pub(super) fn iterator_methods(&self, value: &Value) -> Option<BytecodeIteratorMethods> {
        let name = match value {
            Value::Struct(instance) => &instance.type_definition.name,
            Value::Enum(instance) => &instance.type_definition.name,
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
        for (local, argument) in locals.iter().zip(arguments) {
            local.borrow_mut().initialize(argument);
        }
        self.frames.push(Frame {
            function,
            registers: vec![None; callee.register_count],
            locals,
            instruction: 0,
            return_action,
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
                let Value::Option { value, .. } = value else {
                    return Err(BytecodeError::new(
                        "Iterator::next must return Option",
                        span,
                    ));
                };
                if let Some(value) = value {
                    let value = Rc::try_unwrap(value)
                        .or_else(|value| value.clone_owned())
                        .map_err(|message| BytecodeError::new(message, span))?;
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
