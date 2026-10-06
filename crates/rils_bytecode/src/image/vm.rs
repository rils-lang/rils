use super::*;

mod calls;
mod places;
mod records;
mod returns;
mod setup;
mod storage;

pub(super) struct Frame {
    function: usize,
    registers: Vec<Option<Value>>,
    locals: Vec<StorageRef>,
    instruction: usize,
    return_action: ReturnAction,
    return_type: Option<Type>,
}

enum ReturnAction {
    Complete,
    Register(usize),
    IntoIterator {
        destination: usize,
    },
    IteratorNext {
        destination: usize,
        some_target: usize,
        none_target: usize,
    },
}

pub(super) struct VirtualMachine<'a> {
    pub(super) module: &'a BytecodeModule,
    pub(super) imports: Vec<Rc<BytecodeHostHandler>>,
    pub(super) host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
    pub(super) native_context: crate::runtime_builtins::NativeOwnedContext,
    pub(super) frames: Vec<Frame>,
    pub(super) steps: usize,
    pub(super) max_steps: usize,
    pub(super) max_call_depth: usize,
    root_is_module_entry: bool,
}

impl<'a> VirtualMachine<'a> {
    pub(super) fn execute(&mut self) -> Result<Value, BytecodeError> {
        loop {
            let frame = self.frames.last().expect("VM always has an active frame");
            let function = &self.module.functions[frame.function];
            let instruction = function
                .instructions
                .get(frame.instruction)
                .ok_or_else(|| {
                    BytecodeError::new(
                        format!(
                            "instruction pointer is out of bounds in `{}`",
                            function.name
                        ),
                        function.span,
                    )
                })?;
            let instruction = instruction.clone();
            self.steps += 1;
            if self.steps > self.max_steps {
                return Err(BytecodeError::new(
                    format!(
                        "execution exceeded the {limit} step limit",
                        limit = self.max_steps
                    ),
                    instruction.span,
                ));
            }
            self.frame_mut().instruction += 1;
            match instruction.instruction {
                Instruction::LoadConstant {
                    destination,
                    constant,
                } => {
                    let function = self.current_function();
                    let value = function.constants[constant].value();
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::LoadFunction {
                    destination,
                    function,
                } => {
                    let callee = &self.module.functions[function];
                    self.frame_mut().registers[destination] =
                        Some(Value::BytecodeFunction(Rc::new(BytecodeFunctionValue {
                            function,
                            name: callee.name.clone(),
                            parameter_count: callee.parameter_count,
                            captures: Vec::new(),
                            bound_arguments: Vec::new(),
                        })));
                }
                Instruction::BindMethod {
                    destination,
                    function,
                    receiver,
                } => {
                    let receiver = self.take_register(receiver, instruction.span)?;
                    let callee = &self.module.functions[function];
                    self.frame_mut().registers[destination] =
                        Some(Value::BytecodeFunction(Rc::new(BytecodeFunctionValue {
                            function,
                            name: callee.name.clone(),
                            parameter_count: callee.parameter_count - 1,
                            captures: Vec::new(),
                            bound_arguments: vec![receiver],
                        })));
                }
                Instruction::BorrowTemporary {
                    destination,
                    source,
                    mutable,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(mutable)));
                    storage.borrow_mut().initialize(value);
                    self.frame_mut().registers[destination] = Some(Value::Reference(Rc::new(
                        ReferenceValue::new_storage(storage, mutable),
                    )));
                }
                Instruction::Reborrow {
                    destination,
                    source,
                    mutable,
                } => {
                    let reference = self.take_register(source, instruction.span)?;
                    let Value::Reference(reference) = reference else {
                        return Err(BytecodeError::new(
                            "reborrow target is not a reference",
                            instruction.span,
                        ));
                    };
                    let reference = reference
                        .reborrow(mutable)
                        .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    self.frame_mut().registers[destination] =
                        Some(Value::Reference(Rc::new(reference)));
                }
                Instruction::CreateClosure {
                    destination,
                    function,
                    captures,
                } => {
                    let callee = &self.module.functions[function];
                    let captures = captures
                        .into_iter()
                        .map(|local| self.frame().locals[local].clone())
                        .collect();
                    self.frame_mut().registers[destination] =
                        Some(Value::BytecodeFunction(Rc::new(BytecodeFunctionValue {
                            function,
                            name: callee.name.clone(),
                            parameter_count: callee.parameter_count,
                            captures,
                            bound_arguments: Vec::new(),
                        })));
                }
                Instruction::TakePlace { destination, place } => {
                    let place = self.resolve_place(place, instruction.span)?;
                    let value = self.take_place(&place, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::TakeLocal { destination, local } => {
                    let value = self.frame().locals[local]
                        .borrow_mut()
                        .take()
                        .map_err(|error| access_error(error, instruction.span))?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::StoreLocal { local, source } => {
                    let value = self.take_register(source, instruction.span)?;
                    self.frame().locals[local]
                        .borrow_mut()
                        .assign(value)
                        .map_err(|error| assign_error(error, instruction.span))?;
                }
                Instruction::InitLocal {
                    local,
                    source,
                    type_annotation,
                } => {
                    let mut value = self.take_register(source, instruction.span)?;
                    if let Some(expected) = type_annotation {
                        value = self
                            .native_context
                            .storage()
                            .apply_declared(value, &expected)
                            .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    }
                    self.frame().locals[local].borrow_mut().initialize(value);
                }
                Instruction::DropLocal { local } => {
                    self.frame().locals[local].borrow_mut().clear();
                }
                Instruction::BorrowLocal {
                    destination,
                    local,
                    mutable,
                } => {
                    let slot = self.frame().locals[local].clone();
                    {
                        let storage = slot.borrow();
                        storage
                            .read()
                            .map_err(|error| access_error(error, instruction.span))?;
                        if mutable && !storage.is_mutable() {
                            return Err(BytecodeError::new(
                                "cannot mutably borrow immutable local",
                                instruction.span,
                            ));
                        }
                    }
                    self.frame_mut().registers[destination] = Some(Value::Reference(Rc::new(
                        ReferenceValue::new_storage(slot, mutable),
                    )));
                }
                Instruction::BorrowPlace {
                    destination,
                    place,
                    mutable,
                } => {
                    if mutable && !self.place_is_mutable(place.local, instruction.span)? {
                        return Err(BytecodeError::new(
                            "cannot mutably borrow through immutable local",
                            instruction.span,
                        ));
                    }
                    let place = self.resolve_place(place, instruction.span)?;
                    let reference = self.place_reference(&place, mutable, instruction.span)?;
                    self.frame_mut().registers[destination] =
                        Some(Value::Reference(Rc::new(reference)));
                }
                Instruction::Dereference {
                    destination,
                    source,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let Value::Reference(reference) = value else {
                        return Err(BytecodeError::new(
                            "cannot dereference a non-reference value",
                            instruction.span,
                        ));
                    };
                    let value = reference
                        .read()
                        .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    if !value.is_copy() {
                        return Err(BytecodeError::new(
                            "cannot move a non-Copy value out of a reference",
                            instruction.span,
                        ));
                    }
                    self.frame_mut().registers[destination] = Some(
                        value
                            .clone_owned()
                            .map_err(|message| BytecodeError::new(message, instruction.span))?,
                    );
                }
                Instruction::StoreDereference { reference, source } => {
                    let reference = self.take_register(reference, instruction.span)?;
                    let value = self.take_register(source, instruction.span)?;
                    let Value::Reference(reference) = reference else {
                        return Err(BytecodeError::new(
                            "assignment target is not a reference",
                            instruction.span,
                        ));
                    };
                    reference
                        .write(value)
                        .map_err(|error| assign_error(error, instruction.span))?;
                }
                Instruction::StorePlace { place, source } => {
                    if !self.place_is_mutable(place.local, instruction.span)? {
                        return Err(BytecodeError::new(
                            "cannot assign through immutable local",
                            instruction.span,
                        ));
                    }
                    let place = self.resolve_place(place, instruction.span)?;
                    let value = self.take_register(source, instruction.span)?;
                    self.store_place(&place, value, instruction.span)?;
                }
                Instruction::IntoIterator {
                    destination,
                    source,
                } => {
                    let source = self.take_register(source, instruction.span)?;
                    let context = &self.native_context;
                    let iterator = match rils_execution::iteration::into_iterator_with_context(
                        source, context,
                    )
                    .map_err(|message| BytecodeError::new(message, instruction.span))?
                    {
                        rils_execution::iteration::IntoIteratorResult::Ready(iterator) => iterator,
                        rils_execution::iteration::IntoIteratorResult::UserDefined(value) => {
                            let methods = self.iterator_methods(&value).ok_or_else(|| {
                                BytecodeError::new(
                                    format!(
                                        "{} does not implement IntoIterator",
                                        value.type_name()
                                    ),
                                    instruction.span,
                                )
                            })?;
                            if let Some(function) = methods.into_iter {
                                self.push_script_call(
                                    function,
                                    vec![value],
                                    ReturnAction::IntoIterator { destination },
                                    instruction.span,
                                )?;
                                continue;
                            }
                            self.script_iterator(value, instruction.span)?
                        }
                    };
                    self.frame_mut().registers[destination] = Some(iterator);
                }
                Instruction::Move {
                    destination,
                    source,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::Unary {
                    destination,
                    operator,
                    operand,
                } => {
                    let operand = self.take_register(operand, instruction.span)?;
                    self.frame_mut().registers[destination] =
                        Some(unary(operator, operand, instruction.span)?);
                }
                Instruction::Cast {
                    destination,
                    source,
                    target,
                } => {
                    let source = self.take_register(source, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(
                        crate::numeric::cast_integer(source, target)
                            .map_err(|message| BytecodeError::new(message, instruction.span))?,
                    );
                }
                Instruction::Binary {
                    destination,
                    left,
                    operator,
                    right,
                } => {
                    let left = self.take_register(left, instruction.span)?;
                    let right = self.take_register(right, instruction.span)?;
                    self.frame_mut().registers[destination] =
                        Some(binary(left, operator, right, instruction.span)?);
                }
                Instruction::IntegerBinary {
                    destination,
                    left,
                    operator,
                    right,
                    integer,
                } => {
                    let left = self.take_register(left, instruction.span)?;
                    let right = self.take_register(right, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(
                        crate::numeric::integer_binary_typed(left, integer, operator, right)
                            .map_err(|message| BytecodeError::new(message, instruction.span))?,
                    );
                }
                Instruction::Call {
                    destination,
                    function,
                    arguments,
                } => {
                    self.ensure_call_capacity(instruction.span)?;
                    let arguments = arguments
                        .into_iter()
                        .map(|register| self.take_register(register, instruction.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let callee = &self.module.functions[function];
                    let locals = new_local_storage(callee);
                    let return_type = returns::resolve_return_type(callee, arguments.iter());
                    for (local, argument) in locals.iter().zip(arguments) {
                        local.borrow_mut().initialize(argument);
                    }
                    self.frames.push(Frame {
                        function,
                        registers: vec![None; callee.register_count],
                        locals,
                        instruction: 0,
                        return_action: ReturnAction::Register(destination),
                        return_type,
                    });
                }
                Instruction::CallValue {
                    destination,
                    callee,
                    arguments,
                } => {
                    let callee = self.take_register(callee, instruction.span)?;
                    let Value::BytecodeFunction(callee) = callee else {
                        return Err(BytecodeError::new(
                            format!("{} is not callable", callee.type_name()),
                            instruction.span,
                        ));
                    };
                    if arguments.len() != callee.parameter_count {
                        return Err(BytecodeError::new(
                            format!(
                                "function `{}` expects {} arguments, found {}",
                                callee.name,
                                callee.parameter_count,
                                arguments.len()
                            ),
                            instruction.span,
                        ));
                    }
                    self.ensure_call_capacity(instruction.span)?;
                    let mut call_arguments = callee.bound_arguments.clone();
                    call_arguments.extend(
                        arguments
                            .into_iter()
                            .map(|register| self.take_register(register, instruction.span))
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                    let function = callee.function;
                    let bytecode_function = &self.module.functions[function];
                    if callee.captures.len() != bytecode_function.capture_count {
                        return Err(BytecodeError::new(
                            "closure environment does not match function layout",
                            instruction.span,
                        ));
                    }
                    if call_arguments.len() != bytecode_function.parameter_count {
                        return Err(BytecodeError::new(
                            "bound function arguments do not match function layout",
                            instruction.span,
                        ));
                    }
                    let mut locals = new_local_storage(bytecode_function);
                    let return_type =
                        returns::resolve_return_type(bytecode_function, call_arguments.iter());
                    for (local, capture) in locals.iter_mut().zip(&callee.captures) {
                        *local = capture.clone();
                    }
                    for (local, argument) in locals
                        .iter()
                        .skip(bytecode_function.capture_count)
                        .zip(call_arguments)
                    {
                        local.borrow_mut().initialize(argument);
                    }
                    self.frames.push(Frame {
                        function,
                        registers: vec![None; bytecode_function.register_count],
                        locals,
                        instruction: 0,
                        return_action: ReturnAction::Register(destination),
                        return_type,
                    });
                }
                Instruction::CallImport {
                    destination,
                    import,
                    arguments,
                } => {
                    let arguments = arguments
                        .into_iter()
                        .map(|register| self.take_register(register, instruction.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let declaration = &self.module.imports[import];
                    if let Some(parameters) = &declaration.signature.parameters {
                        for (parameter, argument) in parameters.iter().zip(&arguments) {
                            if !parameter.accepts(argument) {
                                return Err(BytecodeError::new(
                                    format!(
                                        "import `{}` argument expects {}, found {}",
                                        declaration.name,
                                        parameter,
                                        argument.type_name()
                                    ),
                                    instruction.span,
                                ));
                            }
                        }
                    }
                    let arguments = rils_execution::native_arguments::prepare(
                        Some(&declaration.signature),
                        &arguments,
                        |value, spec| self.format_value(value, spec, instruction.span),
                    )?;
                    let value = match declaration.name.as_str() {
                        "std::io::print" | "std::io::println" => {
                            if arguments.is_empty() && declaration.name == "std::io::println" {
                                (self.imports[import])(&arguments).map_err(|message| {
                                    BytecodeError::new(message, instruction.span)
                                })?
                            } else {
                                let Some(format) = arguments.first().and_then(Value::as_string)
                                else {
                                    return Err(BytecodeError::new(
                                        "output function requires a format string",
                                        instruction.span,
                                    ));
                                };
                                let output = self.format_import_arguments(
                                    &format,
                                    &arguments[1..],
                                    instruction.span,
                                )?;
                                (self.imports[import])(&[
                                    rils_execution::value::native_string("{}"),
                                    rils_execution::value::native_string(output),
                                ])
                                .map_err(|message| BytecodeError::new(message, instruction.span))?
                            }
                        }
                        _ => (self.imports[import])(&arguments)
                            .map_err(|message| BytecodeError::new(message, instruction.span))?,
                    };
                    if !declaration.signature.return_type.accepts(&value) {
                        return Err(BytecodeError::new(
                            format!(
                                "import `{}` returned {}, expected {}",
                                declaration.name,
                                value.type_name(),
                                declaration.signature.return_type
                            ),
                            instruction.span,
                        ));
                    }
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::CallNative {
                    destination,
                    import,
                    arguments,
                } => {
                    let arguments = arguments
                        .into_iter()
                        .map(|register| self.take_register(register, instruction.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let symbol = self.module.native_imports[import].symbol.clone();
                    let empty_type = self.module.native_imports[import]
                        .specialized_empty_collection_type()
                        .cloned();
                    let native_empty = empty_type.and_then(|empty_type| {
                        self.native_context.empty_collection(&empty_type).ok()
                    });
                    let value = if let Some(value) = native_empty {
                        value
                    } else if crate::runtime_builtins::requires_owned_native_call(&symbol) {
                        let context = &self.native_context;
                        crate::runtime_builtins::call_native_owned_symbol(
                            &symbol, arguments, context,
                        )
                        .expect("owned native symbol is registered")
                        .map_err(|message| BytecodeError::new(message, instruction.span))?
                    } else {
                        self.call_native_symbol(&symbol, &arguments, instruction.span)?
                    };
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::ConstructRecord {
                    destination,
                    type_id,
                    variant,
                    fields,
                } => {
                    let value =
                        self.construct_record(type_id, variant, fields, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::ConstructTupleVariant {
                    destination,
                    type_id,
                    variant,
                    fields,
                } => {
                    let value =
                        self.construct_tuple_variant(type_id, variant, fields, instruction.span)?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::ConstructUnitVariant {
                    destination,
                    type_id,
                    variant,
                } => {
                    let RuntimeType::Enum(definition) = &self.module.types[type_id] else {
                        return Err(BytecodeError::new(
                            "unit variant requires enum type",
                            instruction.span,
                        ));
                    };
                    self.frame_mut().registers[destination] =
                        Some(Value::Enum(Rc::new(EnumInstance {
                            type_definition: definition.clone(),
                            variant,
                            payload: EnumPayload::Unit,
                            type_arguments: Vec::new(),
                        })));
                }
                operation @ (Instruction::BuildTuple { .. }
                | Instruction::BuildArray { .. }
                | Instruction::BuildRepeatArray { .. }
                | Instruction::BuildRange { .. }
                | Instruction::BuildOptionNone { .. }
                | Instruction::BuildOptionSome { .. }
                | Instruction::BuildResultOk { .. }
                | Instruction::BuildResultErr { .. }
                | Instruction::ApplyStorage { .. }) => {
                    self.execute_storage(operation, instruction.span)?
                }
                Instruction::TryResult {
                    destination,
                    source,
                } => {
                    let result = self.take_register(source, instruction.span)?;
                    let (structs, enums) = self.type_definitions();
                    let result = crate::value::owned_sum::materialize(result, &structs, &enums)
                        .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    match result {
                        Value::Result {
                            value: Ok(value), ..
                        } => {
                            let value = Rc::try_unwrap(value)
                                .or_else(|value| value.clone_owned())
                                .map_err(|message| BytecodeError::new(message, instruction.span))?;
                            self.frame_mut().registers[destination] = Some(value);
                        }
                        Value::Result {
                            value: Err(error),
                            error_type,
                            ..
                        } => {
                            let result = Value::Result {
                                value: Err(error),
                                ok_type: None,
                                error_type,
                            };
                            if let Some(value) = self.finish_return(result, instruction.span)? {
                                return Ok(value);
                            }
                        }
                        value => {
                            return Err(BytecodeError::new(
                                format!(
                                    "the `?` operator requires Result, found {}",
                                    value.type_name()
                                ),
                                instruction.span,
                            ));
                        }
                    }
                }
                Instruction::MatchPattern {
                    destination,
                    source,
                    pattern,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let value = crate::value::owned_sum::materialize(
                        value,
                        &self.native_context.structs,
                        &self.native_context.enums,
                    )
                    .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    self.frame_mut().registers[source] = Some(value);
                    let matched = self.frame().registers[source]
                        .as_ref()
                        .is_some_and(|value| pattern_matches(&pattern, value));
                    self.frame_mut().registers[destination] = Some(Value::Bool(matched));
                }
                Instruction::BindPattern { source, pattern } => {
                    let value = self.frame().registers[source]
                        .as_ref()
                        .ok_or_else(|| {
                            BytecodeError::new("match value register is empty", instruction.span)
                        })?
                        .clone();
                    let mut bindings = Vec::new();
                    collect_pattern_bindings(&pattern, &value, &mut bindings);
                    for (local, value) in bindings {
                        self.frame().locals[local].borrow_mut().initialize(value);
                    }
                }
                Instruction::Jump { target } => self.frame_mut().instruction = target,
                Instruction::Branch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    let value = self.frame().registers[condition].as_ref().ok_or_else(|| {
                        BytecodeError::new("branch condition register is empty", instruction.span)
                    })?;
                    self.frame_mut().instruction = if condition_value(value, instruction.span)? {
                        then_target
                    } else {
                        else_target
                    };
                }
                Instruction::IteratorNext {
                    iterator,
                    destination,
                    some_target,
                    none_target,
                } => {
                    let script_iterator = match self.frame().registers[iterator].as_ref() {
                        Some(Value::BytecodeIterator(iterator)) => Some(iterator.clone()),
                        _ => None,
                    };
                    if let Some(iterator) = script_iterator {
                        let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(
                            iterator.storage.clone(),
                            true,
                        )));
                        self.push_script_call(
                            iterator.next_function,
                            vec![reference],
                            ReturnAction::IteratorNext {
                                destination,
                                some_target,
                                none_target,
                            },
                            instruction.span,
                        )?;
                        continue;
                    }
                    let item = {
                        let iterator =
                            self.frame_mut().registers[iterator]
                                .as_mut()
                                .ok_or_else(|| {
                                    BytecodeError::new(
                                        "iterator register is empty",
                                        instruction.span,
                                    )
                                })?;
                        rils_execution::iteration::next_builtin(iterator)
                            .ok_or_else(|| {
                                BytecodeError::new(
                                    format!("{} is not an iterator", iterator.type_name()),
                                    instruction.span,
                                )
                            })?
                            .map_err(|message| BytecodeError::new(message, instruction.span))?
                    };
                    if let Some(item) = item {
                        self.frame_mut().registers[destination] = Some(item);
                        self.frame_mut().instruction = some_target;
                    } else {
                        self.frame_mut().instruction = none_target;
                    }
                }
                Instruction::Return { source } => {
                    let value = self.take_register(source, instruction.span)?;
                    if let Some(value) = self.finish_return(value, instruction.span)? {
                        return Ok(value);
                    }
                }
                Instruction::MatchFail => {
                    return Err(BytecodeError::new(
                        "non-exhaustive match reached at runtime",
                        instruction.span,
                    ));
                }
            }
        }
    }

    fn take_register(&mut self, register: usize, span: Span) -> Result<Value, BytecodeError> {
        self.frame_mut().registers[register]
            .take()
            .ok_or_else(|| BytecodeError::new("read from an empty register", span))
    }

    fn take_registers(
        &mut self,
        registers: Vec<usize>,
        span: Span,
    ) -> Result<Vec<Value>, BytecodeError> {
        registers
            .into_iter()
            .map(|register| self.take_register(register, span))
            .collect()
    }

    fn frame(&self) -> &Frame {
        self.frames.last().expect("VM always has an active frame")
    }

    fn frame_mut(&mut self) -> &mut Frame {
        self.frames
            .last_mut()
            .expect("VM always has an active frame")
    }

    fn current_function(&self) -> &BytecodeFunction {
        &self.module.functions[self.frame().function]
    }
}
