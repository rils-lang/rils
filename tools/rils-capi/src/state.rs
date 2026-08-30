use super::*;

#[derive(Default)]
pub(crate) struct LastError {
    pub(crate) code: i32,
    pub(crate) message: String,
    pub(crate) source_name: String,
    pub(crate) span: Span,
}

#[derive(Clone)]
pub(crate) struct Runtime {
    pub(crate) max_steps: usize,
    pub(crate) modules: Vec<Handle>,
    pub(crate) instances: Vec<Handle>,
    pub(crate) script_values: Vec<Handle>,
    pub(crate) host_contract: HostContract,
    pub(crate) host: BytecodeHost,
    pub(crate) allowed_capabilities: HashSet<String>,
    pub(crate) dispatcher: Option<RilsHostDispatcher>,
    pub(crate) dispatcher_user_data: *mut c_void,
    pub(crate) output_callback: Option<RilsOutputCallback>,
    pub(crate) output_user_data: *mut c_void,
    pub(crate) host_value_formatter: Option<RilsHostValueFormatCallback>,
    pub(crate) host_value_formatter_user_data: *mut c_void,
    pub(crate) host_frozen: bool,
}

#[derive(Clone)]
pub(crate) struct LogicalHostType {
    pub(crate) name: String,
    pub(crate) base_types: HashSet<String>,
    pub(crate) transport: HostTypeTransport,
    pub(crate) value_layout: Option<HostValueLayout>,
}

#[derive(Clone)]
pub(crate) struct Module {
    pub(crate) runtime: Handle,
    pub(crate) bytecode: BytecodeModule,
    pub(crate) source_name: String,
}

pub(crate) fn module_source_name(module: &Module, span: Span) -> &str {
    module
        .bytecode
        .source_name(span.source)
        .unwrap_or(&module.source_name)
}

#[derive(Clone)]
pub(crate) struct Instance {
    pub(crate) runtime: Handle,
    pub(crate) module: Handle,
    pub(crate) script_values: Vec<Handle>,
}

#[derive(Clone)]
pub(crate) struct ScriptValue {
    pub(crate) runtime: Handle,
    pub(crate) instance: Handle,
    pub(crate) target: String,
    pub(crate) value: Value,
}

pub(crate) struct Slot<T> {
    pub(crate) generation: u32,
    pub(crate) value: Option<T>,
}

pub(crate) struct SlotMap<T> {
    pub(crate) slots: Vec<Slot<T>>,
    pub(crate) kind: u8,
}

impl<T> SlotMap<T> {
    pub(crate) fn new(kind: u8) -> Self {
        Self {
            slots: Vec::new(),
            kind,
        }
    }
    pub(crate) fn insert(&mut self, value: T) -> Handle {
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

    pub(crate) fn get(&self, handle: Handle) -> Option<&T> {
        let (index, generation) = decode_handle(handle, self.kind)?;
        let slot = self.slots.get(index)?;
        (slot.generation == generation)
            .then_some(slot.value.as_ref())
            .flatten()
    }

    pub(crate) fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        let (index, generation) = decode_handle(handle, self.kind)?;
        let slot = self.slots.get_mut(index)?;
        (slot.generation == generation)
            .then_some(slot.value.as_mut())
            .flatten()
    }

    pub(crate) fn remove(&mut self, handle: Handle) -> Option<T> {
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

pub(crate) struct State {
    pub(crate) runtimes: SlotMap<Runtime>,
    pub(crate) modules: SlotMap<Module>,
    pub(crate) instances: SlotMap<Instance>,
    pub(crate) script_values: SlotMap<ScriptValue>,
    pub(crate) strings: HashMap<Handle, String>,
    pub(crate) next_string_id: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runtimes: SlotMap::new(1),
            modules: SlotMap::new(2),
            instances: SlotMap::new(3),
            script_values: SlotMap::new(0),
            strings: HashMap::new(),
            next_string_id: 1,
        }
    }
}

pub(crate) fn insert_string(value: String) -> Result<Handle, i32> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = state.next_string_id;
        if id == 0 || id >= (1 << 30) {
            return Err(fail(
                RILS_STATUS_UNSUPPORTED_VALUE,
                "thread-local string handle space is exhausted",
                "",
                Span::default(),
            ));
        }
        state.next_string_id += 1;
        let thread = THREAD_ID.with(|thread| *thread) as u64;
        // Generation zero is deliberately invalid for every ordinary SlotMap
        // handle, keeping transferred string handles in a disjoint namespace.
        let handle = (thread << 46) | u64::from(id);
        state.strings.insert(handle, value);
        Ok(handle)
    })
}

pub(crate) fn string_handle_is_current_thread(handle: Handle) -> bool {
    let low = (handle & 0x3fff_ffff) as u32;
    let generation = ((handle >> 30) & 0xffff) as u32;
    let thread = ((handle >> 46) & 0xffff) as u16;
    let kind = (handle >> 62) as u8;
    low != 0 && generation == 0 && kind == 0 && thread == THREAD_ID.with(|current| *current)
}

pub(crate) fn take_string(handle: Handle) -> Result<String, i32> {
    if !string_handle_is_current_thread(handle) {
        return Err(fail(
            RILS_STATUS_INVALID_HANDLE,
            "invalid or cross-thread string handle",
            "",
            Span::default(),
        ));
    }
    STATE.with(|state| {
        state.borrow_mut().strings.remove(&handle).ok_or_else(|| {
            fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid, consumed, or destroyed string handle",
                "",
                Span::default(),
            )
        })
    })
}

pub(crate) fn discard_ffi_string(value: RilsValue) {
    if value.tag == RILS_VALUE_STRING && string_handle_is_current_thread(value.low) {
        STATE.with(|state| {
            state.borrow_mut().strings.remove(&value.low);
        });
    }
}

pub(crate) struct FfiStringInputGuard<'a>(pub(crate) &'a [RilsValue]);

impl Drop for FfiStringInputGuard<'_> {
    fn drop(&mut self) {
        for value in self.0 {
            discard_ffi_string(*value);
        }
    }
}

pub(crate) struct FfiStringValueGuard(pub(crate) RilsValue);

impl Drop for FfiStringValueGuard {
    fn drop(&mut self) {
        discard_ffi_string(self.0);
    }
}

pub(crate) static NEXT_THREAD_ID: AtomicU16 = AtomicU16::new(1);

thread_local! {
    pub(crate) static THREAD_ID: u16 = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed).max(1);
    pub(crate) static STATE: RefCell<State> = RefCell::new(State::default());
    pub(crate) static LAST_ERROR: RefCell<LastError> = RefCell::new(LastError::default());
    pub(crate) static HOST_CALLBACK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn encode_handle(index: usize, generation: u32, kind: u8) -> Handle {
    let thread = THREAD_ID.with(|id| *id) as u64;
    ((kind as u64) << 62) | (thread << 46) | ((generation as u64) << 30) | (index as u64 + 1)
}

pub(crate) fn decode_handle(handle: Handle, expected_kind: u8) -> Option<(usize, u32)> {
    let low = (handle & 0x3fff_ffff) as u32;
    let generation = ((handle >> 30) & 0xffff) as u32;
    let thread = ((handle >> 46) & 0xffff) as u16;
    let kind = (handle >> 62) as u8;
    let current_thread = THREAD_ID.with(|id| *id);
    (low != 0 && generation != 0 && thread == current_thread && kind == expected_kind)
        .then_some(((low - 1) as usize, generation))
}

pub(crate) fn clear_error() {
    LAST_ERROR.with(|error| *error.borrow_mut() = LastError::default());
}

pub(crate) fn fail(code: i32, message: impl Into<String>, source_name: &str, span: Span) -> i32 {
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

pub(crate) fn status_entry(function: impl FnOnce() -> i32) -> i32 {
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

pub(crate) fn handle_entry(function: impl FnOnce() -> Handle) -> Handle {
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

pub(crate) struct HostCallbackGuard;

impl HostCallbackGuard {
    pub(crate) fn enter() -> Result<Self, String> {
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

pub(crate) fn clone_module(runtime: Handle, module: Handle) -> Result<Module, i32> {
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

pub(crate) fn runtime_host_snapshot(runtime: Handle) -> Result<(HostContract, BytecodeHost), i32> {
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

pub(crate) unsafe fn read_bytes<'a>(value: RilsSlice) -> Result<&'a [u8], i32> {
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

pub(crate) unsafe fn read_utf8(value: RilsSlice, label: &str) -> Result<&str, i32> {
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
