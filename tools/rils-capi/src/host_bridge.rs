use super::*;

pub(crate) struct HostDispatcherInvocation<'a> {
    pub(crate) dispatcher: RilsHostDispatcher,
    pub(crate) user_data: *mut c_void,
    pub(crate) function_id: u64,
    pub(crate) function_name: &'a str,
    pub(crate) signature: &'a FunctionSignature,
    pub(crate) contract: &'a HostContract,
    pub(crate) logical_return_type: Option<&'a LogicalHostType>,
}

pub(crate) fn invoke_host_dispatcher(
    invocation: HostDispatcherInvocation<'_>,
    arguments: &[Value],
) -> Result<Value, String> {
    let HostDispatcherInvocation {
        dispatcher,
        user_data,
        function_id,
        function_name,
        signature,
        contract,
        logical_return_type,
    } = invocation;
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
    let mut encoded = Vec::with_capacity(arguments.len());
    for (value, parameter) in arguments.iter().zip(parameters) {
        match to_ffi_host_argument(value, parameter, contract) {
            Ok(value) => encoded.push(value),
            Err(_) => {
                for value in encoded {
                    discard_ffi_string(value);
                }
                return Err(current_error_message());
            }
        }
    }
    let _encoded_string_guard = FfiStringInputGuard(&encoded);
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
        let _result_string_guard = FfiStringValueGuard(result);
        return Err(format!(
            "host function `{function_name}` failed with status {status}: {}",
            copy_callback_error(error)
        ));
    }
    let _result_string_guard = FfiStringValueGuard(result);
    let result = if let Type::Named { name, arguments } = &signature.return_type
        && arguments.is_empty()
        && let Some(definition) = contract
            .host_type(name)
            .and_then(|declaration| declaration.enum_definition.as_ref())
    {
        from_ffi_host_enum(result, name, definition).map_err(|_| current_error_message())?
    } else {
        from_ffi_value(result, logical_return_type).map_err(|_| current_error_message())?
    };
    signature.return_type.constrain(&result).ok_or_else(|| {
        format!(
            "host function `{function_name}` returned `{}`, expected `{}`",
            result.type_name(),
            signature.return_type
        )
    })
}

pub(crate) fn build_runtime_host(runtime: &Runtime) -> Result<BytecodeHost, String> {
    let mut host = BytecodeHost::standard();
    for capability in &runtime.allowed_capabilities {
        if BytecodeHost::standard_library_capabilities().contains(&capability.as_str()) {
            host.enable_standard_library_capability(capability)?;
        } else {
            host.allow_capability(capability.clone());
        }
    }
    if runtime.allowed_capabilities.contains("std::io") {
        configure_output_handler(&mut host, runtime.output_callback, runtime.output_user_data)?;
    }
    configure_host_value_formatter(
        &mut host,
        runtime.host_value_formatter,
        runtime.host_value_formatter_user_data,
    );
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
        let callback_contract = runtime.host_contract.clone();
        let logical_return_type = match &signature.return_type {
            Type::Named { name, arguments }
                if arguments.is_empty()
                    && runtime
                        .host_contract
                        .host_type(name)
                        .is_some_and(|declaration| declaration.enum_definition.is_none()) =>
            {
                Some(logical_host_type(&runtime.host_contract, name)?)
            }
            _ => None,
        };
        host.register_function(
            function_name,
            signature,
            function.capability.clone(),
            move |arguments| {
                invoke_host_dispatcher(
                    HostDispatcherInvocation {
                        dispatcher,
                        user_data,
                        function_id,
                        function_name: &callback_name,
                        signature: &callback_signature,
                        contract: &callback_contract,
                        logical_return_type: logical_return_type.as_ref(),
                    },
                    arguments,
                )
            },
        )?;
    }
    Ok(host)
}

pub(crate) fn configure_output_handler(
    host: &mut BytecodeHost,
    callback: Option<RilsOutputCallback>,
    user_data: *mut c_void,
) -> Result<(), String> {
    if let Some(callback) = callback {
        let user_data = user_data as usize;
        host.set_output_handler(move |text, newline| {
            let slice = RilsSlice {
                data: text.as_ptr(),
                length: text.len(),
            };
            // SAFETY: The callback and user data remain owned by the embedding runtime;
            // the UTF-8 slice is readable only for this synchronous callback.
            unsafe { callback(user_data as *mut c_void, slice, u32::from(newline)) };
            Ok(())
        })
    } else {
        host.enable_standard_io()
    }
}

pub(crate) fn configure_host_value_formatter(
    host: &mut BytecodeHost,
    callback: Option<RilsHostValueFormatCallback>,
    user_data: *mut c_void,
) {
    let Some(callback) = callback else {
        host.reset_host_value_formatter();
        return;
    };
    let user_data = user_data as usize;
    host.set_host_value_formatter(move |value, spec| {
        let Value::HostObject(object) = value else {
            return Ok(None);
        };
        let native = if let Some(handle) = rils::opaque_host_handle(value) {
            RilsValue {
                tag: RILS_VALUE_HOST_HANDLE,
                low: u64::from_le_bytes(handle.object_id.to_le_bytes()),
                high: (u64::from(handle.generation) << 32) | u64::from(handle.type_id),
                ..RilsValue::default()
            }
        } else if let Some(inline) = rils::inline_host_value(value) {
            RilsValue {
                tag: RILS_VALUE_INLINE_VALUE,
                low: u64::from_le_bytes(inline.bytes[..8].try_into().expect("fixed payload")),
                high: u64::from_le_bytes(inline.bytes[8..].try_into().expect("fixed payload")),
                ..RilsValue::default()
            }
        } else {
            return Ok(None);
        };
        let logical_type = RilsSlice {
            data: object.type_definition.name.as_ptr(),
            length: object.type_definition.name.len(),
        };
        let kind = match spec.kind {
            rils::HostFormatKind::Display => 0,
            rils::HostFormatKind::Debug => 1,
        };
        let precision = spec.precision.unwrap_or(usize::MAX);
        // SAFETY: The embedding host owns the callback and user data. All inputs are call-scoped.
        let required = unsafe {
            callback(
                user_data as *mut c_void,
                logical_type,
                native,
                kind,
                u32::from(spec.alternate),
                precision,
                ptr::null_mut(),
                0,
            )
        };
        if required == usize::MAX {
            return Ok(None);
        }
        let mut buffer = vec![0u8; required];
        // SAFETY: `buffer` is writable for its reported capacity during this synchronous call.
        let written = unsafe {
            callback(
                user_data as *mut c_void,
                logical_type,
                native,
                kind,
                u32::from(spec.alternate),
                precision,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };
        if written == usize::MAX {
            return Ok(None);
        }
        if written > buffer.len() {
            return Err(
                "host value formatter output changed between buffer query and write".into(),
            );
        }
        buffer.truncate(written);
        String::from_utf8(buffer)
            .map(Some)
            .map_err(|_| "host value formatter returned invalid UTF-8".into())
    });
}
