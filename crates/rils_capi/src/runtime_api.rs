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
/// Registers a batch of host function declarations. Input data is copied.
///
/// # Safety
///
/// `functions` and every non-empty nested slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_functions(
    runtime: Handle,
    functions: *const RilsHostFunction,
    function_count: usize,
) -> i32 {
    status_entry(|| {
        if function_count != 0 && functions.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host function array is null",
                "",
                Span::default(),
            );
        }
        let descriptors = if function_count == 0 {
            &[]
        } else {
            // SAFETY: The caller promises a readable array for this call.
            unsafe { slice::from_raw_parts(functions, function_count) }
        };
        let mut declarations = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            if descriptor.reserved > 3 || descriptor.function_id == 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host function receiver kind is invalid or function id is zero",
                    "",
                    Span::default(),
                );
            }
            // SAFETY: Nested slices follow the same call-scoped input contract.
            let name = match unsafe { read_utf8(descriptor.name, "host function name") } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            // SAFETY: Nested slices follow the same call-scoped input contract.
            let capability =
                match unsafe { read_utf8(descriptor.capability, "host function capability") } {
                    Ok(value) => value.to_owned(),
                    Err(status) => return status,
                };
            if descriptor.parameter_count != 0 && descriptor.parameter_tags.is_null() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host function parameter tag array is null",
                    "",
                    Span::default(),
                );
            }
            let tags = if descriptor.parameter_count == 0 {
                &[]
            } else {
                // SAFETY: The caller promises a readable tag array for this call.
                unsafe {
                    slice::from_raw_parts(descriptor.parameter_tags, descriptor.parameter_count)
                }
            };
            let parameters = match tags
                .iter()
                .map(|tag| portable_type_from_tag(*tag, false))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(value) => value,
                Err(message) => {
                    return fail(RILS_STATUS_UNSUPPORTED_VALUE, message, "", Span::default());
                }
            };
            let return_type = match portable_type_from_tag(descriptor.return_tag, true) {
                Ok(value) => value,
                Err(message) => {
                    return fail(RILS_STATUS_UNSUPPORTED_VALUE, message, "", Span::default());
                }
            };
            declarations.push((
                descriptor.function_id,
                name,
                FunctionSignature::fixed(parameters, return_type),
                capability,
                HostReceiver::from_tag(descriptor.reserved as u8)
                    .expect("receiver kind was validated above"),
            ));
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
            if runtime.host_frozen || !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host registry cannot change after freeze or module creation",
                    "",
                    Span::default(),
                );
            }
            let mut contract = runtime.host_contract.clone();
            for (function_id, name, signature, capability, receiver) in declarations {
                if let Err(message) = contract.register_function_with_options_and_receiver(
                    function_id,
                    name,
                    signature,
                    capability,
                    HostCallKind::Direct,
                    HostThreadAffinity::MainThread,
                    receiver,
                ) {
                    return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
                }
            }
            runtime.host_contract = contract;
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Registers a versioned binary host manifest fragment. Repeated calls merge
/// compatible fragments deterministically. Input data is copied.
///
/// # Safety
///
/// A non-empty manifest slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_manifest(
    runtime: Handle,
    manifest: RilsSlice,
) -> i32 {
    status_entry(|| {
        // SAFETY: The caller promises a readable call-scoped byte slice.
        let manifest = match unsafe { read_bytes(manifest) } {
            Ok(value) if !value.is_empty() => value,
            Ok(_) => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host manifest cannot be empty",
                    "",
                    Span::default(),
                );
            }
            Err(status) => return status,
        };
        let contract = match HostContract::from_manifest_bytes(manifest) {
            Ok(contract) => contract,
            Err(message) => {
                return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
            }
        };
        if let Err(message) = validate_c_dispatcher_contract(&contract) {
            return fail(RILS_STATUS_UNSUPPORTED_VALUE, message, "", Span::default());
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
            if runtime.host_frozen || !runtime.modules.is_empty() || !runtime.instances.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host manifest cannot change after freeze or module creation",
                    "",
                    Span::default(),
                );
            }
            if runtime.host_contract.is_empty() {
                runtime.host_contract = contract;
            } else if let Err(message) = runtime.host_contract.merge(&contract) {
                return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
            }
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Returns the canonical binary host manifest size for `runtime`.
///
/// # Safety
///
/// `out_size` must point to writable storage for one `size_t` value.
pub unsafe extern "C" fn rils_runtime_host_manifest_size(
    runtime: Handle,
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
        let manifest = STATE.with(|state| {
            state
                .borrow()
                .runtimes
                .get(runtime)
                .map(|runtime| runtime.host_contract.to_manifest_bytes())
        });
        let Some(manifest) = manifest else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime handle",
                "",
                Span::default(),
            );
        };
        let manifest = match manifest {
            Ok(manifest) => manifest,
            Err(message) => {
                return fail(RILS_STATUS_PANIC, message, "", Span::default());
            }
        };
        // SAFETY: The pointer was checked and the caller promises writable storage.
        unsafe { out_size.write(manifest.len()) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Writes the canonical binary host manifest into caller-owned memory.
///
/// # Safety
///
/// `out_written` must be writable. A non-empty buffer must be writable for
/// `buffer_capacity` bytes.
pub unsafe extern "C" fn rils_runtime_write_host_manifest(
    runtime: Handle,
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
        let manifest = STATE.with(|state| {
            state
                .borrow()
                .runtimes
                .get(runtime)
                .map(|runtime| runtime.host_contract.to_manifest_bytes())
        });
        let Some(manifest) = manifest else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime handle",
                "",
                Span::default(),
            );
        };
        let manifest = match manifest {
            Ok(manifest) => manifest,
            Err(message) => {
                return fail(RILS_STATUS_PANIC, message, "", Span::default());
            }
        };
        // SAFETY: The pointer was checked and the caller promises writable storage.
        unsafe { out_written.write(manifest.len()) };
        if buffer_capacity < manifest.len() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                format!(
                    "host manifest buffer is too small: requires {}, received {buffer_capacity}",
                    manifest.len()
                ),
                "",
                Span::default(),
            );
        }
        if !manifest.is_empty() && buffer.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host manifest buffer is null",
                "",
                Span::default(),
            );
        }
        // SAFETY: The destination is writable for at least `manifest.len()` bytes and cannot
        // overlap the Rust-owned source buffer.
        unsafe { ptr::copy_nonoverlapping(manifest.as_ptr(), buffer, manifest.len()) };
        RILS_STATUS_OK
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
