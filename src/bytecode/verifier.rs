use super::*;

impl BytecodeModule {
    pub(super) fn verify(&self) -> Result<(), BytecodeError> {
        let mut source_ids = HashSet::new();
        let mut source_names = HashSet::new();
        for source in &self.sources {
            if source.id == SourceId::UNKNOWN
                || source.name.is_empty()
                || !source_ids.insert(source.id)
                || !source_names.insert(source.name.as_str())
            {
                return Err(BytecodeError::new(
                    "bytecode module has an invalid source table",
                    Span::default(),
                ));
            }
        }
        if self
            .types
            .iter()
            .any(|runtime_type| !self.valid_runtime_type(runtime_type))
            || self
                .imports
                .iter()
                .any(|import| !self.valid_signature(&import.signature))
        {
            return Err(BytecodeError::new(
                "bytecode module type metadata references an unknown source",
                Span::default(),
            ));
        }
        if self.functions.is_empty() || self.entry >= self.functions.len() {
            return Err(BytecodeError::new(
                "bytecode module has no valid entry function",
                Span::default(),
            ));
        }
        for function in &self.functions {
            self.verify_function(function)?;
        }
        for methods in self.iterators.values() {
            if methods
                .into_iter
                .into_iter()
                .chain(methods.next)
                .any(|function| function >= self.functions.len())
            {
                return Err(BytecodeError::new(
                    "iterator method table contains an invalid function index",
                    Span::default(),
                ));
            }
            if methods.into_iter.is_some_and(|function| {
                let function = &self.functions[function];
                function.capture_count != 0 || function.parameter_count != 1
            }) || methods.next.is_some_and(|function| {
                let function = &self.functions[function];
                function.capture_count != 0 || function.parameter_count != 1
            }) {
                return Err(BytecodeError::new(
                    "iterator method table contains an invalid method layout",
                    Span::default(),
                ));
            }
        }
        Ok(())
    }

    fn verify_function(&self, function: &BytecodeFunction) -> Result<(), BytecodeError> {
        if !self.valid_span(function.span)
            || function
                .instructions
                .iter()
                .any(|instruction| !self.valid_span(instruction.span))
        {
            return Err(BytecodeError::new(
                format!("function `{}` references an unknown source", function.name),
                function.span,
            ));
        }
        if function.instructions.is_empty()
            || function.parameter_count + function.capture_count > function.local_count
            || function.local_mutability.len() != function.local_count
        {
            return Err(BytecodeError::new(
                format!("function `{}` has an invalid frame layout", function.name),
                function.span,
            ));
        }
        let mut has_return = false;
        for instruction in &function.instructions {
            let invalid_register = |register: usize| register >= function.register_count;
            let invalid_place = |place: &BytecodePlace| {
                place.local >= function.local_count
                    || place.projections.is_empty()
                    || place.projections.iter().any(|projection| {
                        matches!(projection, BytecodeProjection::Index(register) if invalid_register(*register))
                    })
            };
            match &instruction.instruction {
                Instruction::LoadConstant {
                    destination,
                    constant,
                } => {
                    if invalid_register(*destination) || *constant >= function.constants.len() {
                        return Err(BytecodeError::new(
                            "invalid constant load operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::LoadFunction {
                    destination,
                    function: callee,
                } => {
                    if invalid_register(*destination) || *callee >= self.functions.len() {
                        return Err(BytecodeError::new(
                            "invalid function reference operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BindMethod {
                    destination,
                    function: callee,
                    receiver,
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*receiver)
                        || *callee >= self.functions.len()
                        || self.functions[*callee].parameter_count == 0
                    {
                        return Err(BytecodeError::new(
                            "invalid bound method operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BorrowTemporary {
                    destination,
                    source,
                    ..
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid temporary borrow operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Reborrow {
                    destination,
                    source,
                    ..
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid reborrow operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::CreateClosure {
                    destination,
                    function: callee,
                    captures,
                } => {
                    if invalid_register(*destination)
                        || *callee >= self.functions.len()
                        || captures.iter().any(|local| *local >= function.local_count)
                        || captures.len() != self.functions[*callee].capture_count
                    {
                        return Err(BytecodeError::new(
                            "invalid closure operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::TakeLocal { destination, local } => {
                    if invalid_register(*destination) || *local >= function.local_count {
                        return Err(BytecodeError::new(
                            "invalid local load operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::TakePlace { destination, place } => {
                    if invalid_register(*destination) || invalid_place(place) {
                        return Err(BytecodeError::new("invalid place read", instruction.span));
                    }
                }
                Instruction::StoreLocal { local, source } => {
                    if *local >= function.local_count || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid local store operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::InitLocal { local, source } => {
                    if *local >= function.local_count || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid local initialization operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::DropLocal { local } => {
                    if *local >= function.local_count {
                        return Err(BytecodeError::new("invalid local drop", instruction.span));
                    }
                }
                Instruction::BorrowLocal {
                    destination, local, ..
                } => {
                    if invalid_register(*destination) || *local >= function.local_count {
                        return Err(BytecodeError::new("invalid local borrow", instruction.span));
                    }
                }
                Instruction::BorrowPlace {
                    destination, place, ..
                } => {
                    if invalid_register(*destination) || invalid_place(place) {
                        return Err(BytecodeError::new("invalid place borrow", instruction.span));
                    }
                }
                Instruction::Dereference {
                    destination,
                    source,
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new("invalid dereference", instruction.span));
                    }
                }
                Instruction::StoreDereference { reference, source } => {
                    if invalid_register(*reference) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid dereference store",
                            instruction.span,
                        ));
                    }
                }
                Instruction::StorePlace { place, source } => {
                    if invalid_register(*source) || invalid_place(place) {
                        return Err(BytecodeError::new("invalid place store", instruction.span));
                    }
                }
                Instruction::IntoIterator {
                    destination,
                    source,
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid iterator construction operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Move {
                    destination,
                    source,
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid register move operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Unary {
                    destination,
                    operand,
                    ..
                } => {
                    if invalid_register(*destination) || invalid_register(*operand) {
                        return Err(BytecodeError::new(
                            "invalid unary operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Cast {
                    destination,
                    source,
                    ..
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid integer cast operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Binary {
                    destination,
                    left,
                    right,
                    ..
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*left)
                        || invalid_register(*right)
                    {
                        return Err(BytecodeError::new(
                            "invalid binary operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Call {
                    destination,
                    function: callee,
                    arguments,
                } => {
                    if invalid_register(*destination)
                        || *callee >= self.functions.len()
                        || arguments.iter().any(|register| invalid_register(*register))
                        || arguments.len() != self.functions[*callee].parameter_count
                        || self.functions[*callee].capture_count != 0
                    {
                        return Err(BytecodeError::new(
                            "invalid function call operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::CallValue {
                    destination,
                    callee,
                    arguments,
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*callee)
                        || arguments.iter().any(|register| invalid_register(*register))
                    {
                        return Err(BytecodeError::new(
                            "invalid indirect call operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::CallImport {
                    destination,
                    import,
                    arguments,
                } => {
                    if invalid_register(*destination)
                        || *import >= self.imports.len()
                        || arguments.iter().any(|register| invalid_register(*register))
                        || self.imports[*import]
                            .signature
                            .parameters
                            .as_ref()
                            .is_some_and(|parameters| parameters.len() != arguments.len())
                    {
                        return Err(BytecodeError::new(
                            "invalid import call operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::CallIntrinsic {
                    destination,
                    intrinsic,
                    target,
                    arguments,
                } => {
                    let declaration = rils_builtins::intrinsic(*intrinsic);
                    if invalid_register(*destination)
                        || arguments.iter().any(|register| invalid_register(*register))
                        || declaration.is_none()
                        || declaration.is_some_and(|item| {
                            arguments.len()
                                != item.signature.parameters.len()
                                    + usize::from(item.kind == rils_builtins::IntrinsicKind::Method)
                        })
                        || (target.is_some()
                            != declaration.is_some_and(|item| {
                                item.kind == rils_builtins::IntrinsicKind::AssociatedFunction
                            }))
                    {
                        return Err(BytecodeError::new(
                            "invalid intrinsic call operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::ConstructRecord {
                    destination,
                    type_id,
                    fields,
                    ..
                } => {
                    if invalid_register(*destination)
                        || *type_id >= self.types.len()
                        || fields
                            .iter()
                            .any(|(_, register)| invalid_register(*register))
                    {
                        return Err(BytecodeError::new(
                            "invalid record construction",
                            instruction.span,
                        ));
                    }
                }
                Instruction::ConstructTupleVariant {
                    destination,
                    type_id,
                    fields,
                    ..
                } => {
                    if invalid_register(*destination)
                        || *type_id >= self.types.len()
                        || fields.iter().any(|register| invalid_register(*register))
                    {
                        return Err(BytecodeError::new(
                            "invalid tuple variant construction",
                            instruction.span,
                        ));
                    }
                }
                Instruction::ConstructUnitVariant {
                    destination,
                    type_id,
                    ..
                } => {
                    if invalid_register(*destination) || *type_id >= self.types.len() {
                        return Err(BytecodeError::new(
                            "invalid unit variant construction",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BuildTuple {
                    destination,
                    elements,
                }
                | Instruction::BuildArray {
                    destination,
                    elements,
                } => {
                    if invalid_register(*destination)
                        || elements.iter().any(|register| invalid_register(*register))
                    {
                        return Err(BytecodeError::new(
                            "invalid sequence construction operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BuildRepeatArray {
                    destination,
                    value,
                    count,
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*value)
                        || invalid_register(*count)
                    {
                        return Err(BytecodeError::new(
                            "invalid repeated array operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BuildRange {
                    destination,
                    start,
                    end,
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*start)
                        || invalid_register(*end)
                    {
                        return Err(BytecodeError::new(
                            "invalid range operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BuildOptionNone { destination } => {
                    if invalid_register(*destination) {
                        return Err(BytecodeError::new(
                            "invalid None construction operand",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BuildOptionSome {
                    destination,
                    source,
                }
                | Instruction::BuildResultOk {
                    destination,
                    source,
                }
                | Instruction::BuildResultErr {
                    destination,
                    source,
                }
                | Instruction::TryResult {
                    destination,
                    source,
                } => {
                    if invalid_register(*destination) || invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid algebraic value operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::MatchPattern {
                    destination,
                    source,
                    pattern,
                } => {
                    if invalid_register(*destination)
                        || invalid_register(*source)
                        || !pattern_locals_valid(pattern, function.local_count)
                    {
                        return Err(BytecodeError::new(
                            "invalid pattern match operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::BindPattern { source, pattern } => {
                    if invalid_register(*source)
                        || !pattern_locals_valid(pattern, function.local_count)
                    {
                        return Err(BytecodeError::new(
                            "invalid pattern binding operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Jump { target } => {
                    if *target >= function.instructions.len() {
                        return Err(BytecodeError::new(
                            "jump target is outside the instruction stream",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Branch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    if invalid_register(*condition)
                        || *then_target >= function.instructions.len()
                        || *else_target >= function.instructions.len()
                    {
                        return Err(BytecodeError::new(
                            "invalid branch operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::IteratorNext {
                    iterator,
                    destination,
                    some_target,
                    none_target,
                } => {
                    if invalid_register(*iterator)
                        || invalid_register(*destination)
                        || *some_target >= function.instructions.len()
                        || *none_target >= function.instructions.len()
                    {
                        return Err(BytecodeError::new(
                            "invalid iterator next operands",
                            instruction.span,
                        ));
                    }
                }
                Instruction::Return { source } => {
                    if invalid_register(*source) {
                        return Err(BytecodeError::new(
                            "invalid return register",
                            instruction.span,
                        ));
                    }
                    has_return = true;
                }
                Instruction::MatchFail => {}
            }
        }
        if !has_return {
            return Err(BytecodeError::new(
                "bytecode module has no return instruction",
                Span::default(),
            ));
        }
        Ok(())
    }

    fn valid_span(&self, span: Span) -> bool {
        span.source == SourceId::UNKNOWN
            || self.sources.iter().any(|source| source.id == span.source)
    }

    fn valid_signature(&self, signature: &FunctionSignature) -> bool {
        signature
            .parameters
            .as_ref()
            .is_none_or(|parameters| parameters.iter().all(|ty| self.valid_type(ty)))
            && self.valid_type(&signature.return_type)
    }

    fn valid_runtime_type(&self, runtime_type: &RuntimeType) -> bool {
        let valid_parameter =
            |parameter: &crate::ast::GenericParameter| self.valid_span(parameter.span);
        let valid_field = |field: &crate::ast::NamedField| {
            self.valid_span(field.span) && self.valid_type(&field.type_annotation)
        };
        match runtime_type {
            RuntimeType::Struct(value) => {
                value.generic_parameters.iter().all(valid_parameter)
                    && value.fields.iter().all(valid_field)
            }
            RuntimeType::Enum(value) => {
                value.generic_parameters.iter().all(valid_parameter)
                    && value.variants.iter().all(|variant| match variant {
                        crate::ast::EnumVariant::Unit { span, .. } => self.valid_span(*span),
                        crate::ast::EnumVariant::Tuple { fields, span, .. } => {
                            self.valid_span(*span)
                                && fields.iter().all(|field| self.valid_type(field))
                        }
                        crate::ast::EnumVariant::Record { fields, span, .. } => {
                            self.valid_span(*span) && fields.iter().all(valid_field)
                        }
                    })
            }
        }
    }

    fn valid_type(&self, ty: &Type) -> bool {
        match ty {
            Type::IntegerVariable(span) | Type::FloatVariable(span) => self.valid_span(*span),
            Type::Tuple(elements) => elements.iter().all(|element| self.valid_type(element)),
            Type::Array { element, .. }
            | Type::Reference { inner: element, .. }
            | Type::Option(element) => self.valid_type(element),
            Type::Function {
                parameters,
                return_type,
            } => {
                parameters
                    .as_ref()
                    .is_none_or(|parameters| parameters.iter().all(|ty| self.valid_type(ty)))
                    && self.valid_type(return_type)
            }
            Type::Result(ok, error) => self.valid_type(ok) && self.valid_type(error),
            Type::Named { arguments, .. } => {
                arguments.iter().all(|argument| self.valid_type(argument))
            }
            Type::Associated {
                base, arguments, ..
            } => {
                self.valid_type(base) && arguments.iter().all(|argument| self.valid_type(argument))
            }
            _ => true,
        }
    }
}
