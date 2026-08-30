use super::*;

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
                script_values: Vec::new(),
                host_contract: HostContract::new(),
                host: BytecodeHost::standard(),
                allowed_capabilities: HashSet::new(),
                dispatcher: None,
                dispatcher_user_data: ptr::null_mut(),
                output_callback: None,
                output_user_data: ptr::null_mut(),
                host_value_formatter: None,
                host_value_formatter_user_data: ptr::null_mut(),
                host_frozen: false,
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
            for value in runtime_value.script_values {
                state.script_values.remove(value);
            }
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
pub extern "C" fn rils_runtime_set_output_callback(
    runtime: Handle,
    callback: Option<RilsOutputCallback>,
    user_data: *mut c_void,
) -> i32 {
    status_entry(|| {
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
            let result = configure_output_handler(&mut runtime.host, callback, user_data);
            match result {
                Ok(()) => {
                    runtime.allowed_capabilities.insert("std::io".to_string());
                    runtime.output_callback = callback;
                    runtime.output_user_data = user_data;
                    RILS_STATUS_OK
                }
                Err(message) => fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default()),
            }
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_set_host_value_formatter(
    runtime: Handle,
    callback: Option<RilsHostValueFormatCallback>,
    user_data: *mut c_void,
) -> i32 {
    status_entry(|| {
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
            configure_host_value_formatter(&mut runtime.host, callback, user_data);
            runtime.host_value_formatter = callback;
            runtime.host_value_formatter_user_data = user_data;
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_set_host_dispatcher(
    runtime: Handle,
    dispatcher: Option<RilsHostDispatcher>,
    user_data: *mut c_void,
) -> i32 {
    status_entry(|| {
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
            if runtime.host_frozen || !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host dispatcher cannot change after freeze or module creation",
                    "",
                    Span::default(),
                );
            }
            runtime.dispatcher = dispatcher;
            runtime.dispatcher_user_data = user_data;
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Grants one capability to bytecode executed by `runtime`. The name is copied.
///
/// # Safety
///
/// A non-empty capability slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_allow_capability(
    runtime: Handle,
    capability: RilsSlice,
) -> i32 {
    status_entry(|| {
        // SAFETY: The caller promises a readable call-scoped slice.
        let capability = match unsafe { read_utf8(capability, "host capability") } {
            Ok(value) if !value.is_empty() => value.to_owned(),
            Ok(_) => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host capability cannot be empty",
                    "",
                    Span::default(),
                );
            }
            Err(status) => return status,
        };
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
            if runtime.host_frozen || !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host capabilities cannot change after freeze or module creation",
                    "",
                    Span::default(),
                );
            }
            runtime.allowed_capabilities.insert(capability);
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_allow_standard_library(runtime: Handle) -> i32 {
    status_entry(|| {
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
            if runtime.host_frozen || !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "standard-library capabilities cannot change after freeze or module creation",
                    "",
                    Span::default(),
                );
            }
            if let Err(message) = runtime.host.enable_standard_library() {
                return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
            }
            runtime.allowed_capabilities.extend(
                BytecodeHost::standard_library_capabilities()
                    .into_iter()
                    .map(str::to_owned),
            );
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_runtime_freeze_host_registry(runtime: Handle) -> i32 {
    status_entry(|| {
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
            if runtime.host_frozen {
                return RILS_STATUS_OK;
            }
            if !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host registry must be frozen before module creation",
                    "",
                    Span::default(),
                );
            }
            let host = match build_runtime_host(runtime) {
                Ok(host) => host,
                Err(message) => {
                    return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
                }
            };
            runtime.host = host;
            runtime.host_frozen = true;
            RILS_STATUS_OK
        })
    })
}
