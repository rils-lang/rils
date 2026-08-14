//! Experimental, panic-safe, host-neutral C ABI for embedding Rils.
//!
//! Handles and their backing objects are bound to the thread that created them.
//! This matches Unity's main-thread plugin usage and lets the facade hold the
//! current non-`Send` bytecode representation without exposing Rust layouts.

use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::atomic::{AtomicU16, Ordering},
};

use rils::{
    BytecodeHost, BytecodeModule, FloatType, FunctionSignature, HostContract, IntegerType, Span,
    Type, Value,
};

pub const RILS_ABI_VERSION: u32 = 1;
pub const RILS_STATUS_OK: i32 = 0;
pub const RILS_STATUS_INVALID_ARGUMENT: i32 = 1;
pub const RILS_STATUS_INVALID_HANDLE: i32 = 2;
pub const RILS_STATUS_COMPILE_ERROR: i32 = 3;
pub const RILS_STATUS_EXECUTION_ERROR: i32 = 4;
pub const RILS_STATUS_UNSUPPORTED_VALUE: i32 = 5;
pub const RILS_STATUS_BYTECODE_ERROR: i32 = 6;
pub const RILS_STATUS_PANIC: i32 = 255;

pub const RILS_VALUE_UNIT: u32 = 0;
pub const RILS_VALUE_BOOL: u32 = 1;
pub const RILS_VALUE_I8: u32 = 2;
pub const RILS_VALUE_I16: u32 = 3;
pub const RILS_VALUE_I32: u32 = 4;
pub const RILS_VALUE_I64: u32 = 5;
pub const RILS_VALUE_I128: u32 = 6;
pub const RILS_VALUE_ISIZE: u32 = 7;
pub const RILS_VALUE_U8: u32 = 8;
pub const RILS_VALUE_U16: u32 = 9;
pub const RILS_VALUE_U32: u32 = 10;
pub const RILS_VALUE_U64: u32 = 11;
pub const RILS_VALUE_U128: u32 = 12;
pub const RILS_VALUE_USIZE: u32 = 13;
pub const RILS_VALUE_F32: u32 = 14;
pub const RILS_VALUE_F64: u32 = 15;
pub const RILS_VALUE_CHAR: u32 = 16;

type Handle = u64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsSlice {
    pub data: *const u8,
    pub length: usize,
}

impl Default for RilsSlice {
    fn default() -> Self {
        Self {
            data: ptr::null(),
            length: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RilsValue {
    pub tag: u32,
    pub reserved: u32,
    pub low: u64,
    pub high: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostFunction {
    pub function_id: u64,
    pub name: RilsSlice,
    pub capability: RilsSlice,
    pub parameter_tags: *const u32,
    pub parameter_count: usize,
    pub return_tag: u32,
    pub reserved: u32,
}

pub type RilsHostDispatcher = unsafe extern "C" fn(
    user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    out_error: *mut RilsSlice,
) -> i32;

#[derive(Default)]
struct LastError {
    code: i32,
    message: String,
    source_name: String,
    span: Span,
}

#[derive(Clone)]
struct Runtime {
    max_steps: usize,
    modules: Vec<Handle>,
    instances: Vec<Handle>,
    host_contract: HostContract,
    host: BytecodeHost,
    allowed_capabilities: HashSet<String>,
    dispatcher: Option<RilsHostDispatcher>,
    dispatcher_user_data: *mut c_void,
    host_frozen: bool,
}

#[derive(Clone)]
struct Module {
    runtime: Handle,
    bytecode: BytecodeModule,
    source_name: String,
}

fn module_source_name(module: &Module, span: Span) -> &str {
    module
        .bytecode
        .source_name(span.source)
        .unwrap_or(&module.source_name)
}

#[derive(Clone, Copy)]
struct Instance {
    runtime: Handle,
    module: Handle,
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

struct SlotMap<T> {
    slots: Vec<Slot<T>>,
    kind: u8,
}

impl<T> SlotMap<T> {
    fn new(kind: u8) -> Self {
        Self {
            slots: Vec::new(),
            kind,
        }
    }
    fn insert(&mut self, value: T) -> Handle {
        if let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.value.is_none())
        {
            slot.value = Some(value);
            return encode_handle(index, slot.generation, self.kind);
        }
        let index = self.slots.len();
        self.slots.push(Slot {
            generation: 1,
            value: Some(value),
        });
        encode_handle(index, 1, self.kind)
    }

    fn get(&self, handle: Handle) -> Option<&T> {
        let (index, generation) = decode_handle(handle, self.kind)?;
        let slot = self.slots.get(index)?;
        (slot.generation == generation)
            .then_some(slot.value.as_ref())
            .flatten()
    }

    fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        let (index, generation) = decode_handle(handle, self.kind)?;
        let slot = self.slots.get_mut(index)?;
        (slot.generation == generation)
            .then_some(slot.value.as_mut())
            .flatten()
    }

    fn remove(&mut self, handle: Handle) -> Option<T> {
        let (index, generation) = decode_handle(handle, self.kind)?;
        let slot = self.slots.get_mut(index)?;
        if slot.generation != generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = (slot.generation % u16::MAX as u32) + 1;
        Some(value)
    }
}

struct State {
    runtimes: SlotMap<Runtime>,
    modules: SlotMap<Module>,
    instances: SlotMap<Instance>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runtimes: SlotMap::new(1),
            modules: SlotMap::new(2),
            instances: SlotMap::new(3),
        }
    }
}

static NEXT_THREAD_ID: AtomicU16 = AtomicU16::new(1);

thread_local! {
    static THREAD_ID: u16 = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed).max(1);
    static STATE: RefCell<State> = RefCell::new(State::default());
    static LAST_ERROR: RefCell<LastError> = RefCell::new(LastError::default());
    static HOST_CALLBACK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

fn encode_handle(index: usize, generation: u32, kind: u8) -> Handle {
    let thread = THREAD_ID.with(|id| *id) as u64;
    ((kind as u64) << 62) | (thread << 46) | ((generation as u64) << 30) | (index as u64 + 1)
}

fn decode_handle(handle: Handle, expected_kind: u8) -> Option<(usize, u32)> {
    let low = (handle & 0x3fff_ffff) as u32;
    let generation = ((handle >> 30) & 0xffff) as u32;
    let thread = ((handle >> 46) & 0xffff) as u16;
    let kind = (handle >> 62) as u8;
    let current_thread = THREAD_ID.with(|id| *id);
    (low != 0 && generation != 0 && thread == current_thread && kind == expected_kind)
        .then_some(((low - 1) as usize, generation))
}

fn clear_error() {
    LAST_ERROR.with(|error| *error.borrow_mut() = LastError::default());
}

fn fail(code: i32, message: impl Into<String>, source_name: &str, span: Span) -> i32 {
    LAST_ERROR.with(|error| {
        *error.borrow_mut() = LastError {
            code,
            message: message.into(),
            source_name: source_name.into(),
            span,
        };
    });
    code
}

fn status_entry(function: impl FnOnce() -> i32) -> i32 {
    if HOST_CALLBACK_ACTIVE.with(Cell::get) {
        return fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "reentrant C API calls from a host dispatcher are not allowed",
            "",
            Span::default(),
        );
    }
    clear_error();
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(status) => status,
        Err(_) => fail(
            RILS_STATUS_PANIC,
            "Rust panic caught at the Rils C ABI boundary",
            "",
            Span::default(),
        ),
    }
}

fn handle_entry(function: impl FnOnce() -> Handle) -> Handle {
    if HOST_CALLBACK_ACTIVE.with(Cell::get) {
        fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "reentrant C API calls from a host dispatcher are not allowed",
            "",
            Span::default(),
        );
        return 0;
    }
    clear_error();
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(handle) => handle,
        Err(_) => {
            fail(
                RILS_STATUS_PANIC,
                "Rust panic caught at the Rils C ABI boundary",
                "",
                Span::default(),
            );
            0
        }
    }
}

struct HostCallbackGuard;

impl HostCallbackGuard {
    fn enter() -> Result<Self, String> {
        HOST_CALLBACK_ACTIVE.with(|active| {
            if active.replace(true) {
                Err("nested host dispatcher calls are not allowed".into())
            } else {
                Ok(Self)
            }
        })
    }
}

impl Drop for HostCallbackGuard {
    fn drop(&mut self) {
        HOST_CALLBACK_ACTIVE.with(|active| active.set(false));
    }
}

fn clone_module(runtime: Handle, module: Handle) -> Result<Module, i32> {
    STATE.with(|state| {
        let state = state.borrow();
        if state.runtimes.get(runtime).is_none() {
            return Err(fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime handle",
                "",
                Span::default(),
            ));
        }
        state
            .modules
            .get(module)
            .filter(|value| value.runtime == runtime)
            .cloned()
            .ok_or_else(|| {
                fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid module handle or module does not belong to runtime",
                    "",
                    Span::default(),
                )
            })
    })
}

fn runtime_host_snapshot(runtime: Handle) -> Result<(HostContract, BytecodeHost), i32> {
    STATE.with(|state| {
        let state = state.borrow();
        let runtime = state.runtimes.get(runtime).ok_or_else(|| {
            fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime handle",
                "",
                Span::default(),
            )
        })?;
        if !runtime.host_contract.is_empty() && !runtime.host_frozen {
            return Err(fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host registry must be frozen before module creation",
                "",
                Span::default(),
            ));
        }
        Ok((runtime.host_contract.clone(), runtime.host.clone()))
    })
}

unsafe fn read_bytes<'a>(value: RilsSlice) -> Result<&'a [u8], i32> {
    if value.length == 0 {
        return Ok(&[]);
    }
    if value.data.is_null() {
        return Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "slice data is null while length is non-zero",
            "",
            Span::default(),
        ));
    }
    // SAFETY: The C caller promises that the input range is readable for this call.
    Ok(unsafe { slice::from_raw_parts(value.data, value.length) })
}

unsafe fn read_utf8(value: RilsSlice, label: &str) -> Result<&str, i32> {
    // SAFETY: Forwarding the caller's slice contract to `read_bytes`.
    let bytes = unsafe { read_bytes(value)? };
    std::str::from_utf8(bytes).map_err(|_| {
        fail(
            RILS_STATUS_INVALID_ARGUMENT,
            format!("{label} is not valid UTF-8"),
            "",
            Span::default(),
        )
    })
}

fn from_ffi_value(value: RilsValue) -> Result<Value, i32> {
    if value.reserved != 0 {
        return Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "reserved value fields must be zero",
            "",
            Span::default(),
        ));
    }
    let require_zero_high = || {
        if value.high == 0 {
            Ok(())
        } else {
            Err(fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "high payload must be zero for this value tag",
                "",
                Span::default(),
            ))
        }
    };
    macro_rules! signed {
        ($variant:ident, $type:ty) => {{
            require_zero_high()?;
            <$type>::try_from(value.low as i64)
                .map(Value::$variant)
                .map_err(|_| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "signed integer payload is out of range",
                        "",
                        Span::default(),
                    )
                })
        }};
    }
    macro_rules! unsigned {
        ($variant:ident, $type:ty) => {{
            require_zero_high()?;
            <$type>::try_from(value.low)
                .map(Value::$variant)
                .map_err(|_| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "unsigned integer payload is out of range",
                        "",
                        Span::default(),
                    )
                })
        }};
    }
    match value.tag {
        RILS_VALUE_UNIT => Ok(Value::Unit),
        RILS_VALUE_BOOL if value.high == 0 && (value.low == 0 || value.low == 1) => {
            Ok(Value::Bool(value.low != 0))
        }
        RILS_VALUE_BOOL => Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "bool payload must be 0 or 1",
            "",
            Span::default(),
        )),
        RILS_VALUE_I8 => signed!(I8, i8),
        RILS_VALUE_I16 => signed!(I16, i16),
        RILS_VALUE_I32 => signed!(I32, i32),
        RILS_VALUE_I64 => signed!(I64, i64),
        RILS_VALUE_I128 => Ok(Value::I128(
            ((u128::from(value.high) << 64) | u128::from(value.low)) as i128,
        )),
        RILS_VALUE_ISIZE => signed!(Isize, isize),
        RILS_VALUE_U8 => unsigned!(U8, u8),
        RILS_VALUE_U16 => unsigned!(U16, u16),
        RILS_VALUE_U32 => unsigned!(U32, u32),
        RILS_VALUE_U64 => unsigned!(U64, u64),
        RILS_VALUE_U128 => Ok(Value::U128(
            (u128::from(value.high) << 64) | u128::from(value.low),
        )),
        RILS_VALUE_USIZE => unsigned!(Usize, usize),
        RILS_VALUE_F32 => {
            require_zero_high()?;
            let bits = u32::try_from(value.low).map_err(|_| {
                fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "f32 payload is out of range",
                    "",
                    Span::default(),
                )
            })?;
            Ok(Value::F32(f32::from_bits(bits)))
        }
        RILS_VALUE_F64 => {
            require_zero_high()?;
            Ok(Value::F64(f64::from_bits(value.low)))
        }
        RILS_VALUE_CHAR => {
            require_zero_high()?;
            let scalar = u32::try_from(value.low)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "char payload is not a Unicode scalar value",
                        "",
                        Span::default(),
                    )
                })?;
            Ok(Value::Char(scalar))
        }
        _ => Err(fail(
            RILS_STATUS_UNSUPPORTED_VALUE,
            format!("unsupported C ABI value tag {}", value.tag),
            "",
            Span::default(),
        )),
    }
}

fn to_ffi_value(value: Value, source_name: &str) -> Result<RilsValue, i32> {
    let scalar = |tag, low, high| RilsValue {
        tag,
        low,
        high,
        ..RilsValue::default()
    };
    let value = match value {
        Value::Unit => RilsValue::default(),
        Value::Bool(value) => scalar(RILS_VALUE_BOOL, u64::from(value), 0),
        Value::I8(value) => scalar(RILS_VALUE_I8, value as i64 as u64, 0),
        Value::I16(value) => scalar(RILS_VALUE_I16, value as i64 as u64, 0),
        Value::I32(value) => scalar(RILS_VALUE_I32, value as i64 as u64, 0),
        Value::I64(value) => scalar(RILS_VALUE_I64, value as u64, 0),
        Value::I128(value) => scalar(
            RILS_VALUE_I128,
            value as u128 as u64,
            (value as u128 >> 64) as u64,
        ),
        Value::Isize(value) => scalar(RILS_VALUE_ISIZE, value as i64 as u64, 0),
        Value::U8(value) => scalar(RILS_VALUE_U8, u64::from(value), 0),
        Value::U16(value) => scalar(RILS_VALUE_U16, u64::from(value), 0),
        Value::U32(value) => scalar(RILS_VALUE_U32, u64::from(value), 0),
        Value::U64(value) => scalar(RILS_VALUE_U64, value, 0),
        Value::U128(value) => scalar(RILS_VALUE_U128, value as u64, (value >> 64) as u64),
        Value::Usize(value) => scalar(RILS_VALUE_USIZE, value as u64, 0),
        Value::F32(value) => scalar(RILS_VALUE_F32, u64::from(value.to_bits()), 0),
        Value::F64(value) => scalar(RILS_VALUE_F64, value.to_bits(), 0),
        Value::Char(value) => scalar(RILS_VALUE_CHAR, u64::from(u32::from(value)), 0),
        other => {
            return Err(fail(
                RILS_STATUS_UNSUPPORTED_VALUE,
                format!(
                    "return type `{}` is not supported by the prototype C ABI",
                    other.type_name()
                ),
                source_name,
                Span::default(),
            ));
        }
    };
    Ok(value)
}

fn current_error_message() -> String {
    LAST_ERROR.with(|error| error.borrow().message.clone())
}

fn portable_type_from_tag(tag: u32, allow_unit: bool) -> Result<Type, String> {
    match tag {
        RILS_VALUE_UNIT if allow_unit => Ok(Type::Unit),
        RILS_VALUE_BOOL => Ok(Type::Bool),
        RILS_VALUE_I32 => Ok(Type::Integer(IntegerType::I32)),
        RILS_VALUE_I64 => Ok(Type::Integer(IntegerType::I64)),
        RILS_VALUE_U32 => Ok(Type::Integer(IntegerType::U32)),
        RILS_VALUE_U64 => Ok(Type::Integer(IntegerType::U64)),
        RILS_VALUE_F32 => Ok(Type::Float(FloatType::F32)),
        RILS_VALUE_F64 => Ok(Type::Float(FloatType::F64)),
        _ => Err(format!(
            "value tag {tag} is not supported by the portable host contract"
        )),
    }
}

fn portable_tag_from_type(ty: &Type, allow_unit: bool) -> Result<u32, String> {
    match ty {
        Type::Unit if allow_unit => Ok(RILS_VALUE_UNIT),
        Type::Bool => Ok(RILS_VALUE_BOOL),
        Type::Integer(IntegerType::I32) => Ok(RILS_VALUE_I32),
        Type::Integer(IntegerType::I64) => Ok(RILS_VALUE_I64),
        Type::Integer(IntegerType::U32) => Ok(RILS_VALUE_U32),
        Type::Integer(IntegerType::U64) => Ok(RILS_VALUE_U64),
        Type::Float(FloatType::F32) => Ok(RILS_VALUE_F32),
        Type::Float(FloatType::F64) => Ok(RILS_VALUE_F64),
        _ => Err(format!(
            "host manifest type `{ty}` is not supported by the current C dispatcher ABI"
        )),
    }
}

fn validate_c_dispatcher_contract(contract: &HostContract) -> Result<(), String> {
    if contract.host_abi_version() != rils::BYTECODE_HOST_ABI_VERSION {
        return Err(format!(
            "host manifest ABI {} is incompatible with runtime ABI {}",
            contract.host_abi_version(),
            rils::BYTECODE_HOST_ABI_VERSION
        ));
    }
    for function in contract.functions() {
        let parameters = function
            .signature
            .parameters
            .as_ref()
            .expect("host contract signatures are fixed");
        for parameter in parameters {
            portable_tag_from_type(parameter, false)?;
        }
        portable_tag_from_type(&function.signature.return_type, true)?;
    }
    Ok(())
}

fn copy_callback_error(error: RilsSlice) -> String {
    if error.length == 0 {
        return "host dispatcher returned an error".into();
    }
    if error.data.is_null() {
        return "host dispatcher returned an invalid error slice".into();
    }
    // SAFETY: The dispatcher contract keeps this slice readable until the callback returns.
    let bytes = unsafe { slice::from_raw_parts(error.data, error.length) };
    std::str::from_utf8(bytes).map_or_else(
        |_| "host dispatcher returned a non-UTF-8 error message".into(),
        str::to_owned,
    )
}

fn invoke_host_dispatcher(
    dispatcher: RilsHostDispatcher,
    user_data: *mut c_void,
    function_id: u64,
    function_name: &str,
    signature: &FunctionSignature,
    arguments: &[Value],
) -> Result<Value, String> {
    let parameters = signature
        .parameters
        .as_ref()
        .ok_or_else(|| format!("host function `{function_name}` is variadic"))?;
    if parameters.len() != arguments.len() {
        return Err(format!(
            "host function `{function_name}` expects {} arguments, found {}",
            parameters.len(),
            arguments.len()
        ));
    }
    for (parameter, argument) in parameters.iter().zip(arguments) {
        if parameter.constrain(argument).is_none() {
            return Err(format!(
                "host function `{function_name}` received `{}` for parameter type `{parameter}`",
                argument.type_name()
            ));
        }
    }
    let encoded = arguments
        .iter()
        .cloned()
        .map(|value| to_ffi_value(value, "").map_err(|_| current_error_message()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = RilsValue::default();
    let mut error = RilsSlice::default();
    let _callback_guard = HostCallbackGuard::enter()?;
    // SAFETY: All pointers remain valid for the callback duration. The callback may only borrow
    // them and must initialize `out_value` on success.
    let status = unsafe {
        dispatcher(
            user_data,
            function_id,
            encoded.as_ptr(),
            encoded.len(),
            &mut result,
            &mut error,
        )
    };
    if status != RILS_STATUS_OK {
        return Err(format!(
            "host function `{function_name}` failed with status {status}: {}",
            copy_callback_error(error)
        ));
    }
    let result = from_ffi_value(result).map_err(|_| current_error_message())?;
    signature.return_type.constrain(&result).ok_or_else(|| {
        format!(
            "host function `{function_name}` returned `{}`, expected `{}`",
            result.type_name(),
            signature.return_type
        )
    })
}

fn build_runtime_host(runtime: &Runtime) -> Result<BytecodeHost, String> {
    let mut host = BytecodeHost::standard();
    for capability in &runtime.allowed_capabilities {
        host.allow_capability(capability.clone());
    }
    let dispatcher = runtime.dispatcher;
    for function in runtime.host_contract.functions() {
        let dispatcher = dispatcher.ok_or_else(|| {
            "a host dispatcher must be set before freezing a non-empty host contract".to_string()
        })?;
        let user_data = runtime.dispatcher_user_data;
        let function_id = function.function_id;
        let function_name = function.name.clone();
        let signature = function.signature.clone();
        let callback_name = function_name.clone();
        let callback_signature = signature.clone();
        host.register_function(
            function_name,
            signature,
            function.capability.clone(),
            move |arguments| {
                invoke_host_dispatcher(
                    dispatcher,
                    user_data,
                    function_id,
                    &callback_name,
                    &callback_signature,
                    arguments,
                )
            },
        )?;
    }
    Ok(host)
}

mod error_api;
mod instance_api;
mod module_api;
mod runtime_api;

pub use error_api::*;
pub use instance_api::*;
pub use module_api::*;
pub use runtime_api::*;

#[cfg(test)]
mod tests;
