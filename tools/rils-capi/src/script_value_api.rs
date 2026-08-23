use super::*;

#[unsafe(no_mangle)]
/// Constructs a persistent script value through the target type's `Default` implementation.
///
/// # Safety
///
/// `target` must reference readable memory for its declared length, and `out_value` must point to
/// writable storage for one Rils handle.
pub unsafe extern "C" fn rils_script_value_create_default(
    runtime: Handle,
    instance: Handle,
    target: RilsSlice,
    out_value: *mut Handle,
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
        let target = match unsafe { read_utf8(target, "target type") } {
            Ok("") => {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "target type must not be empty",
                    "",
                    Span::default(),
                );
            }
            Ok(value) => value.to_owned(),
            Err(status) => return status,
        };
        let resolved = STATE.with(|state| {
            let state = state.borrow();
            let instance_value = state.instances.get(instance)?.clone();
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
            .construct_default_with_host_and_limit(&target, &host, max_steps)
        {
            Ok(value) => value,
            Err(error) => {
                let source_name = module_source_name(&module, error.span).to_owned();
                return fail(
                    RILS_STATUS_EXECUTION_ERROR,
                    error.message,
                    &source_name,
                    error.span,
                );
            }
        };
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state
                .instances
                .get(instance)
                .is_none_or(|value| value.runtime != runtime)
            {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "instance was destroyed while constructing its default value",
                    &module.source_name,
                    Span::default(),
                );
            }
            let handle = state.script_values.insert(ScriptValue {
                runtime,
                instance,
                target,
                value,
            });
            state
                .instances
                .get_mut(instance)
                .expect("instance was checked")
                .script_values
                .push(handle);
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .script_values
                .push(handle);
            unsafe { out_value.write(handle) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_script_value_destroy(runtime: Handle, value: Handle) -> i32 {
    status_entry(|| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(script_value) = state.script_values.get(value).cloned() else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid script value handle",
                    "",
                    Span::default(),
                );
            };
            if script_value.runtime != runtime || state.runtimes.get(runtime).is_none() {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid runtime or script value handle",
                    "",
                    Span::default(),
                );
            }
            state.script_values.remove(value);
            if let Some(instance) = state.instances.get_mut(script_value.instance) {
                instance.script_values.retain(|handle| *handle != value);
            }
            state
                .runtimes
                .get_mut(runtime)
                .expect("runtime was checked")
                .script_values
                .retain(|handle| *handle != value);
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Invokes a trait method on a persistent script value.
///
/// # Safety
///
/// The name slices and every logical-type slice must reference readable memory for their declared
/// lengths. When `argument_count` is non-zero, `arguments` and `argument_types` must each reference
/// that many initialized records. `out_value` must point to writable storage for one `RilsValue`.
pub unsafe extern "C" fn rils_script_value_call_trait(
    runtime: Handle,
    instance: Handle,
    value: Handle,
    trait_name: RilsSlice,
    method_name: RilsSlice,
    arguments: *const RilsValue,
    argument_types: *const RilsHostParameter,
    argument_count: usize,
    out_value: *mut RilsValue,
) -> i32 {
    status_entry(|| {
        if out_value.is_null()
            || (argument_count != 0 && (arguments.is_null() || argument_types.is_null()))
        {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "invalid argument or output pointer",
                "",
                Span::default(),
            );
        }
        let trait_name = match unsafe { read_utf8(trait_name, "trait name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let method_name = match unsafe { read_utf8(method_name, "method name") } {
            Ok(value) => value,
            Err(status) => return status,
        };
        let raw_arguments = if argument_count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(arguments, argument_count) }
        };
        let argument_types = if argument_count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(argument_types, argument_count) }
        };
        let resolved = STATE.with(|state| {
            let state = state.borrow();
            let instance_value = state.instances.get(instance)?.clone();
            let script_value = state.script_values.get(value)?.clone();
            if instance_value.runtime != runtime
                || script_value.runtime != runtime
                || script_value.instance != instance
            {
                return None;
            }
            let runtime_value = state.runtimes.get(runtime)?;
            let module = state.modules.get(instance_value.module)?.clone();
            Some((
                runtime_value.max_steps,
                runtime_value.host.clone(),
                runtime_value.host_contract.clone(),
                module,
                script_value,
            ))
        });
        let Some((max_steps, host, host_contract, module, mut script_value)) = resolved else {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "invalid runtime, instance, or script value handle",
                "",
                Span::default(),
            );
        };
        let mut arguments = Vec::with_capacity(argument_count);
        for (value, parameter) in raw_arguments.iter().zip(argument_types) {
            if parameter.reserved != 0 || value.tag != parameter.transport_tag {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "trait argument transport metadata is invalid",
                    &module.source_name,
                    Span::default(),
                );
            }
            let logical_type = match unsafe { read_utf8(parameter.logical_type, "logical type") } {
                Ok(value) => value,
                Err(status) => return status,
            };
            let logical_type = match logical_type_from_transport(
                &host_contract,
                parameter.transport_tag,
                logical_type,
                false,
            ) {
                Ok(value) => value,
                Err(message) => {
                    return fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        message,
                        &module.source_name,
                        Span::default(),
                    );
                }
            };
            let logical_host_type = match logical_type {
                Type::Named { name, arguments }
                    if arguments.is_empty() && host_contract.host_type(&name).is_some() =>
                {
                    match logical_host_type(&host_contract, &name) {
                        Ok(logical) => Some(logical),
                        Err(message) => {
                            return fail(
                                RILS_STATUS_INVALID_ARGUMENT,
                                message,
                                &module.source_name,
                                Span::default(),
                            );
                        }
                    }
                }
                _ => None,
            };
            match from_ffi_value(*value, logical_host_type.as_ref()) {
                Ok(value) => arguments.push(value),
                Err(status) => return status,
            }
        }
        let result = match module.bytecode.call_trait_method_with_host_and_limit(
            &script_value.target,
            trait_name,
            method_name,
            &mut script_value.value,
            arguments,
            &host,
            max_steps,
        ) {
            Ok(result) => result,
            Err(error) => {
                let source_name = module_source_name(&module, error.span).to_owned();
                return fail(
                    RILS_STATUS_EXECUTION_ERROR,
                    error.message,
                    &source_name,
                    error.span,
                );
            }
        };
        let updated = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(stored) = state.script_values.get_mut(value) else {
                return false;
            };
            if stored.runtime != runtime || stored.instance != instance {
                return false;
            }
            stored.value = script_value.value;
            true
        });
        if !updated {
            return fail(
                RILS_STATUS_INVALID_HANDLE,
                "script value was destroyed during its trait call",
                &module.source_name,
                Span::default(),
            );
        }
        let result = match to_ffi_value(result, &module.source_name) {
            Ok(result) => result,
            Err(status) => return status,
        };
        unsafe { out_value.write(result) };
        RILS_STATUS_OK
    })
}
