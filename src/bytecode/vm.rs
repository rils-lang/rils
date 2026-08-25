use super::*;

pub(super) struct Frame {
    function: usize,
    registers: Vec<Option<Value>>,
    locals: Vec<StorageRef>,
    instruction: usize,
    return_action: ReturnAction,
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

struct ResolvedPlace {
    local: usize,
    projections: Vec<ResolvedProjection>,
}

enum ResolvedProjection {
    Field(String),
    Index(usize),
}

enum PlaceContainer {
    Struct(Rc<StructInstance>),
    Sequence(Rc<SequenceValue>),
}

pub(super) struct VirtualMachine<'a> {
    pub(super) module: &'a BytecodeModule,
    pub(super) imports: Vec<Rc<BytecodeHostHandler>>,
    pub(super) host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
    pub(super) frames: Vec<Frame>,
    pub(super) steps: usize,
    pub(super) max_steps: usize,
    pub(super) max_call_depth: usize,
    root_is_module_entry: bool,
}

impl<'a> VirtualMachine<'a> {
    pub(super) fn new(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        limits: crate::ExecutionLimits,
    ) -> Self {
        let entry = &module.functions[module.entry];
        Self {
            module,
            imports,
            host_value_formatter,
            frames: vec![Frame {
                function: module.entry,
                registers: vec![None; entry.register_count],
                locals: new_local_storage(entry),
                instruction: 0,
                return_action: ReturnAction::Complete,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: true,
        }
    }

    pub(super) fn new_call(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        limits: crate::ExecutionLimits,
        function: usize,
        arguments: Vec<Value>,
    ) -> Result<Self, BytecodeError> {
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
        for (local, argument) in locals.iter().zip(arguments) {
            local.borrow_mut().initialize(argument);
        }
        Ok(Self {
            module,
            imports,
            host_value_formatter,
            frames: vec![Frame {
                function,
                registers: vec![None; callee.register_count],
                locals,
                instruction: 0,
                return_action: ReturnAction::Complete,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: false,
        })
    }

    pub(super) fn execute(mut self) -> Result<Value, BytecodeError> {
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
                Instruction::InitLocal { local, source } => {
                    let value = self.take_register(source, instruction.span)?;
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
                    let iterator = match source {
                        Value::Range(range) => Value::Range(range),
                        Value::SequenceIterator(iterator) => Value::SequenceIterator(iterator),
                        Value::Array(sequence) | Value::Vec(sequence) => {
                            let element_type = sequence
                                .element_type
                                .borrow()
                                .clone()
                                .unwrap_or(Type::Unknown);
                            let items = sequence
                                .elements
                                .borrow_mut()
                                .iter_mut()
                                .map(|slot| {
                                    slot.value.take().ok_or_else(|| {
                                        BytecodeError::new(
                                            "cannot iterate a partially moved collection",
                                            instruction.span,
                                        )
                                    })
                                })
                                .collect::<Result<VecDeque<_>, _>>()?;
                            Value::SequenceIterator(Rc::new(SequenceIteratorValue {
                                items: RefCell::new(items),
                                element_type,
                            }))
                        }
                        Value::HashMap(map) => crate::hash_collections::call(
                            "core::hash_map::into_iter",
                            &[Value::HashMap(map)],
                        )
                        .map_err(|message| BytecodeError::new(message, instruction.span))?,
                        Value::HashSet(set) => crate::hash_collections::call(
                            "core::hash_set::into_iter",
                            &[Value::HashSet(set)],
                        )
                        .map_err(|message| BytecodeError::new(message, instruction.span))?,
                        value => {
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
                    for (local, argument) in locals.iter().zip(arguments) {
                        local.borrow_mut().initialize(argument);
                    }
                    self.frames.push(Frame {
                        function,
                        registers: vec![None; callee.register_count],
                        locals,
                        instruction: 0,
                        return_action: ReturnAction::Register(destination),
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
                    let value = match declaration.name.as_str() {
                        "core::fmt::write_str" => {
                            let buffer = crate::formatting::buffer_from_value(&arguments[0])
                                .map_err(|message| BytecodeError::new(message, instruction.span))?;
                            let Value::String(value) = &arguments[1] else {
                                return Err(BytecodeError::new(
                                    "Formatter::write_str expects string",
                                    instruction.span,
                                ));
                            };
                            buffer.write_str(value);
                            super::formatting::format_ok()
                        }
                        "core::fmt::write_derived_debug" => self.write_derived_debug_import(
                            &arguments[0],
                            &arguments[1],
                            instruction.span,
                        )?,
                        "std::io::print" | "std::io::println" => {
                            if arguments.is_empty() && declaration.name == "std::io::println" {
                                (self.imports[import])(&arguments).map_err(|message| {
                                    BytecodeError::new(message, instruction.span)
                                })?
                            } else {
                                let Some(Value::String(format)) = arguments.first() else {
                                    return Err(BytecodeError::new(
                                        "output function requires a format string",
                                        instruction.span,
                                    ));
                                };
                                let output = self.format_import_arguments(
                                    format,
                                    &arguments[1..],
                                    instruction.span,
                                )?;
                                (self.imports[import])(&[
                                    Value::String("{}".into()),
                                    Value::String(output.into()),
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
                Instruction::CallIntrinsic {
                    destination,
                    intrinsic,
                    target,
                    arguments,
                } => {
                    let arguments = arguments
                        .into_iter()
                        .map(|register| self.take_register(register, instruction.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let value = crate::numeric::execute_intrinsic(intrinsic, target, &arguments)
                        .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::ConstructRecord {
                    destination,
                    type_id,
                    variant,
                    fields,
                } => {
                    let values = fields
                        .into_iter()
                        .map(|(name, register)| {
                            Ok((name, self.take_register(register, instruction.span)?))
                        })
                        .collect::<Result<HashMap<_, _>, BytecodeError>>()?;
                    let value = match (&self.module.types[type_id], variant) {
                        (RuntimeType::Struct(definition), None) => {
                            let slots = definition
                                .fields
                                .iter()
                                .map(|field| {
                                    let value = values
                                        .get(&field.name)
                                        .cloned()
                                        .expect("static analysis checked fields");
                                    let annotation =
                                        if matches!(field.type_annotation, Type::Variable(_)) {
                                            Type::of_value(&value).unwrap_or(Type::Unknown)
                                        } else {
                                            field.type_annotation.clone()
                                        };
                                    (
                                        field.name.clone(),
                                        FieldSlot {
                                            value: Some(value),
                                            type_annotation: annotation,
                                            references: 0,
                                        },
                                    )
                                })
                                .collect();
                            Value::Struct(Rc::new(StructInstance {
                                type_definition: definition.clone(),
                                fields: RefCell::new(slots),
                                type_arguments: Vec::new(),
                            }))
                        }
                        (RuntimeType::Enum(definition), Some(variant)) => {
                            Value::Enum(Rc::new(EnumInstance {
                                type_definition: definition.clone(),
                                variant,
                                payload: EnumPayload::Record(values),
                                type_arguments: Vec::new(),
                            }))
                        }
                        _ => {
                            return Err(BytecodeError::new(
                                "record constructor does not match its type",
                                instruction.span,
                            ));
                        }
                    };
                    self.frame_mut().registers[destination] = Some(value);
                }
                Instruction::ConstructTupleVariant {
                    destination,
                    type_id,
                    variant,
                    fields,
                } => {
                    let values = self.take_registers(fields, instruction.span)?;
                    let RuntimeType::Enum(definition) = &self.module.types[type_id] else {
                        return Err(BytecodeError::new(
                            "tuple variant requires enum type",
                            instruction.span,
                        ));
                    };
                    self.frame_mut().registers[destination] =
                        Some(Value::Enum(Rc::new(EnumInstance {
                            type_definition: definition.clone(),
                            variant,
                            payload: EnumPayload::Tuple(values),
                            type_arguments: Vec::new(),
                        })));
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
                Instruction::BuildTuple {
                    destination,
                    elements,
                } => {
                    let values = self.take_registers(elements, instruction.span)?;
                    self.frame_mut().registers[destination] =
                        Some(sequence_value(values, false, instruction.span)?);
                }
                Instruction::BuildArray {
                    destination,
                    elements,
                } => {
                    let values = self.take_registers(elements, instruction.span)?;
                    self.frame_mut().registers[destination] =
                        Some(sequence_value(values, true, instruction.span)?);
                }
                Instruction::BuildRepeatArray {
                    destination,
                    value,
                    count,
                } => {
                    let value = self.take_register(value, instruction.span)?;
                    let count = self.take_register(count, instruction.span)?;
                    let Value::Usize(count) = count else {
                        return Err(BytecodeError::new(
                            "array repeat count must be usize",
                            instruction.span,
                        ));
                    };
                    if !value.is_copy() {
                        return Err(BytecodeError::new(
                            "array repeat syntax requires a Copy value",
                            instruction.span,
                        ));
                    }
                    let values = (0..count)
                        .map(|_| {
                            value
                                .clone_owned()
                                .map_err(|message| BytecodeError::new(message, instruction.span))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.frame_mut().registers[destination] =
                        Some(sequence_value(values, true, instruction.span)?);
                }
                Instruction::BuildRange {
                    destination,
                    start,
                    end,
                } => {
                    let start = self.take_register(start, instruction.span)?;
                    let end = self.take_register(end, instruction.span)?;
                    let range = RangeValue::new(start, end)
                        .map_err(|message| BytecodeError::new(message, instruction.span))?;
                    self.frame_mut().registers[destination] = Some(Value::Range(range));
                }
                Instruction::BuildOptionNone { destination } => {
                    self.frame_mut().registers[destination] = Some(Value::Option {
                        value: None,
                        element_type: None,
                    });
                }
                Instruction::BuildOptionSome {
                    destination,
                    source,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let element_type = Type::of_value(&value);
                    self.frame_mut().registers[destination] = Some(Value::Option {
                        value: Some(Rc::new(value)),
                        element_type,
                    });
                }
                Instruction::BuildResultOk {
                    destination,
                    source,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let ok_type = Type::of_value(&value);
                    self.frame_mut().registers[destination] = Some(Value::Result {
                        value: Ok(Rc::new(value)),
                        ok_type,
                        error_type: None,
                    });
                }
                Instruction::BuildResultErr {
                    destination,
                    source,
                } => {
                    let value = self.take_register(source, instruction.span)?;
                    let error_type = Type::of_value(&value);
                    self.frame_mut().registers[destination] = Some(Value::Result {
                        value: Err(Rc::new(value)),
                        ok_type: None,
                        error_type,
                    });
                }
                Instruction::TryResult {
                    destination,
                    source,
                } => {
                    let result = self.take_register(source, instruction.span)?;
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
                        match iterator {
                            Value::Range(range) => range
                                .next()
                                .map_err(|message| BytecodeError::new(message, instruction.span))?,
                            Value::SequenceIterator(iterator) => {
                                iterator.items.borrow_mut().pop_front()
                            }
                            value => {
                                return Err(BytecodeError::new(
                                    format!("{} is not an iterator", value.type_name()),
                                    instruction.span,
                                ));
                            }
                        }
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

    fn iterator_methods(&self, value: &Value) -> Option<BytecodeIteratorMethods> {
        let name = match value {
            Value::Struct(instance) => &instance.type_definition.name,
            Value::Enum(instance) => &instance.type_definition.name,
            _ => return None,
        };
        self.module.iterators.get(name).cloned()
    }

    fn script_iterator(&self, value: Value, span: Span) -> Result<Value, BytecodeError> {
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

    fn push_script_call(
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

    fn ensure_call_capacity(&self, span: Span) -> Result<(), BytecodeError> {
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

    fn resolve_place(
        &mut self,
        place: BytecodePlace,
        span: Span,
    ) -> Result<ResolvedPlace, BytecodeError> {
        let mut projections = Vec::with_capacity(place.projections.len());
        for projection in place.projections {
            projections.push(match projection {
                BytecodeProjection::Field(field) => ResolvedProjection::Field(field),
                BytecodeProjection::Index(register) => {
                    let value = self.take_register(register, span)?;
                    let Value::Usize(index) = value else {
                        return Err(BytecodeError::new("collection index must be usize", span));
                    };
                    ResolvedProjection::Index(index)
                }
            });
        }
        Ok(ResolvedPlace {
            local: place.local,
            projections,
        })
    }

    fn place_root(&self, local: usize, span: Span) -> Result<PlaceContainer, BytecodeError> {
        let value = self.frame().locals[local]
            .borrow()
            .read()
            .map_err(|error| access_error(error, span))?;
        self.place_container(value, span)
    }

    fn place_is_mutable(&self, local: usize, span: Span) -> Result<bool, BytecodeError> {
        let value = self.frame().locals[local]
            .borrow()
            .read()
            .map_err(|error| access_error(error, span))?;
        match value {
            Value::Reference(reference) => Ok(reference.mutable),
            _ => Ok(self.current_function().local_mutability[local]),
        }
    }

    fn place_container(&self, value: Value, span: Span) -> Result<PlaceContainer, BytecodeError> {
        match value {
            Value::Struct(instance) => Ok(PlaceContainer::Struct(instance)),
            Value::Tuple(sequence) | Value::Array(sequence) | Value::Vec(sequence) => {
                Ok(PlaceContainer::Sequence(sequence))
            }
            Value::Reference(reference) => self.place_container(
                reference
                    .read()
                    .map_err(|message| BytecodeError::new(message, span))?,
                span,
            ),
            value => Err(BytecodeError::new(
                format!("cannot project into {}", value.type_name()),
                span,
            )),
        }
    }

    fn projected_value(
        &self,
        container: &PlaceContainer,
        projection: &ResolvedProjection,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                let fields = instance.fields.borrow();
                let slot = fields
                    .get(field)
                    .ok_or_else(|| BytecodeError::new(format!("unknown field `{field}`"), span))?;
                slot.value.clone().ok_or_else(|| {
                    BytecodeError::new(format!("field `{field}` has been moved"), span)
                })
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                let elements = sequence.elements.borrow();
                let slot = elements.get(*index).ok_or_else(|| {
                    BytecodeError::new(format!("index {index} is out of bounds"), span)
                })?;
                slot.value.clone().ok_or_else(|| {
                    BytecodeError::new(format!("element at index {index} has been moved"), span)
                })
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    fn place_parent<'p>(
        &self,
        place: &'p ResolvedPlace,
        span: Span,
    ) -> Result<(PlaceContainer, &'p ResolvedProjection), BytecodeError> {
        let (last, parents) = place
            .projections
            .split_last()
            .ok_or_else(|| BytecodeError::new("place projection cannot be empty", span))?;
        let mut container = self.place_root(place.local, span)?;
        for projection in parents {
            let value = self.projected_value(&container, projection, span)?;
            container = self.place_container(value, span)?;
        }
        Ok((container, last))
    }

    fn take_place(&self, place: &ResolvedPlace, span: Span) -> Result<Value, BytecodeError> {
        let (container, projection) = self.place_parent(place, span)?;
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                take_field_slot(instance.fields.borrow_mut().get_mut(field), field, span)
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                if *index >= sequence.elements.borrow().len() {
                    return Err(BytecodeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                take_field_slot(
                    sequence.elements.borrow_mut().get_mut(*index),
                    &format!("index {index}"),
                    span,
                )
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    fn store_place(
        &self,
        place: &ResolvedPlace,
        value: Value,
        span: Span,
    ) -> Result<(), BytecodeError> {
        let (container, projection) = self.place_parent(place, span)?;
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                store_field_slot(
                    instance.fields.borrow_mut().get_mut(field),
                    field,
                    value,
                    span,
                )
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                if *index >= sequence.elements.borrow().len() {
                    return Err(BytecodeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                store_field_slot(
                    sequence.elements.borrow_mut().get_mut(*index),
                    &format!("index {index}"),
                    value,
                    span,
                )
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    fn place_reference(
        &self,
        place: &ResolvedPlace,
        mutable: bool,
        span: Span,
    ) -> Result<ReferenceValue, BytecodeError> {
        let mut container = self.place_root(place.local, span)?;
        let mut guard = None;
        for (index, projection) in place.projections.iter().enumerate() {
            let reference = match (&container, projection) {
                (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                    ReferenceValue::new_guarded_struct_field(
                        instance.clone(),
                        field.clone(),
                        mutable,
                        guard,
                    )
                }
                (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(element)) => {
                    ReferenceValue::new_guarded_sequence_element(
                        sequence.clone(),
                        *element,
                        mutable,
                        guard,
                    )
                }
                _ => {
                    return Err(BytecodeError::new(
                        "place projection does not match its value",
                        span,
                    ));
                }
            }
            .map_err(|message| BytecodeError::new(message, span))?;
            if index + 1 == place.projections.len() {
                return Ok(reference);
            }
            let reference = Rc::new(reference);
            let value = reference
                .read()
                .map_err(|message| BytecodeError::new(message, span))?;
            container = self.place_container(value, span)?;
            guard = Some(reference);
        }
        unreachable!("empty place projections are rejected")
    }

    fn finish_return(&mut self, value: Value, span: Span) -> Result<Option<Value>, BytecodeError> {
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
