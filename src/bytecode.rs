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
    source::{SourceFile, SourceId, Span, format_diagnostic},
    types::{FunctionSignature, IntegerType, Type},
    value::{
        BytecodeFunctionValue, BytecodeIteratorValue, EnumInstance, EnumPayload, EnumType,
        FieldSlot, HashMapValue, HashSetValue, RangeValue, ReferenceValue, SequenceIteratorValue,
        SequenceValue, StructInstance, StructType, Value,
    },
};

mod core_imports;
mod encoder;
mod format;
mod host;
mod verifier;
mod vm;

use core_imports::*;
use encoder::encode;
pub use format::{BYTECODE_FORMAT_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeFormatError};
pub use host::{BYTECODE_HOST_ABI_VERSION, BytecodeHost, BytecodeHostHandler, BytecodeImport};
use vm::VirtualMachine;

pub use rils_compiler::{
    CompileError, HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM,
    HOST_MANIFEST_FORMAT_VERSION, HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION,
    HOST_MANIFEST_JSON_MAX_BYTES, HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES,
    HOST_MANIFEST_MAX_FUNCTIONS, HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS,
    HOST_MANIFEST_MAX_TYPES, HostCallKind, HostContract, HostFunctionDeclaration,
    HostModuleDeclaration, HostReceiver, HostThreadAffinity, HostTypeDeclaration,
    HostTypeTransport,
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
    sources: Vec<SourceFile>,
    functions: Vec<BytecodeFunction>,
    types: Vec<RuntimeType>,
    imports: Vec<BytecodeImport>,
    iterators: HashMap<String, BytecodeIteratorMethods>,
    trait_implementations: Vec<BytecodeTraitImplementation>,
    entry: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeTraitImplementation {
    target: String,
    trait_name: String,
    source: SourceId,
    methods: HashMap<String, usize>,
}

impl BytecodeTraitImplementation {
    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn trait_name(&self) -> &str {
        &self.trait_name
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn methods(&self) -> impl Iterator<Item = &str> {
        self.methods.keys().map(String::as_str)
    }
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
    pub fn sources(&self) -> &[SourceFile] {
        &self.sources
    }

    pub fn source_name(&self, source: SourceId) -> Option<&str> {
        self.sources
            .iter()
            .find(|file| file.id == source)
            .map(|file| file.name.as_str())
    }

    pub fn trait_implementations(
        &self,
        trait_name: &str,
    ) -> impl Iterator<Item = &BytecodeTraitImplementation> {
        self.trait_implementations
            .iter()
            .filter(move |implementation| {
                trait_name_matches(&implementation.trait_name, trait_name)
            })
    }

    pub fn construct_default_with_host_and_limit(
        &self,
        target: &str,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        let implementation = self
            .trait_implementations("Default")
            .find(|implementation| implementation.target == target)
            .ok_or_else(|| {
                BytecodeError::new(
                    format!("type `{target}` does not implement `Default`"),
                    Span::default(),
                )
            })?;
        let function = implementation
            .methods
            .get("default")
            .copied()
            .ok_or_else(|| {
                BytecodeError::new(
                    format!("`Default` implementation for `{target}` has no `default` method"),
                    Span::default(),
                )
            })?;
        self.call_function_with_host_and_limit(function, Vec::new(), host, max_steps)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the public trait dispatch API keeps target, method, host, and budget explicit"
    )]
    pub fn call_trait_method_with_host_and_limit(
        &self,
        target: &str,
        trait_name: &str,
        method_name: &str,
        receiver: &mut Value,
        mut arguments: Vec<Value>,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        let implementation = self
            .trait_implementations(trait_name)
            .find(|implementation| implementation.target == target)
            .ok_or_else(|| {
                BytecodeError::new(
                    format!("type `{target}` does not implement `{trait_name}`"),
                    Span::default(),
                )
            })?;
        let function = implementation
            .methods
            .get(method_name)
            .copied()
            .ok_or_else(|| {
                BytecodeError::new(
                    format!("trait `{trait_name}` has no method `{method_name}` for `{target}`"),
                    Span::default(),
                )
            })?;
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        storage.borrow_mut().initialize(receiver.clone());
        arguments.insert(
            0,
            Value::Reference(Rc::new(ReferenceValue::new_storage(storage.clone(), true))),
        );
        let result = self.call_function_with_host_and_limit(function, arguments, host, max_steps);
        *receiver = storage.borrow().read().map_err(|_| {
            BytecodeError::new(
                format!("trait method `{trait_name}::{method_name}` moved its receiver"),
                Span::default(),
            )
        })?;
        result
    }

    fn call_function_with_host_and_limit(
        &self,
        function: usize,
        arguments: Vec<Value>,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        let bytecode_function = self.functions.get(function).ok_or_else(|| {
            BytecodeError::new(
                "trait method function index is out of bounds",
                Span::default(),
            )
        })?;
        if bytecode_function.capture_count != 0 {
            return Err(BytecodeError::new(
                format!(
                    "function `{}` requires a closure environment",
                    bytecode_function.name
                ),
                bytecode_function.span,
            ));
        }
        let imports = self.link(host)?;
        VirtualMachine::new_call(
            self,
            imports,
            crate::ExecutionLimits {
                max_steps,
                ..crate::ExecutionLimits::default()
            },
            function,
            arguments,
        )?
        .execute()
    }

    pub fn execute(&self) -> Result<Value, BytecodeError> {
        self.execute_with_limits(crate::ExecutionLimits::default())
    }

    pub fn execute_with_limit(&self, max_steps: usize) -> Result<Value, BytecodeError> {
        self.execute_with_limits(crate::ExecutionLimits {
            max_steps,
            ..crate::ExecutionLimits::default()
        })
    }

    pub fn execute_with_limits(
        &self,
        limits: crate::ExecutionLimits,
    ) -> Result<Value, BytecodeError> {
        self.execute_with_host_and_limits(&BytecodeHost::standard(), limits)
    }

    pub fn execute_with_host(&self, host: &BytecodeHost) -> Result<Value, BytecodeError> {
        self.execute_with_host_and_limits(host, crate::ExecutionLimits::default())
    }

    pub fn execute_with_host_and_limit(
        &self,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        self.execute_with_host_and_limits(
            host,
            crate::ExecutionLimits {
                max_steps,
                ..crate::ExecutionLimits::default()
            },
        )
    }

    pub fn execute_with_host_and_limits(
        &self,
        host: &BytecodeHost,
        limits: crate::ExecutionLimits,
    ) -> Result<Value, BytecodeError> {
        self.verify()?;
        let imports = self.link(host)?;
        VirtualMachine::new(self, imports, limits).execute()
    }

    /// Calls a named bytecode function without executing the module entry point.
    ///
    /// This is intended for embedding hosts that keep a compiled module and invoke
    /// stateless script entry points repeatedly. Functions with captured values are
    /// rejected because their closure environment only exists while another
    /// bytecode invocation is running.
    pub fn call(&self, name: &str, arguments: Vec<Value>) -> Result<Value, BytecodeError> {
        self.call_with_host_and_limits(
            name,
            arguments,
            &BytecodeHost::standard(),
            crate::ExecutionLimits::default(),
        )
    }

    pub fn call_with_host_and_limit(
        &self,
        name: &str,
        arguments: Vec<Value>,
        host: &BytecodeHost,
        max_steps: usize,
    ) -> Result<Value, BytecodeError> {
        self.call_with_host_and_limits(
            name,
            arguments,
            host,
            crate::ExecutionLimits {
                max_steps,
                ..crate::ExecutionLimits::default()
            },
        )
    }

    pub fn call_with_host_and_limits(
        &self,
        name: &str,
        arguments: Vec<Value>,
        host: &BytecodeHost,
        limits: crate::ExecutionLimits,
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
        VirtualMachine::new_call(self, imports, limits, function, arguments)?.execute()
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

fn trait_name_matches(stored: &str, requested: &str) -> bool {
    stored == requested
        || stored
            .rsplit("::")
            .next()
            .is_some_and(|name| name == requested)
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

pub(crate) fn compile_program_with_host_and_sources(
    program: &crate::ast::Program,
    host: &HostContract,
    sources: Vec<SourceFile>,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_program_with_host_and_sources(
        program, host, sources,
    )?)
}

fn validate_contract_abi(host: &HostContract) -> Result<(), CompileError> {
    if host.host_abi_version() != BYTECODE_HOST_ABI_VERSION {
        return Err(CompileError::new(
            format!(
                "host contract ABI {} is incompatible with bytecode host ABI {}",
                host.host_abi_version(),
                BYTECODE_HOST_ABI_VERSION
            ),
            Span::default(),
        ));
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

#[cfg(test)]
#[path = "bytecode/tests.rs"]
mod tests;
