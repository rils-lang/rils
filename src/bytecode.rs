use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    fmt,
    rc::Rc,
};

use crate::{
    ast::{BinaryOp, UnaryOp},
    environment::{AccessError, AssignError, StorageRef, StorageSlot},
    hir::{HirLiteral, HirPattern, HirTypeDefinition},
    mir::{MirFunction, MirInstruction, MirProgram, MirTerminator},
    source::{Span, format_diagnostic},
    types::{FunctionSignature, IntegerType, Type},
    value::{
        BytecodeFunctionValue, BytecodeIteratorValue, EnumInstance, EnumPayload, EnumType,
        FieldSlot, RangeValue, ReferenceValue, SequenceIteratorValue, SequenceValue,
        StructInstance, StructType, Value,
    },
};

mod encoder;
mod format;
mod host;
mod verifier;
mod vm;

use encoder::encode;
pub use format::{BYTECODE_FORMAT_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeFormatError};
pub use host::{BYTECODE_HOST_ABI_VERSION, BytecodeHost, BytecodeHostHandler, BytecodeImport};
use vm::VirtualMachine;

pub use rils_compiler::{
    CompileError, HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM,
    HOST_MANIFEST_FORMAT_VERSION, HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION,
    HOST_MANIFEST_JSON_MAX_BYTES, HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES,
    HOST_MANIFEST_MAX_FUNCTIONS, HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS,
    HostCallKind, HostContract, HostFunctionDeclaration, HostModuleDeclaration, HostThreadAffinity,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeError {
    pub message: String,
    pub span: Span,
}

impl BytecodeError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        format_diagnostic(
            source_name,
            source,
            self.span,
            &format!("bytecode error: {}", self.message),
        )
    }
}

impl fmt::Display for BytecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bytecode error: {}", self.message)
    }
}

impl Error for BytecodeError {}

#[derive(Clone)]
pub struct BytecodeModule {
    functions: Vec<BytecodeFunction>,
    types: Vec<RuntimeType>,
    imports: Vec<BytecodeImport>,
    iterators: HashMap<String, BytecodeIteratorMethods>,
    entry: usize,
}

#[derive(Clone, Default)]
struct BytecodeIteratorMethods {
    into_iter: Option<usize>,
    next: Option<usize>,
}

#[derive(Clone)]
enum RuntimeType {
    Struct(Rc<StructType>),
    Enum(Rc<EnumType>),
}

#[derive(Clone)]
struct BytecodeFunction {
    name: String,
    exported: bool,
    constants: Vec<Constant>,
    instructions: Vec<SpannedInstruction>,
    register_count: usize,
    local_count: usize,
    local_mutability: Vec<bool>,
    parameter_count: usize,
    capture_count: usize,
    span: Span,
}

impl BytecodeModule {
    pub fn execute(&self) -> Result<Value, BytecodeError> {
        self.execute_with_limit(1_000_000)
    }

    pub fn execute_with_limit(&self, max_steps: usize) -> Result<Value, BytecodeError> {
        self.execute_with_host_and_limit(&BytecodeHost::standard(), max_steps)
    }

    pub fn execute_with_host(&self, host: &BytecodeHost) -> Result<Value, BytecodeError> {
        self.execute_with_host_and_limit(host, 1_000_000)
    }

    pub fn execute_with_host_and_limit(
        &self,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        self.verify()?;
        let imports = self.link(host)?;
        VirtualMachine::new(self, imports, max_steps).execute()
    }

    /// Calls a named bytecode function without executing the module entry point.
    ///
    /// This is intended for embedding hosts that keep a compiled module and invoke
    /// stateless script entry points repeatedly. Functions with captured values are
    /// rejected because their closure environment only exists while another
    /// bytecode invocation is running.
    pub fn call(&self, name: &str, arguments: Vec<Value>) -> Result<Value, BytecodeError> {
        self.call_with_host_and_limit(name, arguments, &BytecodeHost::standard(), 1_000_000)
    }

    pub fn call_with_host_and_limit(
        &self,
        name: &str,
        arguments: Vec<Value>,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        self.verify()?;
        let function = self
            .functions
            .iter()
            .position(|function| function.exported && function.name == name)
            .ok_or_else(|| {
                BytecodeError::new(
                    format!("unknown exported function `{name}`"),
                    Span::default(),
                )
            })?;
        if function == self.entry {
            return Err(BytecodeError::new(
                "the module entry point cannot be called by name",
                Span::default(),
            ));
        }
        let imports = self.link(host)?;
        VirtualMachine::new_call(self, imports, max_steps, function, arguments)?.execute()
    }

    pub fn imports(&self) -> &[BytecodeImport] {
        &self.imports
    }

    /// Verifies the bytecode and resolves every import against `host` without
    /// executing module code.
    pub fn validate_host(&self, host: &BytecodeHost) -> Result<(), BytecodeError> {
        self.verify()?;
        self.link(host).map(|_| ())
    }

    pub fn instruction_count(&self) -> usize {
        self.functions
            .iter()
            .map(|function| function.instructions.len())
            .sum()
    }

    pub fn register_count(&self) -> usize {
        self.functions
            .iter()
            .map(|function| function.register_count)
            .sum()
    }

    pub fn local_count(&self) -> usize {
        self.functions
            .iter()
            .map(|function| function.local_count)
            .sum()
    }

    pub fn function_count(&self) -> usize {
        self.functions.len().saturating_sub(1)
    }

    fn link(&self, host: &BytecodeHost) -> Result<Vec<Rc<BytecodeHostHandler>>, BytecodeError> {
        self.imports
            .iter()
            .map(|import| {
                if import.abi_version != host.abi_version {
                    return Err(BytecodeError::new(
                        format!(
                            "import `{}` requires host ABI {}, found {}",
                            import.name, import.abi_version, host.abi_version
                        ),
                        Span::default(),
                    ));
                }
                if !host.capabilities.contains(&import.capability) {
                    return Err(BytecodeError::new(
                        format!(
                            "capability `{}` required by import `{}` is not authorized",
                            import.capability, import.name
                        ),
                        Span::default(),
                    ));
                }
                let binding = host.functions.get(&import.name).ok_or_else(|| {
                    BytecodeError::new(
                        format!("missing bytecode import `{}`", import.name),
                        Span::default(),
                    )
                })?;
                if binding.capability != import.capability {
                    return Err(BytecodeError::new(
                        format!("capability mismatch for import `{}`", import.name),
                        Span::default(),
                    ));
                }
                if binding.signature != import.signature {
                    return Err(BytecodeError::new(
                        format!("signature mismatch for import `{}`", import.name),
                        Span::default(),
                    ));
                }
                Ok(binding.function.clone())
            })
            .collect()
    }
}

fn new_local_storage(function: &BytecodeFunction) -> Vec<StorageRef> {
    function
        .local_mutability
        .iter()
        .map(|mutable| Rc::new(RefCell::new(StorageSlot::uninitialized(*mutable))))
        .collect()
}

fn take_field_slot(
    slot: Option<&mut FieldSlot>,
    label: &str,
    span: Span,
) -> Result<Value, BytecodeError> {
    let slot = slot.ok_or_else(|| BytecodeError::new(format!("unknown {label}"), span))?;
    let value = slot
        .value
        .as_ref()
        .ok_or_else(|| BytecodeError::new(format!("{label} has been moved"), span))?;
    if value.is_copy() {
        value
            .clone_owned()
            .map_err(|message| BytecodeError::new(message, span))
    } else if slot.references > 0 {
        Err(BytecodeError::new(
            format!("cannot move {label} while it is referenced"),
            span,
        ))
    } else {
        Ok(slot.value.take().expect("slot value was checked"))
    }
}

fn store_field_slot(
    slot: Option<&mut FieldSlot>,
    label: &str,
    value: Value,
    span: Span,
) -> Result<(), BytecodeError> {
    let slot = slot.ok_or_else(|| BytecodeError::new(format!("unknown {label}"), span))?;
    if slot.references > 0 {
        return Err(BytecodeError::new(
            format!("cannot replace {label} while it is referenced"),
            span,
        ));
    }
    slot.value =
        Some(slot.type_annotation.constrain(&value).ok_or_else(|| {
            BytecodeError::new(format!("value is incompatible with {label}"), span)
        })?);
    Ok(())
}

fn access_error(error: AccessError, span: Span) -> BytecodeError {
    let message = match error {
        AccessError::Undefined | AccessError::Moved => "use of moved or uninitialized local",
        AccessError::Borrowed => "cannot move local while it is referenced",
        AccessError::PartiallyMoved => "use of partially moved local",
    };
    BytecodeError::new(message, span)
}

fn assign_error(error: AssignError, span: Span) -> BytecodeError {
    BytecodeError::new(
        match error {
            AssignError::Undefined => "assignment target is undefined".into(),
            AssignError::Immutable => "cannot assign to immutable local".into(),
            AssignError::TypeMismatch(expected) => {
                format!("assignment value must have type {expected}")
            }
            AssignError::OptionRequiresAnnotation => {
                "Option assignment requires a type annotation".into()
            }
            AssignError::ReferenceEscape => "reference cannot escape its scope".into(),
            AssignError::BorrowedTarget => {
                "cannot replace a value while part of it is referenced".into()
            }
        },
        span,
    )
}

pub fn compile(source: &str) -> Result<BytecodeModule, CompileError> {
    encode(rils_compiler::compile(source)?)
}

pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_with_host(source, host)?)
}

pub(crate) fn compile_program_with_host(
    program: &crate::ast::Program,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_program_with_host(program, host)?)
}

fn validate_contract_abi(host: &HostContract) -> Result<(), CompileError> {
    if host.host_abi_version() != BYTECODE_HOST_ABI_VERSION {
        return Err(CompileError {
            message: format!(
                "host contract ABI {} is incompatible with bytecode host ABI {}",
                host.host_abi_version(),
                BYTECODE_HOST_ABI_VERSION
            ),
            span: Span::default(),
        });
    }
    Ok(())
}

#[derive(Clone)]
enum Constant {
    Unit,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),
}

impl Constant {
    fn value(&self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Bool(value) => Value::Bool(*value),
            Self::I8(value) => Value::I8(*value),
            Self::I16(value) => Value::I16(*value),
            Self::I32(value) => Value::I32(*value),
            Self::I64(value) => Value::I64(*value),
            Self::I128(value) => Value::I128(*value),
            Self::Isize(value) => Value::Isize(*value),
            Self::U8(value) => Value::U8(*value),
            Self::U16(value) => Value::U16(*value),
            Self::U32(value) => Value::U32(*value),
            Self::U64(value) => Value::U64(*value),
            Self::U128(value) => Value::U128(*value),
            Self::Usize(value) => Value::Usize(*value),
            Self::F32(value) => Value::F32(*value),
            Self::F64(value) => Value::F64(*value),
            Self::Char(value) => Value::Char(*value),
            Self::String(value) => Value::String(Rc::from(value.as_str())),
        }
    }
}

#[derive(Clone)]
struct SpannedInstruction {
    instruction: Instruction,
    span: Span,
}

#[derive(Clone)]
struct BytecodePlace {
    local: usize,
    projections: Vec<BytecodeProjection>,
}

#[derive(Clone)]
enum BytecodeProjection {
    Field(String),
    Index(usize),
}

#[derive(Clone)]
enum Instruction {
    LoadConstant {
        destination: usize,
        constant: usize,
    },
    LoadFunction {
        destination: usize,
        function: usize,
    },
    BindMethod {
        destination: usize,
        function: usize,
        receiver: usize,
    },
    BorrowTemporary {
        destination: usize,
        source: usize,
        mutable: bool,
    },
    Reborrow {
        destination: usize,
        source: usize,
        mutable: bool,
    },
    CreateClosure {
        destination: usize,
        function: usize,
        captures: Vec<usize>,
    },
    TakeLocal {
        destination: usize,
        local: usize,
    },
    TakePlace {
        destination: usize,
        place: BytecodePlace,
    },
    StoreLocal {
        local: usize,
        source: usize,
    },
    InitLocal {
        local: usize,
        source: usize,
    },
    DropLocal {
        local: usize,
    },
    BorrowLocal {
        destination: usize,
        local: usize,
        mutable: bool,
    },
    BorrowPlace {
        destination: usize,
        place: BytecodePlace,
        mutable: bool,
    },
    Dereference {
        destination: usize,
        source: usize,
    },
    StoreDereference {
        reference: usize,
        source: usize,
    },
    StorePlace {
        place: BytecodePlace,
        source: usize,
    },
    IntoIterator {
        destination: usize,
        source: usize,
    },
    Move {
        destination: usize,
        source: usize,
    },
    Unary {
        destination: usize,
        operator: UnaryOp,
        operand: usize,
    },
    Cast {
        destination: usize,
        source: usize,
        target: IntegerType,
    },
    Binary {
        destination: usize,
        left: usize,
        operator: BinaryOp,
        right: usize,
    },
    Call {
        destination: usize,
        function: usize,
        arguments: Vec<usize>,
    },
    CallValue {
        destination: usize,
        callee: usize,
        arguments: Vec<usize>,
    },
    CallImport {
        destination: usize,
        import: usize,
        arguments: Vec<usize>,
    },
    CallIntrinsic {
        destination: usize,
        intrinsic: rils_builtins::IntrinsicId,
        target: Option<IntegerType>,
        arguments: Vec<usize>,
    },
    ConstructRecord {
        destination: usize,
        type_id: usize,
        variant: Option<String>,
        fields: Vec<(String, usize)>,
    },
    ConstructTupleVariant {
        destination: usize,
        type_id: usize,
        variant: String,
        fields: Vec<usize>,
    },
    ConstructUnitVariant {
        destination: usize,
        type_id: usize,
        variant: String,
    },
    BuildTuple {
        destination: usize,
        elements: Vec<usize>,
    },
    BuildArray {
        destination: usize,
        elements: Vec<usize>,
    },
    BuildRepeatArray {
        destination: usize,
        value: usize,
        count: usize,
    },
    BuildRange {
        destination: usize,
        start: usize,
        end: usize,
    },
    BuildOptionNone {
        destination: usize,
    },
    BuildOptionSome {
        destination: usize,
        source: usize,
    },
    BuildResultOk {
        destination: usize,
        source: usize,
    },
    BuildResultErr {
        destination: usize,
        source: usize,
    },
    TryResult {
        destination: usize,
        source: usize,
    },
    MatchPattern {
        destination: usize,
        source: usize,
        pattern: HirPattern,
    },
    BindPattern {
        source: usize,
        pattern: HirPattern,
    },
    Jump {
        target: usize,
    },
    Branch {
        condition: usize,
        then_target: usize,
        else_target: usize,
    },
    IteratorNext {
        iterator: usize,
        destination: usize,
        some_target: usize,
        none_target: usize,
    },
    Return {
        source: usize,
    },
    MatchFail,
}

fn pattern_locals_valid(pattern: &HirPattern, local_count: usize) -> bool {
    match pattern {
        HirPattern::Binding(local) => *local < local_count,
        HirPattern::Some(inner) | HirPattern::Ok(inner) | HirPattern::Err(inner) => {
            pattern_locals_valid(inner, local_count)
        }
        HirPattern::TupleVariant { fields, .. } => fields
            .iter()
            .all(|pattern| pattern_locals_valid(pattern, local_count)),
        HirPattern::Record { fields, .. } => fields
            .iter()
            .all(|(_, pattern)| pattern_locals_valid(pattern, local_count)),
        HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::None | HirPattern::Path(_) => {
            true
        }
    }
}

fn pattern_matches(pattern: &HirPattern, value: &Value) -> bool {
    match pattern {
        HirPattern::Wildcard | HirPattern::Binding(_) => true,
        HirPattern::Literal(literal) => hir_literal_value(literal) == *value,
        HirPattern::Some(inner) => match value {
            Value::Option {
                value: Some(value), ..
            } => pattern_matches(inner, value),
            _ => false,
        },
        HirPattern::None => matches!(value, Value::Option { value: None, .. }),
        HirPattern::Ok(inner) => match value {
            Value::Result {
                value: Ok(value), ..
            } => pattern_matches(inner, value),
            _ => false,
        },
        HirPattern::Err(inner) => match value {
            Value::Result {
                value: Err(value), ..
            } => pattern_matches(inner, value),
            _ => false,
        },
        HirPattern::TupleVariant { path, fields } => {
            let Some((enum_name, variant_name)) = pattern_variant(path) else {
                return false;
            };
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Tuple(values) = &instance.payload else {
                return false;
            };
            type_name_matches(&instance.type_definition.name, enum_name)
                && instance.variant == variant_name
                && fields.len() == values.len()
                && fields
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value))
        }
        HirPattern::Record { path, fields } => {
            if let Value::Struct(instance) = value
                && path
                    .last()
                    .is_some_and(|name| type_name_matches(&instance.type_definition.name, name))
            {
                let values = instance.fields.borrow();
                return fields.len() == values.len()
                    && fields.iter().all(|(name, pattern)| {
                        values
                            .get(name)
                            .and_then(|field| field.value.as_ref())
                            .is_some_and(|value| pattern_matches(pattern, value))
                    });
            }
            let Some((enum_name, variant_name)) = pattern_variant(path) else {
                return false;
            };
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Record(values) = &instance.payload else {
                return false;
            };
            type_name_matches(&instance.type_definition.name, enum_name)
                && instance.variant == variant_name
                && fields.len() == values.len()
                && fields.iter().all(|(name, pattern)| {
                    values
                        .get(name)
                        .is_some_and(|value| pattern_matches(pattern, value))
                })
        }
        HirPattern::Path(path) => {
            let Some((enum_name, variant_name)) = pattern_variant(path) else {
                return false;
            };
            matches!(
                value,
                Value::Enum(instance)
                    if type_name_matches(&instance.type_definition.name, enum_name)
                        && instance.variant == variant_name
                        && matches!(instance.payload, EnumPayload::Unit)
            )
        }
    }
}

fn collect_pattern_bindings(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
) {
    match pattern {
        HirPattern::Binding(local) => bindings.push((*local, value.clone())),
        HirPattern::Some(inner) => {
            if let Value::Option {
                value: Some(value), ..
            } = value
            {
                collect_pattern_bindings(inner, value, bindings);
            }
        }
        HirPattern::Ok(inner) => {
            if let Value::Result {
                value: Ok(value), ..
            } = value
            {
                collect_pattern_bindings(inner, value, bindings);
            }
        }
        HirPattern::Err(inner) => {
            if let Value::Result {
                value: Err(value), ..
            } = value
            {
                collect_pattern_bindings(inner, value, bindings);
            }
        }
        HirPattern::TupleVariant { fields, .. } => {
            if let Value::Enum(instance) = value
                && let EnumPayload::Tuple(values) = &instance.payload
            {
                for (pattern, value) in fields.iter().zip(values) {
                    collect_pattern_bindings(pattern, value, bindings);
                }
            }
        }
        HirPattern::Record { fields, .. } => match value {
            Value::Struct(instance) => {
                let values = instance.fields.borrow();
                for (name, pattern) in fields {
                    if let Some(value) = values.get(name).and_then(|field| field.value.as_ref()) {
                        collect_pattern_bindings(pattern, value, bindings);
                    }
                }
            }
            Value::Enum(instance) => {
                if let EnumPayload::Record(values) = &instance.payload {
                    for (name, pattern) in fields {
                        if let Some(value) = values.get(name) {
                            collect_pattern_bindings(pattern, value, bindings);
                        }
                    }
                }
            }
            _ => {}
        },
        HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::None | HirPattern::Path(_) => {}
    }
}

fn hir_literal_value(literal: &HirLiteral) -> Value {
    match literal {
        HirLiteral::Unit => Value::Unit,
        HirLiteral::Bool(value) => Value::Bool(*value),
        HirLiteral::I8(value) => Value::I8(*value),
        HirLiteral::I16(value) => Value::I16(*value),
        HirLiteral::I32(value) => Value::I32(*value),
        HirLiteral::I64(value) => Value::I64(*value),
        HirLiteral::I128(value) => Value::I128(*value),
        HirLiteral::Isize(value) => Value::Isize(*value),
        HirLiteral::U8(value) => Value::U8(*value),
        HirLiteral::U16(value) => Value::U16(*value),
        HirLiteral::U32(value) => Value::U32(*value),
        HirLiteral::U64(value) => Value::U64(*value),
        HirLiteral::U128(value) => Value::U128(*value),
        HirLiteral::Usize(value) => Value::Usize(*value),
        HirLiteral::F32(value) => Value::F32(*value),
        HirLiteral::F64(value) => Value::F64(*value),
        HirLiteral::Char(value) => Value::Char(*value),
        HirLiteral::String(value) => Value::String(Rc::from(value.as_str())),
    }
}

fn pattern_variant(path: &[String]) -> Option<(&str, &str)> {
    (path.len() >= 2).then(|| (path[path.len() - 2].as_str(), path[path.len() - 1].as_str()))
}

fn type_name_matches(canonical: &str, pattern: &str) -> bool {
    canonical == pattern || canonical.rsplit("::").next() == Some(pattern)
}

fn sequence_value(values: Vec<Value>, array: bool, span: Span) -> Result<Value, BytecodeError> {
    let mut element_type = Type::Unknown;
    if array {
        for value in &values {
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            element_type = merge_sequence_types(&element_type, &actual).ok_or_else(|| {
                BytecodeError::new(
                    format!(
                        "array elements must have one type, found `{element_type}` and `{actual}`"
                    ),
                    span,
                )
            })?;
        }
    }
    let elements = values
        .into_iter()
        .map(|value| FieldSlot {
            type_annotation: if array {
                element_type.clone()
            } else {
                Type::of_value(&value).unwrap_or(Type::Unknown)
            },
            value: Some(value),
            references: 0,
        })
        .collect();
    let sequence = Rc::new(SequenceValue {
        elements: RefCell::new(elements),
        element_type: RefCell::new(array.then_some(element_type)),
    });
    Ok(if array {
        Value::Array(sequence)
    } else {
        Value::Tuple(sequence)
    })
}

fn merge_sequence_types(left: &Type, right: &Type) -> Option<Type> {
    if left == &Type::Unknown {
        return Some(right.clone());
    }
    if right == &Type::Unknown || left == right {
        return Some(left.clone());
    }
    None
}

fn condition_value(value: &Value, span: Span) -> Result<bool, BytecodeError> {
    match value {
        Value::Unit => Err(BytecodeError::new(
            "`()` cannot be used as a condition",
            span,
        )),
        Value::Option { .. } => Err(BytecodeError::new(
            "Option cannot be used as a condition",
            span,
        )),
        value => Ok(value.is_truthy()),
    }
}

fn unary(operator: UnaryOp, value: Value, span: Span) -> Result<Value, BytecodeError> {
    match (operator, value) {
        (UnaryOp::Not, value) => Ok(Value::Bool(!condition_value(&value, span)?)),
        (UnaryOp::Negate, value) => {
            crate::numeric::negate(value).map_err(|message| BytecodeError::new(message, span))
        }
        (UnaryOp::Dereference, _) => Err(BytecodeError::new(
            "dereference is not supported by the bytecode MVP",
            span,
        )),
    }
}

fn binary(
    left: Value,
    operator: BinaryOp,
    right: Value,
    span: Span,
) -> Result<Value, BytecodeError> {
    use BinaryOp::*;
    if matches!(operator, Equal | NotEqual) {
        let equal = left == right;
        return Ok(Value::Bool(if operator == Equal { equal } else { !equal }));
    }
    if operator == Add
        && let (Value::String(left), Value::String(right)) = (&left, &right)
    {
        return Ok(Value::String(Rc::from(format!("{left}{right}"))));
    }
    crate::numeric::binary(left, operator, right)
        .map_err(|message| BytecodeError::new(message, span))
}

fn core_imports() -> Vec<(&'static str, FunctionSignature)> {
    let shared = || Type::Reference {
        mutable: false,
        inner: Box::new(Type::Unknown),
    };
    let mutable = || Type::Reference {
        mutable: true,
        inner: Box::new(Type::Unknown),
    };
    vec![
        (
            "type_of",
            FunctionSignature::fixed(vec![Type::Unknown], Type::String),
        ),
        (
            "clone",
            FunctionSignature::fixed(
                vec![Type::Reference {
                    mutable: false,
                    inner: Box::new(Type::Unknown),
                }],
                Type::Unknown,
            ),
        ),
        (
            "is_ok",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_err",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_some",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_none",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "unwrap",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown),
        ),
        (
            "unwrap_or",
            FunctionSignature::fixed(vec![Type::Unknown, Type::Unknown], Type::Unknown),
        ),
        ("core::assert", FunctionSignature::variadic(Type::Unit)),
        (
            "core::vec::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::vec::from",
            FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::sequence::len",
            FunctionSignature::fixed(vec![shared()], Type::USIZE),
        ),
        (
            "core::value::is_empty",
            FunctionSignature::fixed(vec![shared()], Type::Bool),
        ),
        (
            "core::vec::push",
            FunctionSignature::fixed(vec![mutable(), Type::Unknown], Type::Unit),
        ),
        (
            "core::vec::pop",
            FunctionSignature::fixed(vec![mutable()], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::vec::clear",
            FunctionSignature::fixed(vec![mutable()], Type::Unit),
        ),
        (
            "core::vec::truncate",
            FunctionSignature::fixed(vec![mutable(), Type::USIZE], Type::Unit),
        ),
        (
            "core::string::contains",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::starts_with",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::ends_with",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::find",
            FunctionSignature::fixed(
                vec![shared(), Type::String],
                Type::Option(Box::new(Type::USIZE)),
            ),
        ),
        (
            "core::string::trim",
            FunctionSignature::fixed(vec![shared()], Type::String),
        ),
        (
            "core::string::replace",
            FunctionSignature::fixed(vec![shared(), Type::String, Type::String], Type::String),
        ),
        (
            "core::value::expect",
            FunctionSignature::fixed(vec![Type::Unknown, Type::String], Type::Unknown),
        ),
        (
            "core::result::ok",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::result::err",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::option::take",
            FunctionSignature::fixed(vec![mutable()], Type::Option(Box::new(Type::Unknown))),
        ),
    ]
}

fn call_core_import(name: &str, arguments: &[Value]) -> Result<Value, String> {
    match name {
        "type_of" => Ok(Value::String(Rc::from(arguments[0].type_name()))),
        "clone" => match &arguments[0] {
            Value::Reference(reference) => reference.read()?.clone_owned(),
            value => Err(format!(
                "`clone` expects a reference, found {}; use `clone(&value)`",
                value.type_name()
            )),
        },
        "is_ok" => match &arguments[0] {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_ok())),
            value => Err(format!(
                "`is_ok` expects Result, found {}",
                value.type_name()
            )),
        },
        "is_err" => match &arguments[0] {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_err())),
            value => Err(format!(
                "`is_err` expects Result, found {}",
                value.type_name()
            )),
        },
        "is_some" => match &arguments[0] {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_some())),
            value => Err(format!(
                "`is_some` expects Option, found {}",
                value.type_name()
            )),
        },
        "is_none" => match &arguments[0] {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_none())),
            value => Err(format!(
                "`is_none` expects Option, found {}",
                value.type_name()
            )),
        },
        "unwrap" => match &arguments[0] {
            Value::Option {
                value: Some(value), ..
            }
            | Value::Result {
                value: Ok(value), ..
            } => value.clone_owned(),
            Value::Option { value: None, .. } => Err("called `unwrap` on `None`".into()),
            Value::Result {
                value: Err(value), ..
            } => Err(format!("called `unwrap` on Err({value})")),
            value => Err(format!(
                "`unwrap` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        "unwrap_or" => match &arguments[0] {
            Value::Option {
                value,
                element_type,
            } => {
                if let Some(expected) = element_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                value
                    .as_ref()
                    .map_or_else(|| arguments[1].clone_owned(), |value| value.clone_owned())
            }
            Value::Result { value, ok_type, .. } => {
                if let Some(expected) = ok_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                match value {
                    Ok(value) => value.clone_owned(),
                    Err(_) => arguments[1].clone_owned(),
                }
            }
            value => Err(format!(
                "`unwrap_or` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        "core::assert" => match arguments.first() {
            Some(Value::Bool(true)) => Ok(Value::Unit),
            Some(Value::Bool(false)) => Err(arguments
                .get(1)
                .map(ToString::to_string)
                .unwrap_or_else(|| "assertion failed".into())),
            Some(value) => Err(format!(
                "`assert` expects bool, found {}",
                value.type_name()
            )),
            None => Err("`assert` expects at least one argument".into()),
        },
        "core::vec::new" => Ok(Value::Vec(Rc::new(SequenceValue {
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        }))),
        "core::vec::from" => {
            let Value::Array(array) = &arguments[0] else {
                return Err("Vec::from expects an array".into());
            };
            if array
                .elements
                .borrow()
                .iter()
                .any(|slot| slot.references > 0)
            {
                return Err("cannot move an array into Vec while an element is referenced".into());
            }
            let elements = array.elements.borrow_mut().drain(..).collect();
            Ok(Value::Vec(Rc::new(SequenceValue {
                elements: RefCell::new(elements),
                element_type: RefCell::new(array.element_type.borrow().clone()),
            })))
        }
        "core::sequence::len" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("len receiver must be a reference".into());
            };
            let value = reference.read()?;
            let length = match value {
                Value::Array(sequence) | Value::Vec(sequence) => sequence.elements.borrow().len(),
                Value::String(value) => value.len(),
                value => {
                    return Err(format!(
                        "len receiver is not a collection: {}",
                        value.type_name()
                    ));
                }
            };
            Ok(Value::Usize(length))
        }
        "core::value::is_empty" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("is_empty receiver must be a reference".into());
            };
            let value = reference.read()?;
            let empty = match value {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    sequence.elements.borrow().is_empty()
                }
                Value::String(value) => value.is_empty(),
                value => return Err(format!("{} has no is_empty method", value.type_name())),
            };
            Ok(Value::Bool(empty))
        }
        "core::vec::push" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::push requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::push requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("push receiver is not Vec".into());
            };
            let value = &arguments[1];
            if value.contains_reference() {
                return Err("Vec cannot own local references".into());
            }
            let current = sequence
                .elements
                .borrow()
                .first()
                .map(|slot| slot.type_annotation.clone())
                .or_else(|| sequence.element_type.borrow().clone())
                .unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&current, &actual)
                .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
            *sequence.element_type.borrow_mut() = Some(element_type.clone());
            sequence.elements.borrow_mut().push(FieldSlot {
                value: Some(value.clone()),
                type_annotation: element_type,
                references: 0,
            });
            Ok(Value::Unit)
        }
        "core::vec::pop" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::pop requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::pop requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("pop receiver is not Vec".into());
            };
            let element_type = sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let value = {
                let mut elements = sequence.elements.borrow_mut();
                if elements.last().is_some_and(|slot| slot.references > 0) {
                    return Err("cannot pop a referenced Vec element".into());
                }
                elements.pop().and_then(|slot| slot.value).map(Rc::new)
            };
            Ok(Value::Option {
                value,
                element_type: Some(element_type),
            })
        }
        "core::vec::clear" | "core::vec::truncate" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            let length = if name == "core::vec::clear" {
                0
            } else {
                let Value::Usize(length) = arguments[1] else {
                    return Err("Vec::truncate length must be usize".into());
                };
                length
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements
                .get(length..)
                .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
            {
                return Err("cannot remove a referenced Vec element".into());
            }
            elements.truncate(length);
            Ok(Value::Unit)
        }
        name if name.starts_with("core::string::") => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("string method receiver must be a reference".into());
            };
            let Value::String(value) = reference.read()? else {
                return Err("string method receiver is not string".into());
            };
            let argument = |index: usize| match arguments.get(index) {
                Some(Value::String(value)) => Ok(value.as_ref()),
                Some(value) => Err(format!(
                    "string argument must be string, found {}",
                    value.type_name()
                )),
                None => Err("missing string argument".into()),
            };
            match name {
                "core::string::contains" => Ok(Value::Bool(value.contains(argument(1)?))),
                "core::string::starts_with" => Ok(Value::Bool(value.starts_with(argument(1)?))),
                "core::string::ends_with" => Ok(Value::Bool(value.ends_with(argument(1)?))),
                "core::string::find" => Ok(Value::Option {
                    value: value
                        .find(argument(1)?)
                        .map(|offset| Rc::new(Value::Usize(offset))),
                    element_type: Some(Type::USIZE),
                }),
                "core::string::trim" => Ok(Value::String(Rc::from(value.trim()))),
                "core::string::replace" => Ok(Value::String(Rc::from(
                    value.replace(argument(1)?, argument(2)?),
                ))),
                _ => Err(format!("unknown string import `{name}`")),
            }
        }
        "core::value::expect" => {
            let Value::String(message) = &arguments[1] else {
                return Err("expect message must be string".into());
            };
            match &arguments[0] {
                Value::Option {
                    value: Some(value), ..
                }
                | Value::Result {
                    value: Ok(value), ..
                } => value.clone_owned(),
                Value::Option { value: None, .. } => Err(message.to_string()),
                Value::Result {
                    value: Err(value), ..
                } => Err(format!("{message}: {value}")),
                value => Err(format!(
                    "expect requires Option or Result, found {}",
                    value.type_name()
                )),
            }
        }
        "core::result::ok" | "core::result::err" => {
            let Value::Result {
                value,
                ok_type,
                error_type,
            } = &arguments[0]
            else {
                return Err("Result conversion receiver is not Result".into());
            };
            let (value, element_type) = match (name, value) {
                ("core::result::ok", Ok(value)) => (Some(value.clone()), ok_type.clone()),
                ("core::result::err", Err(value)) => (Some(value.clone()), error_type.clone()),
                ("core::result::ok", Err(_)) => (None, ok_type.clone()),
                ("core::result::err", Ok(_)) => (None, error_type.clone()),
                _ => unreachable!(),
            };
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        "core::option::take" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Option::take requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Option::take requires `&mut self`".into());
            }
            let Value::Option {
                value,
                element_type,
            } = reference.read()?
            else {
                return Err("Option::take receiver is not Option".into());
            };
            reference
                .write(Value::Option {
                    value: None,
                    element_type: element_type.clone(),
                })
                .map_err(|error| assign_error(error, Span::default()).message)?;
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        _ => Err(format!("unknown core import `{name}`")),
    }
}

#[cfg(test)]
#[path = "bytecode/tests.rs"]
mod tests;
