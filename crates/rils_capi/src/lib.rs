//! Experimental, panic-safe, host-neutral C ABI for embedding Rils.
//!
//! Handles and their backing objects are bound to the thread that created them.
//! This matches Unity's main-thread plugin usage and lets the facade hold the
//! current non-`Send` bytecode representation without exposing Rust layouts.

use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::atomic::{AtomicU16, Ordering},
};

use rils::{BytecodeModule, Span, Value};

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
}

#[derive(Clone)]
struct Module {
    runtime: Handle,
    bytecode: BytecodeModule,
    source_name: String,
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

#[unsafe(no_mangle)]
pub extern "C" fn rils_abi_version() -> u32 {
    RILS_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_create() -> Handle {
    handle_entry(|| {
        STATE.with(|state| {
            state.borrow_mut().runtimes.insert(Runtime {
                max_steps: 1_000_000,
                modules: Vec::new(),
                instances: Vec::new(),
            })
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_destroy(runtime: Handle) -> i32 {
    status_entry(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(runtime_value) = state.runtimes.remove(runtime) else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            };
            for instance in runtime_value.instances {
                state.instances.remove(instance);
            }
            for module in runtime_value.modules {
                state.modules.remove(module);
            }
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_set_max_steps(runtime: Handle, max_steps: u64) -> i32 {
    status_entry(|| {
        let Ok(max_steps) = usize::try_from(max_steps) else {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "step limit is too large",
                "",
                Span::default(),
            );
        };
        if max_steps == 0 {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "step limit must be non-zero",
                "",
                Span::default(),
            );
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(runtime) = state.runtimes.get_mut(runtime) else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            };
            runtime.max_steps = max_steps;
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Compiles a UTF-8 source slice into a module owned by `runtime`.
///
/// # Safety
///
/// Non-empty input slices must point to readable memory for the duration of the call, and
/// `out_module` must point to writable storage for one handle.
pub unsafe extern "C" fn rils_module_compile(
    runtime: Handle,
    source_name: RilsSlice,
    source: RilsSlice,
    out_module: *mut Handle,
) -> i32 {
    status_entry(|| {
        if out_module.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_module is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: Input pointers are only read during this call.
        let source_name = match unsafe { read_utf8(source_name, "source name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Input pointers are only read during this call.
        let source = match unsafe { read_utf8(source, "source") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let bytecode = match rils::compile(source) {
            Ok(module) => module,
            Err(error) => {
                return fail(
                    RILS_STATUS_COMPILE_ERROR,
                    error.message,
                    source_name,
                    error.span,
                );
            }
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            }
            let module = state.modules.insert(Module {
                runtime,
                bytecode,
                source_name: source_name.into(),
            });
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .modules
                .push(module);
            // SAFETY: `out_module` was checked for null and the caller promises writable storage.
            unsafe { out_module.write(module) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Loads a Rils source file and its external modules into a module owned by `runtime`.
///
/// # Safety
///
/// A non-empty UTF-8 path slice must point to readable memory for the duration of the call, and
/// `out_module` must point to writable storage for one handle.
pub unsafe extern "C" fn rils_module_compile_file(
    runtime: Handle,
    path: RilsSlice,
    out_module: *mut Handle,
) -> i32 {
    status_entry(|| {
        if out_module.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_module is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: The path is read only during this call.
        let path = match unsafe { read_utf8(path, "path") } {
            Ok("") => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "path must not be empty",
                    "",
                    Span::default(),
                );
            }
            Ok(value) => value,
            Err(status) => return status,
        };
        let bytecode = match rils::compile_file(path) {
            Ok(module) => module,
            Err(error) => {
                return fail(RILS_STATUS_COMPILE_ERROR, error.message, path, error.span);
            }
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            }
            let module = state.modules.insert(Module {
                runtime,
                bytecode,
                source_name: path.into(),
            });
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .modules
                .push(module);
            // SAFETY: `out_module` was checked and the caller promises writable storage.
            unsafe { out_module.write(module) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Loads and verifies a bytecode module from an in-memory `.rilbc` image.
///
/// # Safety
///
/// A non-empty bytecode slice must point to readable memory for the duration of the call, and
/// `out_module` must point to writable storage for one handle.
pub unsafe extern "C" fn rils_module_load_bytecode(
    runtime: Handle,
    bytecode: RilsSlice,
    out_module: *mut Handle,
) -> i32 {
    status_entry(|| {
        if out_module.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_module is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: The bytecode is read only during this call.
        let bytes = match unsafe { read_bytes(bytecode) } {
            Ok(bytes) => bytes,
            Err(status) => return status,
        };
        let bytecode = match BytecodeModule::from_bytes(bytes) {
            Ok(module) => module,
            Err(error) => {
                return fail(
                    RILS_STATUS_BYTECODE_ERROR,
                    error.message,
                    "<bytecode>",
                    Span::default(),
                );
            }
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            }
            let module = state.modules.insert(Module {
                runtime,
                bytecode,
                source_name: "<bytecode>".into(),
            });
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .modules
                .push(module);
            // SAFETY: `out_module` was checked and the caller promises writable storage.
            unsafe { out_module.write(module) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Loads and verifies a `.rilbc` file into a module owned by `runtime`.
///
/// # Safety
///
/// A non-empty UTF-8 path slice must point to readable memory for the duration of the call, and
/// `out_module` must point to writable storage for one handle.
pub unsafe extern "C" fn rils_module_load_bytecode_file(
    runtime: Handle,
    path: RilsSlice,
    out_module: *mut Handle,
) -> i32 {
    status_entry(|| {
        if out_module.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_module is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: The path is read only during this call.
        let path = match unsafe { read_utf8(path, "path") } {
            Ok("") => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "path must not be empty",
                    "",
                    Span::default(),
                );
            }
            Ok(value) => value,
            Err(status) => return status,
        };
        let bytecode = match BytecodeModule::read_file(path) {
            Ok(module) => module,
            Err(error) => {
                return fail(
                    RILS_STATUS_BYTECODE_ERROR,
                    error.message,
                    path,
                    Span::default(),
                );
            }
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime handle",
                    "",
                    Span::default(),
                );
            }
            let module = state.modules.insert(Module {
                runtime,
                bytecode,
                source_name: path.into(),
            });
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .modules
                .push(module);
            // SAFETY: `out_module` was checked and the caller promises writable storage.
            unsafe { out_module.write(module) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Returns the number of bytes required to serialize `module` as `.rilbc`.
///
/// # Safety
///
/// `out_size` must point to writable storage for one `size_t` value.
pub unsafe extern "C" fn rils_module_bytecode_size(
    runtime: Handle,
    module: Handle,
    out_size: *mut usize,
) -> i32 {
    status_entry(|| {
        if out_size.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_size is null",
                "",
                Span::default(),
            );
        }
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let bytecode = match module.bytecode.to_bytes() {
            Ok(bytecode) => bytecode,
            Err(error) => {
                return fail(
                    RILS_STATUS_BYTECODE_ERROR,
                    error.message,
                    &module.source_name,
                    Span::default(),
                );
            }
        };
        // SAFETY: `out_size` was checked and the caller promises writable storage.
        unsafe { out_size.write(bytecode.len()) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Serializes `module` as `.rilbc` into caller-owned memory.
///
/// If the buffer is too small, `out_written` receives the required size and the function returns
/// `RILS_STATUS_INVALID_ARGUMENT` without writing to `buffer`.
///
/// # Safety
///
/// `out_written` must point to writable storage for one `size_t`. When `buffer_capacity` is
/// non-zero, `buffer` must point to a writable range of at least that many bytes.
pub unsafe extern "C" fn rils_module_write_bytecode(
    runtime: Handle,
    module: Handle,
    buffer: *mut u8,
    buffer_capacity: usize,
    out_written: *mut usize,
) -> i32 {
    status_entry(|| {
        if out_written.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_written is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: `out_written` was checked and the caller promises writable storage.
        unsafe { out_written.write(0) };
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let bytecode = match module.bytecode.to_bytes() {
            Ok(bytecode) => bytecode,
            Err(error) => {
                return fail(
                    RILS_STATUS_BYTECODE_ERROR,
                    error.message,
                    &module.source_name,
                    Span::default(),
                );
            }
        };
        // SAFETY: `out_written` remains valid for this call.
        unsafe { out_written.write(bytecode.len()) };
        if buffer_capacity < bytecode.len() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                format!(
                    "bytecode buffer is too small: requires {}, received {buffer_capacity}",
                    bytecode.len()
                ),
                &module.source_name,
                Span::default(),
            );
        }
        if !bytecode.is_empty() && buffer.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "bytecode buffer is null",
                &module.source_name,
                Span::default(),
            );
        }
        // SAFETY: The caller promises a writable buffer of `buffer_capacity` bytes, which was
        // checked to be at least the serialized bytecode length. The source and destination do
        // not overlap because the source is owned by this call.
        unsafe { ptr::copy_nonoverlapping(bytecode.as_ptr(), buffer, bytecode.len()) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Serializes `module` to a `.rilbc` file.
///
/// # Safety
///
/// A non-empty UTF-8 path slice must point to readable memory for the duration of the call.
pub unsafe extern "C" fn rils_module_write_bytecode_file(
    runtime: Handle,
    module: Handle,
    path: RilsSlice,
) -> i32 {
    status_entry(|| {
        // SAFETY: The path is read only during this call.
        let path = match unsafe { read_utf8(path, "path") } {
            Ok("") => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "path must not be empty",
                    "",
                    Span::default(),
                );
            }
            Ok(value) => value,
            Err(status) => return status,
        };
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        match module.bytecode.write_file(path) {
            Ok(()) => RILS_STATUS_OK,
            Err(error) => fail(
                RILS_STATUS_BYTECODE_ERROR,
                error.message,
                &module.source_name,
                Span::default(),
            ),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_module_destroy(runtime: Handle, module: Handle) -> i32 {
    status_entry(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(module_value) = state.modules.get(module) else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid module handle",
                    "",
                    Span::default(),
                );
            };
            if module_value.runtime != runtime || state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "module does not belong to runtime",
                    "",
                    Span::default(),
                );
            }
            let children = state
                .runtimes
                .get(runtime)
                .expect("runtime was checked")
                .instances
                .clone();
            for handle in children {
                if state
                    .instances
                    .get(handle)
                    .is_some_and(|instance| instance.module == module)
                {
                    state.instances.remove(handle);
                }
            }
            state.modules.remove(module);
            let surviving_instances = state
                .runtimes
                .get(runtime)
                .expect("runtime was checked")
                .instances
                .iter()
                .copied()
                .filter(|handle| state.instances.get(*handle).is_some())
                .collect();
            let runtime = state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked");
            runtime.modules.retain(|handle| *handle != module);
            runtime.instances = surviving_instances;
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Creates an instance owned by `runtime` for a previously compiled module.
///
/// # Safety
///
/// `out_instance` must point to writable storage for one handle.
pub unsafe extern "C" fn rils_instance_create(
    runtime: Handle,
    module: Handle,
    out_instance: *mut Handle,
) -> i32 {
    status_entry(|| {
        if out_instance.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_instance is null",
                "",
                Span::default(),
            );
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.runtimes.get(runtime).is_none()
                || state
                    .modules
                    .get(module)
                    .is_none_or(|value| value.runtime != runtime)
            {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime or module handle",
                    "",
                    Span::default(),
                );
            }
            let instance = state.instances.insert(Instance { runtime, module });
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .instances
                .push(instance);
            // SAFETY: `out_instance` was checked and the caller promises writable storage.
            unsafe { out_instance.write(instance) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_instance_destroy(runtime: Handle, instance: Handle) -> i32 {
    status_entry(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state
                .instances
                .get(instance)
                .is_none_or(|value| value.runtime != runtime)
                || state.runtimes.get(runtime).is_none()
            {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime or instance handle",
                    "",
                    Span::default(),
                );
            }
            state.instances.remove(instance);
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .instances
                .retain(|handle| *handle != instance);
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Executes the compiled module entry point for an instance.
///
/// # Safety
///
/// `out_value` must point to writable storage for one value.
pub unsafe extern "C" fn rils_instance_execute(
    runtime: Handle,
    instance: Handle,
    out_value: *mut RilsValue,
) -> i32 {
    status_entry(|| {
        if out_value.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_value is null",
                "",
                Span::default(),
            );
        }
        let resolved = STATE.with(|state| {
            let state = state.borrow();
            let instance_value = *state.instances.get(instance)?;
            if instance_value.runtime != runtime {
                return None;
            }
            let runtime_value = state.runtimes.get(runtime)?;
            let module = state.modules.get(instance_value.module)?.clone();
            Some((runtime_value.max_steps, module))
        });
        let Some((max_steps, module)) = resolved else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime or instance handle",
                "",
                Span::default(),
            );
        };
        let value = match module
            .bytecode
            .execute_with_host_and_limit(&rils::BytecodeHost::standard(), max_steps)
        {
            Ok(value) => value,
            Err(error) => {
                return fail(
                    RILS_STATUS_EXECUTION_ERROR,
                    error.message,
                    &module.source_name,
                    error.span,
                );
            }
        };
        let value = match to_ffi_value(value, &module.source_name) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: `out_value` was checked and the caller promises writable storage.
        unsafe { out_value.write(value) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Calls an exported script function with scalar arguments.
///
/// # Safety
///
/// Non-empty slices must be readable for the duration of the call, and `out_value` must point
/// to writable storage for one value.
pub unsafe extern "C" fn rils_instance_call(
    runtime: Handle,
    instance: Handle,
    function_name: RilsSlice,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
) -> i32 {
    status_entry(|| {
        if out_value.is_null() || (argument_count != 0 && arguments.is_null()) {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "invalid argument or output pointer",
                "",
                Span::default(),
            );
        }
        // SAFETY: The function-name range is read only during this call.
        let function_name = match unsafe { read_utf8(function_name, "function name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Null is accepted only for an empty slice; otherwise the caller promises readability.
        let arguments = if argument_count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(arguments, argument_count) }
        };
        let arguments = match arguments
            .iter()
            .copied()
            .map(from_ffi_value)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(values) => values,
            Err(status) => return status,
        };
        let resolved = STATE.with(|state| {
            let state = state.borrow();
            let instance_value = *state.instances.get(instance)?;
            if instance_value.runtime != runtime {
                return None;
            }
            let runtime_value = state.runtimes.get(runtime)?;
            let module = state.modules.get(instance_value.module)?.clone();
            Some((runtime_value.max_steps, module))
        });
        let Some((max_steps, module)) = resolved else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime or instance handle",
                "",
                Span::default(),
            );
        };
        let value = match module.bytecode.call_with_host_and_limit(
            function_name,
            arguments,
            &rils::BytecodeHost::standard(),
            max_steps,
        ) {
            Ok(value) => value,
            Err(error) => {
                return fail(
                    RILS_STATUS_EXECUTION_ERROR,
                    error.message,
                    &module.source_name,
                    error.span,
                );
            }
        };
        let value = match to_ffi_value(value, &module.source_name) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: `out_value` was checked and the caller promises writable storage.
        unsafe { out_value.write(value) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_code() -> i32 {
    LAST_ERROR.with(|error| error.borrow().code)
}

fn error_slice(value: &str) -> RilsSlice {
    RilsSlice {
        data: value.as_ptr(),
        length: value.len(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_message() -> RilsSlice {
    LAST_ERROR.with(|error| error_slice(&error.borrow().message))
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_source_name() -> RilsSlice {
    LAST_ERROR.with(|error| error_slice(&error.borrow().source_name))
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_span_start() -> u64 {
    LAST_ERROR.with(|error| error.borrow().span.start as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_span_end() -> u64 {
    LAST_ERROR.with(|error| error.borrow().span.end as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(value: &str) -> RilsSlice {
        RilsSlice {
            data: value.as_ptr(),
            length: value.len(),
        }
    }

    fn raw_bytes(value: &[u8]) -> RilsSlice {
        RilsSlice {
            data: value.as_ptr(),
            length: value.len(),
        }
    }

    #[test]
    fn compiles_and_calls_two_number_function() {
        let runtime = rils_runtime_create();
        let mut module = 0;
        let source = "pub fn add(left: i32, right: i32) -> i32 { left + right }";
        // SAFETY: All pointers refer to live Rust test values for the duration of each call.
        assert_eq!(
            unsafe { rils_module_compile(runtime, bytes("demo.rils"), bytes(source), &mut module) },
            RILS_STATUS_OK
        );
        let mut instance = 0;
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_instance_create(runtime, module, &mut instance) },
            RILS_STATUS_OK
        );
        let arguments = [
            RilsValue {
                tag: RILS_VALUE_I32,
                low: 20,
                ..RilsValue::default()
            },
            RilsValue {
                tag: RILS_VALUE_I32,
                low: 22,
                ..RilsValue::default()
            },
        ];
        let mut result = RilsValue::default();
        // SAFETY: Input and output ranges are valid for the call.
        assert_eq!(
            unsafe {
                rils_instance_call(
                    runtime,
                    instance,
                    bytes("add"),
                    arguments.as_ptr(),
                    arguments.len(),
                    &mut result,
                )
            },
            RILS_STATUS_OK
        );
        assert_eq!(result.tag, RILS_VALUE_I32);
        assert_eq!(result.low, 42);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn loads_and_executes_bytecode_from_memory() {
        let image = rils::compile("40 + 2")
            .unwrap()
            .to_bytes()
            .expect("bytecode serializes");
        let runtime = rils_runtime_create();
        let mut module = 0;
        // SAFETY: The image and output pointer remain valid for the duration of the call.
        assert_eq!(
            unsafe { rils_module_load_bytecode(runtime, raw_bytes(&image), &mut module) },
            RILS_STATUS_OK
        );
        let mut instance = 0;
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_instance_create(runtime, module, &mut instance) },
            RILS_STATUS_OK
        );
        let mut result = RilsValue::default();
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_instance_execute(runtime, instance, &mut result) },
            RILS_STATUS_OK
        );
        assert_eq!(result.tag, RILS_VALUE_I32);
        assert_eq!(result.low, 42);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);

        let runtime = rils_runtime_create();
        let mut module = 0;
        let mut corrupted = image;
        *corrupted.last_mut().unwrap() ^= 1;
        // SAFETY: The image and output pointer remain valid for the duration of the call.
        assert_eq!(
            unsafe { rils_module_load_bytecode(runtime, raw_bytes(&corrupted), &mut module) },
            RILS_STATUS_BYTECODE_ERROR
        );
        assert_eq!(rils_last_error_code(), RILS_STATUS_BYTECODE_ERROR);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn exports_compiled_bytecode_to_memory_and_file() {
        let runtime = rils_runtime_create();
        let mut module = 0;
        // SAFETY: All pointers refer to live Rust test values for the duration of the call.
        assert_eq!(
            unsafe {
                rils_module_compile(runtime, bytes("export.rils"), bytes("40 + 2"), &mut module)
            },
            RILS_STATUS_OK
        );

        let mut size = 0;
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_module_bytecode_size(runtime, module, &mut size) },
            RILS_STATUS_OK
        );
        assert!(size > 0);

        let mut image = vec![0; size];
        let mut written = 0;
        // SAFETY: The output buffer and size pointer are valid.
        assert_eq!(
            unsafe {
                rils_module_write_bytecode(
                    runtime,
                    module,
                    image.as_mut_ptr(),
                    image.len(),
                    &mut written,
                )
            },
            RILS_STATUS_OK
        );
        assert_eq!(written, size);
        assert_eq!(
            BytecodeModule::from_bytes(&image)
                .unwrap()
                .execute()
                .unwrap(),
            Value::I32(42)
        );

        let mut small = vec![0; size - 1];
        written = 0;
        // SAFETY: The deliberately small output buffer and size pointer are valid.
        assert_eq!(
            unsafe {
                rils_module_write_bytecode(
                    runtime,
                    module,
                    small.as_mut_ptr(),
                    small.len(),
                    &mut written,
                )
            },
            RILS_STATUS_INVALID_ARGUMENT
        );
        assert_eq!(written, size);

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rils-capi-bytecode-export-{}-{unique}.rilbc",
            std::process::id()
        ));
        let path_text = path.to_str().unwrap();
        // SAFETY: The path slice is valid for the duration of the call.
        assert_eq!(
            unsafe { rils_module_write_bytecode_file(runtime, module, bytes(path_text)) },
            RILS_STATUS_OK
        );
        assert_eq!(
            BytecodeModule::read_file(&path).unwrap().execute().unwrap(),
            Value::I32(42)
        );
        std::fs::remove_file(path).unwrap();
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn scalar_value_protocol_round_trips_all_payload_shapes() {
        let values = [
            Value::I8(-8),
            Value::I64(i64::MIN),
            Value::I128(i128::MIN + 42),
            Value::Isize(-9),
            Value::U8(8),
            Value::U64(u64::MAX),
            Value::U128(u128::MAX - 42),
            Value::Usize(9),
            Value::F32(1.25),
            Value::F64(-2.5),
            Value::Char('你'),
        ];
        for expected in values {
            let encoded = to_ffi_value(expected.clone(), "").unwrap();
            assert_eq!(from_ffi_value(encoded).unwrap(), expected);
        }
    }

    #[test]
    fn compiles_executes_file_and_loads_external_modules() {
        let directory =
            std::env::temp_dir().join(format!("rils-capi-file-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let entry = directory.join("main.rils");
        let dependency = directory.join("math.rils");
        std::fs::write(&entry, "mod math; use math::answer; answer()").unwrap();
        std::fs::write(&dependency, "pub fn answer() -> i32 { 42 }").unwrap();

        let runtime = rils_runtime_create();
        let mut module = 0;
        let entry_text = entry.to_str().unwrap();
        // SAFETY: The path and output pointer remain valid for the duration of the call.
        assert_eq!(
            unsafe { rils_module_compile_file(runtime, bytes(entry_text), &mut module) },
            RILS_STATUS_OK
        );
        let mut instance = 0;
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_instance_create(runtime, module, &mut instance) },
            RILS_STATUS_OK
        );
        let mut result = RilsValue::default();
        // SAFETY: The output pointer is valid.
        assert_eq!(
            unsafe { rils_instance_execute(runtime, instance, &mut result) },
            RILS_STATUS_OK
        );
        assert_eq!(result.tag, RILS_VALUE_I32);
        assert_eq!(result.low, 42);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);

        std::fs::remove_file(entry).unwrap();
        std::fs::remove_file(dependency).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn rejects_stale_handles_and_reports_compile_spans() {
        let runtime = rils_runtime_create();
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_INVALID_HANDLE);

        let runtime = rils_runtime_create();
        let mut module = 0;
        // SAFETY: All pointers refer to live Rust test values for the duration of the call.
        let status = unsafe {
            rils_module_compile(
                runtime,
                bytes("broken.rils"),
                bytes("let = 1;"),
                &mut module,
            )
        };
        assert_eq!(status, RILS_STATUS_COMPILE_ERROR);
        assert_eq!(rils_last_error_code(), RILS_STATUS_COMPILE_ERROR);
        let name = rils_last_error_source_name();
        // SAFETY: Error strings remain borrowed until the next non-getter ABI call.
        assert_eq!(
            unsafe { std::str::from_utf8_unchecked(slice::from_raw_parts(name.data, name.length)) },
            "broken.rils"
        );
        assert!(rils_last_error_span_end() >= rils_last_error_span_start());
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn recycles_ten_thousand_generation_handles() {
        let runtime = rils_runtime_create();
        let mut module = 0;
        let source = "pub fn answer() -> i32 { 42 }";
        // SAFETY: All pointers refer to live Rust test values for the duration of the call.
        assert_eq!(
            unsafe {
                rils_module_compile(runtime, bytes("stress.rils"), bytes(source), &mut module)
            },
            RILS_STATUS_OK
        );
        for _ in 0..10_000 {
            let mut instance = 0;
            // SAFETY: The output pointer is valid.
            assert_eq!(
                unsafe { rils_instance_create(runtime, module, &mut instance) },
                RILS_STATUS_OK
            );
            assert_eq!(rils_instance_destroy(runtime, instance), RILS_STATUS_OK);
            assert_eq!(
                rils_instance_destroy(runtime, instance),
                RILS_STATUS_INVALID_HANDLE
            );
        }
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn rejects_wrong_kind_and_cross_thread_handles() {
        let runtime = rils_runtime_create();
        let mut module = 0;
        // SAFETY: All pointers refer to live Rust test values for the duration of the call.
        assert_eq!(
            unsafe {
                rils_module_compile(
                    runtime,
                    bytes("kind.rils"),
                    bytes("pub fn answer() -> i32 { 42 }"),
                    &mut module,
                )
            },
            RILS_STATUS_OK
        );
        assert_eq!(
            rils_runtime_set_max_steps(module, 10),
            RILS_STATUS_INVALID_HANDLE
        );
        let cross_thread = std::thread::spawn(move || rils_runtime_destroy(runtime));
        assert_eq!(cross_thread.join().unwrap(), RILS_STATUS_INVALID_HANDLE);
        assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    }

    #[test]
    fn converts_panics_to_status_errors() {
        assert_eq!(status_entry(|| panic!("boundary test")), RILS_STATUS_PANIC);
        assert_eq!(rils_last_error_code(), RILS_STATUS_PANIC);
    }
}
