use super::*;

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
        let (contract, host) = match runtime_host_snapshot(runtime) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let bytecode = match rils::compile_with_host(source, &contract) {
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
        if let Err(error) = bytecode.validate_host(&host) {
            return fail(
                RILS_STATUS_BYTECODE_ERROR,
                error.message,
                source_name,
                error.span,
            );
        }
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
        let (contract, host) = match runtime_host_snapshot(runtime) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let bytecode = match rils::compile_file_with_host(path, &contract) {
            Ok(module) => module,
            Err(error) => {
                let source_name = error.source_name().unwrap_or(path).to_owned();
                return fail(
                    RILS_STATUS_COMPILE_ERROR,
                    error.message,
                    &source_name,
                    error.span,
                );
            }
        };
        if let Err(error) = bytecode.validate_host(&host) {
            let source_name = bytecode
                .source_name(error.span.source)
                .unwrap_or(path)
                .to_owned();
            return fail(
                RILS_STATUS_BYTECODE_ERROR,
                error.message,
                &source_name,
                error.span,
            );
        }
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
        let (_, host) = match runtime_host_snapshot(runtime) {
            Ok(value) => value,
            Err(status) => return status,
        };
        if let Err(error) = bytecode.validate_host(&host) {
            let source_name = bytecode
                .source_name(error.span.source)
                .unwrap_or("<bytecode>")
                .to_owned();
            return fail(
                RILS_STATUS_BYTECODE_ERROR,
                error.message,
                &source_name,
                error.span,
            );
        }
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
        let (_, host) = match runtime_host_snapshot(runtime) {
            Ok(value) => value,
            Err(status) => return status,
        };
        if let Err(error) = bytecode.validate_host(&host) {
            let source_name = bytecode
                .source_name(error.span.source)
                .unwrap_or(path)
                .to_owned();
            return fail(
                RILS_STATUS_BYTECODE_ERROR,
                error.message,
                &source_name,
                error.span,
            );
        }
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
pub extern "C" fn rils_module_validate_host(runtime: Handle, module: Handle) -> i32 {
    status_entry(|| {
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let (_, host) = match runtime_host_snapshot(runtime) {
            Ok(value) => value,
            Err(status) => return status,
        };
        match module.bytecode.validate_host(&host) {
            Ok(()) => RILS_STATUS_OK,
            Err(error) => {
                let source_name = module_source_name(&module, error.span).to_owned();
                fail(
                    RILS_STATUS_BYTECODE_ERROR,
                    error.message,
                    &source_name,
                    error.span,
                )
            }
        }
    })
}

fn trait_implementation_targets(
    module: &Module,
    trait_name: &str,
    source_name: Option<&str>,
) -> Vec<String> {
    let mut targets = module
        .bytecode
        .trait_implementations(trait_name)
        .filter(|implementation| {
            source_name.is_none_or(|source_name| {
                module.bytecode.source_name(implementation.source()) == Some(source_name)
            })
        })
        .map(|implementation| implementation.target().to_owned())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    targets
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rils_module_trait_implementation_count(
    runtime: Handle,
    module: Handle,
    trait_name: RilsSlice,
    source_name: RilsSlice,
    out_count: *mut usize,
) -> i32 {
    status_entry(|| {
        if out_count.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "out_count is null",
                "",
                Span::default(),
            );
        }
        let trait_name = match unsafe { read_utf8(trait_name, "trait name") } {
            Ok("") => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "trait name must not be empty",
                    "",
                    Span::default(),
                );
            }
            Ok(value) => value,
            Err(status) => return status,
        };
        let source_name = match unsafe { read_utf8(source_name, "source name") } {
            Ok("") => None,
            Ok(value) => Some(value),
            Err(status) => return status,
        };
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let count = trait_implementation_targets(&module, trait_name, source_name).len();
        unsafe { out_count.write(count) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rils_module_trait_implementation_name_size(
    runtime: Handle,
    module: Handle,
    trait_name: RilsSlice,
    source_name: RilsSlice,
    index: usize,
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
        let trait_name = match unsafe { read_utf8(trait_name, "trait name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let source_name = match unsafe { read_utf8(source_name, "source name") } {
            Ok("") => None,
            Ok(value) => Some(value),
            Err(status) => return status,
        };
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let targets = trait_implementation_targets(&module, trait_name, source_name);
        let Some(target) = targets.get(index) else {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "trait implementation index is out of bounds",
                &module.source_name,
                Span::default(),
            );
        };
        unsafe { out_size.write(target.len()) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rils_module_write_trait_implementation_name(
    runtime: Handle,
    module: Handle,
    trait_name: RilsSlice,
    source_name: RilsSlice,
    index: usize,
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
        unsafe { out_written.write(0) };
        let trait_name = match unsafe { read_utf8(trait_name, "trait name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let source_name = match unsafe { read_utf8(source_name, "source name") } {
            Ok("") => None,
            Ok(value) => Some(value),
            Err(status) => return status,
        };
        let module = match clone_module(runtime, module) {
            Ok(module) => module,
            Err(status) => return status,
        };
        let targets = trait_implementation_targets(&module, trait_name, source_name);
        let Some(target) = targets.get(index) else {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "trait implementation index is out of bounds",
                &module.source_name,
                Span::default(),
            );
        };
        unsafe { out_written.write(target.len()) };
        if buffer_capacity < target.len() || (!target.is_empty() && buffer.is_null()) {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "trait implementation name buffer is too small or null",
                &module.source_name,
                Span::default(),
            );
        }
        unsafe { ptr::copy_nonoverlapping(target.as_ptr(), buffer, target.len()) };
        RILS_STATUS_OK
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
            let mut removed_values = Vec::new();
            for handle in children {
                if state
                    .instances
                    .get(handle)
                    .is_some_and(|instance| instance.module == module)
                {
                    if let Some(instance) = state.instances.remove(handle) {
                        removed_values.extend(instance.script_values);
                    }
                }
            }
            for value in removed_values {
                state.script_values.remove(value);
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
            let surviving_script_values = state
                .runtimes
                .get(runtime)
                .expect("runtime was checked")
                .script_values
                .iter()
                .copied()
                .filter(|handle| state.script_values.get(*handle).is_some())
                .collect();
            let runtime = state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked");
            runtime.modules.retain(|handle| *handle != module);
            runtime.instances = surviving_instances;
            runtime.script_values = surviving_script_values;
            RILS_STATUS_OK
        })
    })
}
