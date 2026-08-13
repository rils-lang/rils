use super::*;

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
/// Executes the compiled module entry point for an instance.
///
/// # Safety
///
/// `out_value` must point to writable storage for one value.
#[unsafe(no_mangle)]
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
            Some((runtime_value.max_steps, runtime_value.host.clone(), module))
        });
        let Some((max_steps, host, module)) = resolved else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime or instance handle",
                "",
                Span::default(),
            );
        };
        let value = match module
            .bytecode
            .execute_with_host_and_limit(&host, max_steps)
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
/// Calls an exported script function with scalar arguments.
///
/// # Safety
///
/// Non-empty slices must be readable for the duration of the call, and `out_value` must point
/// to writable storage for one value.
#[unsafe(no_mangle)]
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
            Some((runtime_value.max_steps, runtime_value.host.clone(), module))
        });
        let Some((max_steps, host, module)) = resolved else {
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
            &host,
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
