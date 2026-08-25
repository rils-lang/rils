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
/// Registers nominal host object types before v2 function declarations.
///
/// # Safety
///
/// `types` and every non-empty nested slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_types(
    runtime: Handle,
    types: *const RilsHostType,
    type_count: usize,
) -> i32 {
    status_entry(|| {
        if type_count != 0 && types.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host type array is null",
                "",
                Span::default(),
            );
        }
        let descriptors = if type_count == 0 {
            &[]
        } else {
            // SAFETY: The caller promises a readable array for this call.
            unsafe { slice::from_raw_parts(types, type_count) }
        };
        let mut declarations = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            if descriptor.reserved != 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host type reserved fields must be zero",
                    "",
                    Span::default(),
                );
            }
            let name = match unsafe { read_utf8(descriptor.name, "host type name") } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            let base_type = match unsafe { read_utf8(descriptor.base_type, "host base type") } {
                Ok("") => None,
                Ok(value) => Some(value.to_owned()),
                Err(status) => return status,
            };
            let transport = match descriptor.transport_tag {
                RILS_VALUE_HOST_HANDLE => HostTypeTransport::HostHandle,
                value => {
                    return fail(
                        RILS_STATUS_UNSUPPORTED_VALUE,
                        format!("value tag {value} is not a supported host type transport"),
                        "",
                        Span::default(),
                    );
                }
            };
            declarations.push((name, base_type, transport, None));
        }
        register_host_type_declarations(runtime, declarations)
    })
}

#[unsafe(no_mangle)]
/// Registers opaque and inline value host types before v2 function declarations.
///
/// # Safety
///
/// `types` and every non-empty nested slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_types_v2(
    runtime: Handle,
    types: *const RilsHostTypeV2,
    type_count: usize,
) -> i32 {
    status_entry(|| {
        if type_count != 0 && types.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host type v2 array is null",
                "",
                Span::default(),
            );
        }
        let descriptors = if type_count == 0 {
            &[]
        } else {
            // SAFETY: The caller promises a readable array for this call.
            unsafe { slice::from_raw_parts(types, type_count) }
        };
        let mut declarations = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            if descriptor.reserved != 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host type v2 reserved fields must be zero",
                    "",
                    Span::default(),
                );
            }
            let name = match unsafe { read_utf8(descriptor.name, "host type name") } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            let base_type = match unsafe { read_utf8(descriptor.base_type, "host base type") } {
                Ok("") => None,
                Ok(value) => Some(value.to_owned()),
                Err(status) => return status,
            };
            let layout = match unsafe { read_utf8(descriptor.value_layout, "host value layout") } {
                Ok("") => None,
                Ok(value) => match HostValueLayout::parse(value) {
                    Ok(layout) => Some(layout),
                    Err(message) => {
                        return fail(RILS_STATUS_UNSUPPORTED_VALUE, message, "", Span::default());
                    }
                },
                Err(status) => return status,
            };
            let transport = match descriptor.transport_tag {
                RILS_VALUE_HOST_HANDLE => HostTypeTransport::HostHandle,
                RILS_VALUE_INLINE_VALUE => HostTypeTransport::InlineValue,
                value => {
                    return fail(
                        RILS_STATUS_UNSUPPORTED_VALUE,
                        format!("value tag {value} is not a supported host type transport"),
                        "",
                        Span::default(),
                    );
                }
            };
            let valid_kind = match descriptor.kind {
                RILS_HOST_TYPE_OPAQUE => {
                    transport == HostTypeTransport::HostHandle && layout.is_none()
                }
                RILS_HOST_TYPE_VALUE => {
                    transport == HostTypeTransport::InlineValue
                        && layout.is_some()
                        && base_type.is_none()
                }
                _ => false,
            };
            if !valid_kind {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host type v2 kind, transport, base type, and layout are inconsistent",
                    "",
                    Span::default(),
                );
            }
            declarations.push((name, base_type, transport, layout));
        }
        register_host_type_declarations(runtime, declarations)
    })
}

#[unsafe(no_mangle)]
/// Registers opaque, inline value, and enum host types.
///
/// # Safety
///
/// `types` and every non-empty nested slice/array must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_types_v3(
    runtime: Handle,
    types: *const RilsHostTypeV3,
    type_count: usize,
) -> i32 {
    status_entry(|| {
        if type_count != 0 && types.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host type v3 array is null",
                "",
                Span::default(),
            );
        }
        let descriptors = if type_count == 0 {
            &[]
        } else {
            // SAFETY: The caller promises a readable array for this call.
            unsafe { slice::from_raw_parts(types, type_count) }
        };
        for descriptor in descriptors {
            if descriptor.reserved != 0 || descriptor.enum_flags & !1 != 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host type v3 reserved fields or enum flags are invalid",
                    "",
                    Span::default(),
                );
            }
            let name = match unsafe { read_utf8(descriptor.name, "host type name") } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            if descriptor.kind != RILS_HOST_TYPE_ENUM {
                if descriptor.enum_variant_count != 0 || descriptor.enum_flags != 0 {
                    return fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "non-enum host type v3 declarations cannot contain enum metadata",
                        "",
                        Span::default(),
                    );
                }
                let v2 = RilsHostTypeV2 {
                    name: descriptor.name,
                    base_type: descriptor.base_type,
                    value_layout: descriptor.value_layout,
                    transport_tag: descriptor.transport_tag,
                    kind: descriptor.kind,
                    reserved: 0,
                };
                // SAFETY: `v2` and all referenced slices remain valid for this call.
                let status = unsafe { rils_runtime_register_host_types_v2(runtime, &v2, 1) };
                if status != RILS_STATUS_OK {
                    return status;
                }
                continue;
            }
            let base = match unsafe { read_utf8(descriptor.base_type, "host enum base type") } {
                Ok(value) => value,
                Err(status) => return status,
            };
            let layout = match unsafe { read_utf8(descriptor.value_layout, "host enum layout") } {
                Ok(value) => value,
                Err(status) => return status,
            };
            if !base.is_empty() || !layout.is_empty() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host enum type cannot declare a base type or inline layout",
                    "",
                    Span::default(),
                );
            }
            let underlying_type = match portable_type_from_tag(descriptor.transport_tag, false) {
                Ok(Type::Integer(integer))
                    if !matches!(integer, IntegerType::Isize | IntegerType::Usize) =>
                {
                    integer
                }
                _ => {
                    return fail(
                        RILS_STATUS_UNSUPPORTED_VALUE,
                        "host enum transport must use a fixed-width integer value tag",
                        "",
                        Span::default(),
                    );
                }
            };
            if descriptor.enum_variant_count != 0 && descriptor.enum_variants.is_null() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host enum variant array is null",
                    "",
                    Span::default(),
                );
            }
            let variants = if descriptor.enum_variant_count == 0 {
                &[]
            } else {
                // SAFETY: The caller promises a readable variant array for this call.
                unsafe {
                    slice::from_raw_parts(descriptor.enum_variants, descriptor.enum_variant_count)
                }
            };
            let mut declarations = Vec::with_capacity(variants.len());
            for variant in variants {
                let variant_name = match unsafe { read_utf8(variant.name, "host enum variant") } {
                    Ok(value) => value.to_owned(),
                    Err(status) => return status,
                };
                declarations.push((
                    variant_name,
                    u128::from(variant.raw_low) | (u128::from(variant.raw_high) << 64),
                ));
            }
            let status = register_host_enum_declaration(
                runtime,
                name,
                underlying_type,
                descriptor.enum_flags & 1 != 0,
                declarations,
            );
            if status != RILS_STATUS_OK {
                return status;
            }
        }
        RILS_STATUS_OK
    })
}

fn register_host_enum_declaration(
    runtime_handle: Handle,
    name: String,
    underlying_type: IntegerType,
    flags: bool,
    variants: Vec<(String, u128)>,
) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(runtime) = state.runtimes.get_mut(runtime_handle) else {
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
        if let Err(message) = contract.register_enum_type(name, underlying_type, flags, variants) {
            return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
        }
        runtime.host_contract = contract;
        RILS_STATUS_OK
    })
}

fn register_host_type_declarations(
    runtime_handle: Handle,
    declarations: Vec<(
        String,
        Option<String>,
        HostTypeTransport,
        Option<HostValueLayout>,
    )>,
) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(runtime) = state.runtimes.get_mut(runtime_handle) else {
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
        for (name, base_type, transport, layout) in declarations {
            let result = if let Some(layout) = layout {
                contract.register_value_type(name, layout)
            } else {
                contract.register_type(name, base_type.as_deref(), transport)
            };
            if let Err(message) = result {
                return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
            }
        }
        if let Err(message) = contract.to_manifest_bytes() {
            return fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default());
        }
        runtime.host_contract = contract;
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Registers v2 host functions with separate logical type and ABI transport metadata.
///
/// # Safety
///
/// `functions` and every non-empty nested slice must remain readable for this call.
pub unsafe extern "C" fn rils_runtime_register_host_functions_v2(
    runtime: Handle,
    functions: *const RilsHostFunctionV2,
    function_count: usize,
) -> i32 {
    status_entry(|| {
        if function_count != 0 && functions.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "host function v2 array is null",
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
            if descriptor.reserved != 0 || descriptor.receiver > 3 || descriptor.function_id == 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host function v2 receiver, reserved fields, or id are invalid",
                    "",
                    Span::default(),
                );
            }
            let name = match unsafe { read_utf8(descriptor.name, "host function name") } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            let capability =
                match unsafe { read_utf8(descriptor.capability, "host function capability") } {
                    Ok(value) => value.to_owned(),
                    Err(status) => return status,
                };
            if descriptor.parameter_count != 0 && descriptor.parameters.is_null() {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host function v2 parameter array is null",
                    "",
                    Span::default(),
                );
            }
            let parameters = if descriptor.parameter_count == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(descriptor.parameters, descriptor.parameter_count) }
            };
            let mut raw_parameters = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                if parameter.reserved != 0 {
                    return fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "host parameter reserved fields must be zero",
                        "",
                        Span::default(),
                    );
                }
                let logical_type =
                    match unsafe { read_utf8(parameter.logical_type, "logical parameter type") } {
                        Ok(value) => value.to_owned(),
                        Err(status) => return status,
                    };
                raw_parameters.push((parameter.transport_tag, logical_type));
            }
            if descriptor.return_parameter.reserved != 0 {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host return parameter reserved fields must be zero",
                    "",
                    Span::default(),
                );
            }
            let return_logical_type = match unsafe {
                read_utf8(
                    descriptor.return_parameter.logical_type,
                    "logical return type",
                )
            } {
                Ok(value) => value.to_owned(),
                Err(status) => return status,
            };
            declarations.push((
                descriptor.function_id,
                name,
                capability,
                raw_parameters,
                (
                    descriptor.return_parameter.transport_tag,
                    return_logical_type,
                ),
                descriptor.receiver,
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
            for (function_id, name, capability, raw_parameters, raw_return, receiver) in
                declarations
            {
                let parameters = match raw_parameters
                    .iter()
                    .map(|(tag, logical)| {
                        logical_type_from_transport(&contract, *tag, logical, false)
                    })
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(value) => value,
                    Err(message) => {
                        return fail(RILS_STATUS_UNSUPPORTED_VALUE, message, "", Span::default());
                    }
                };
                let return_type =
                    match logical_type_from_transport(&contract, raw_return.0, &raw_return.1, true)
                    {
                        Ok(value) => value,
                        Err(message) => {
                            return fail(
                                RILS_STATUS_UNSUPPORTED_VALUE,
                                message,
                                "",
                                Span::default(),
                            );
                        }
                    };
                if let Err(message) = contract.register_function_with_options_and_receiver(
                    function_id,
                    name,
                    FunctionSignature::fixed(parameters, return_type),
                    capability,
                    HostCallKind::Direct,
                    HostThreadAffinity::MainThread,
                    HostReceiver::from_tag(receiver as u8)
                        .expect("receiver kind was validated above"),
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
