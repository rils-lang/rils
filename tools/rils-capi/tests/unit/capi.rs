use super::*;

unsafe extern "C" fn add_dispatcher(
    _user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    _out_error: *mut RilsSlice,
) -> i32 {
    if function_id != 100 || argument_count != 2 || arguments.is_null() || out_value.is_null() {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: The native caller provides two readable arguments for this callback.
    let arguments = unsafe { slice::from_raw_parts(arguments, argument_count) };
    if arguments[0].tag != RILS_VALUE_I32 || arguments[1].tag != RILS_VALUE_I32 {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: The native caller provides writable output storage for this callback.
    unsafe {
        out_value.write(RilsValue {
            tag: RILS_VALUE_I32,
            reserved: 0,
            low: (arguments[0].low as i64 + arguments[1].low as i64) as u64,
            high: 0,
        });
    }
    RILS_STATUS_OK
}

unsafe extern "C" fn handle_dispatcher(
    _user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    _out_error: *mut RilsSlice,
) -> i32 {
    if arguments.is_null() || out_value.is_null() {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    let arguments = unsafe { slice::from_raw_parts(arguments, argument_count) };
    match function_id {
        101 if argument_count == 0 => {}
        102 if argument_count == 1 && arguments[0].tag == RILS_VALUE_HOST_HANDLE => {}
        103 if argument_count == 1 && arguments[0].tag == RILS_VALUE_HOST_HANDLE => {
            unsafe {
                out_value.write(RilsValue {
                    tag: RILS_VALUE_I64,
                    reserved: 0,
                    low: arguments[0].low,
                    high: 0,
                });
            }
            return RILS_STATUS_OK;
        }
        _ => return RILS_STATUS_INVALID_ARGUMENT,
    }
    unsafe {
        out_value.write(RilsValue {
            tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
            low: if function_id == 101 {
                77
            } else {
                arguments[0].low
            },
            high: (3_u64 << 32) | 9,
        });
    }
    RILS_STATUS_OK
}

unsafe extern "C" fn inline_value_dispatcher(
    _user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    _out_error: *mut RilsSlice,
) -> i32 {
    if arguments.is_null() || out_value.is_null() {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    let arguments = unsafe { slice::from_raw_parts(arguments, argument_count) };
    let component = |value: &RilsValue, index: usize| {
        let word = if index < 2 { value.low } else { value.high };
        f32::from_bits((word >> ((index & 1) * 32)) as u32)
    };
    let pack = |x: f32, y: f32, z: f32| RilsValue {
        tag: RILS_VALUE_INLINE_VALUE,
        reserved: 0,
        low: u64::from(x.to_bits()) | (u64::from(y.to_bits()) << 32),
        high: u64::from(z.to_bits()),
    };
    let result = match function_id {
        201 if argument_count == 3 && arguments.iter().all(|value| value.tag == RILS_VALUE_F32) => {
            pack(
                f32::from_bits(arguments[0].low as u32),
                f32::from_bits(arguments[1].low as u32),
                f32::from_bits(arguments[2].low as u32),
            )
        }
        202 if argument_count == 1 && arguments[0].tag == RILS_VALUE_INLINE_VALUE => RilsValue {
            tag: RILS_VALUE_F32,
            reserved: 0,
            low: u64::from(
                (component(&arguments[0], 0)
                    + component(&arguments[0], 1)
                    + component(&arguments[0], 2))
                .to_bits(),
            ),
            high: 0,
        },
        _ => return RILS_STATUS_INVALID_ARGUMENT,
    };
    unsafe { out_value.write(result) };
    RILS_STATUS_OK
}

unsafe extern "C" fn enum_dispatcher(
    _user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    _out_error: *mut RilsSlice,
) -> i32 {
    if function_id != 301 || argument_count != 1 || arguments.is_null() || out_value.is_null() {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    let arguments = unsafe { slice::from_raw_parts(arguments, argument_count) };
    if arguments[0].tag != RILS_VALUE_I32 || arguments[0].low != 1 {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    unsafe {
        out_value.write(RilsValue {
            tag: RILS_VALUE_I32,
            reserved: 0,
            low: 3,
            high: 0,
        });
    }
    RILS_STATUS_OK
}

unsafe extern "C" fn string_dispatcher(
    _user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    _out_error: *mut RilsSlice,
) -> i32 {
    if function_id != 401 || argument_count != 1 || arguments.is_null() || out_value.is_null() {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: The runtime provides one callback-scoped argument.
    let argument = unsafe { *arguments };
    if argument.tag != RILS_VALUE_STRING || argument.reserved != 0 || argument.high != 0 {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    let mut size = 0;
    if unsafe { rils_string_size(argument.low, &mut size) } != RILS_STATUS_OK {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    let mut utf8 = vec![0; size];
    let mut written = 0;
    // SAFETY: `utf8` provides writable storage of the reported size.
    if unsafe { rils_string_write(argument.low, utf8.as_mut_ptr(), utf8.len(), &mut written) }
        != RILS_STATUS_OK
    {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    if rils_string_destroy(argument.low) != RILS_STATUS_OK {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    utf8.truncate(written);
    let Ok(input) = String::from_utf8(utf8) else {
        return RILS_STATUS_INVALID_ARGUMENT;
    };
    let output = format!("received:{input}");
    let mut output_handle = 0;
    // SAFETY: The output string remains readable for this call.
    if unsafe { rils_string_create(bytes(&output), &mut output_handle) } != RILS_STATUS_OK {
        return RILS_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: The runtime provides writable output storage.
    unsafe {
        out_value.write(RilsValue {
            tag: RILS_VALUE_STRING,
            reserved: 0,
            low: output_handle,
            high: 0,
        });
    }
    RILS_STATUS_OK
}

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

unsafe extern "C" fn output_callback(user_data: *mut c_void, text: RilsSlice, newline: u32) {
    // SAFETY: The test keeps the Vec alive and exclusively borrowed during script execution.
    let events = unsafe { &mut *(user_data.cast::<Vec<(String, bool)>>()) };
    // SAFETY: The runtime guarantees a readable callback-scoped UTF-8 slice.
    let bytes = unsafe { slice::from_raw_parts(text.data, text.length) };
    events.push((String::from_utf8(bytes.to_vec()).unwrap(), newline != 0));
}

unsafe extern "C" fn inline_value_formatter(
    _user_data: *mut c_void,
    logical_type: RilsSlice,
    value: RilsValue,
    kind: u32,
    _alternate: u32,
    _precision: usize,
    buffer: *mut u8,
    capacity: usize,
) -> usize {
    // SAFETY: The runtime guarantees readable callback-scoped slices.
    let logical_type = unsafe { slice::from_raw_parts(logical_type.data, logical_type.length) };
    if logical_type != b"unity_engine::Vector3" || value.tag != RILS_VALUE_INLINE_VALUE || kind != 0
    {
        return usize::MAX;
    }
    let x = f32::from_bits(value.low as u32);
    let y = f32::from_bits((value.low >> 32) as u32);
    let z = f32::from_bits(value.high as u32);
    let rendered = format!("Vector3({x}, {y}, {z})");
    if !buffer.is_null() && capacity >= rendered.len() {
        // SAFETY: The runtime supplies a writable buffer with the reported capacity.
        unsafe { ptr::copy_nonoverlapping(rendered.as_ptr(), buffer, rendered.len()) };
    }
    rendered.len()
}

#[test]
fn routes_formatted_output_through_the_runtime_callback() {
    let runtime = rils_runtime_create();
    let mut events = Vec::new();
    assert_eq!(
        rils_runtime_set_output_callback(
            runtime,
            Some(output_callback),
            (&mut events as *mut Vec<(String, bool)>).cast(),
        ),
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_allow_standard_library(runtime), RILS_STATUS_OK);
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);
    let source = r#"
        let _ = std::fs::try_exists("definitely-missing-rils-standard-library-smoke");
        print!("value={}", 7);
        println!(" done");
    "#;
    let mut module = 0;
    assert_eq!(
        unsafe { rils_module_compile(runtime, bytes("output.rils"), bytes(source), &mut module) },
        RILS_STATUS_OK
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK
    );
    assert_eq!(
        events,
        [("value=7".to_string(), false), (" done".to_string(), true)]
    );
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn registers_freezes_and_dispatches_custom_host_functions() {
    let runtime = rils_runtime_create();
    let parameter_tags = [RILS_VALUE_I32, RILS_VALUE_I32];
    let descriptor = RilsHostFunction {
        function_id: 100,
        name: bytes("unity_engine::math::add"),
        capability: bytes("unity.math"),
        parameter_tags: parameter_tags.as_ptr(),
        parameter_count: parameter_tags.len(),
        return_tag: RILS_VALUE_I32,
        reserved: 0,
    };
    // SAFETY: All descriptor pointers remain readable for the registration call.
    assert_eq!(
        unsafe { rils_runtime_register_host_functions(runtime, &descriptor, 1) },
        RILS_STATUS_OK
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(add_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    // SAFETY: The capability slice remains readable for the call.
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("unity.math")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);

    let source = "unity_engine::math::add(20, 22)";
    let mut module = 0;
    // SAFETY: Source slices and output storage remain valid for the call.
    assert_eq!(
        unsafe { rils_module_compile(runtime, bytes("host.rils"), bytes(source), &mut module) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_module_validate_host(runtime, module), RILS_STATUS_OK);
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    // SAFETY: Output storage remains valid for the call.
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK
    );
    assert_eq!(result.tag, RILS_VALUE_I32);
    assert_eq!(result.low, 42);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn dispatches_opaque_host_handles_through_bytecode() {
    let runtime = rils_runtime_create();
    let no_parameters: [u32; 0] = [];
    let handle_parameter = [RILS_VALUE_HOST_HANDLE];
    let descriptors = [
        RilsHostFunction {
            function_id: 101,
            name: bytes("unity_engine::object::get"),
            capability: bytes("unity.object"),
            parameter_tags: no_parameters.as_ptr(),
            parameter_count: 0,
            return_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
        RilsHostFunction {
            function_id: 102,
            name: bytes("unity_engine::object::echo"),
            capability: bytes("unity.object"),
            parameter_tags: handle_parameter.as_ptr(),
            parameter_count: 1,
            return_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe { rils_runtime_register_host_functions(runtime, descriptors.as_ptr(), 2) },
        RILS_STATUS_OK
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(handle_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("unity.object")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);

    let source = "fn echo(handle: HostHandle) -> HostHandle { unity_engine::object::echo(handle) } fn run() -> HostHandle { echo(unity_engine::object::get()) } run()";
    let mut module = 0;
    assert_eq!(
        unsafe { rils_module_compile(runtime, bytes("handle.rils"), bytes(source), &mut module) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_module_validate_host(runtime, module), RILS_STATUS_OK);
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK
    );
    assert_eq!(result.tag, RILS_VALUE_HOST_HANDLE);
    assert_eq!(result.low, 77);
    assert_eq!(result.high, (3_u64 << 32) | 9);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn dispatches_named_host_types_with_inherited_receiver_methods() {
    let runtime = rils_runtime_create();
    let types = [
        RilsHostType {
            name: bytes("unity_engine::Object"),
            base_type: bytes(""),
            transport_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
        RilsHostType {
            name: bytes("unity_engine::GameObject"),
            base_type: bytes("unity_engine::Object"),
            transport_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe { rils_runtime_register_host_types(runtime, types.as_ptr(), types.len()) },
        RILS_STATUS_OK
    );
    let object_parameter = [RilsHostParameter {
        logical_type: bytes("unity_engine::Object"),
        transport_tag: RILS_VALUE_HOST_HANDLE,
        reserved: 0,
    }];
    let functions = [
        RilsHostFunctionV2 {
            function_id: 101,
            name: bytes("unity_engine::object::get"),
            capability: bytes("unity.object"),
            parameters: ptr::null(),
            parameter_count: 0,
            return_parameter: RilsHostParameter {
                logical_type: bytes("unity_engine::GameObject"),
                transport_tag: RILS_VALUE_HOST_HANDLE,
                reserved: 0,
            },
            receiver: 0,
            reserved: 0,
        },
        RilsHostFunctionV2 {
            function_id: 103,
            name: bytes("unity_engine::object::instance_id"),
            capability: bytes("unity.object"),
            parameters: object_parameter.as_ptr(),
            parameter_count: object_parameter.len(),
            return_parameter: RilsHostParameter {
                logical_type: bytes(""),
                transport_tag: RILS_VALUE_I64,
                reserved: 0,
            },
            receiver: 2,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe {
            rils_runtime_register_host_functions_v2(runtime, functions.as_ptr(), functions.len())
        },
        RILS_STATUS_OK
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(handle_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("unity.object")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);

    let source = "unity_engine::object::get().instance_id()";
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(
                runtime,
                bytes("named-host.rils"),
                bytes(source),
                &mut module,
            )
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(result.tag, RILS_VALUE_I64);
    assert_eq!(result.low, 77);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn dispatches_inline_value_types_through_multiple_host_calls() {
    let runtime = rils_runtime_create();
    let mut output = Vec::new();
    let types = [RilsHostTypeV2 {
        name: bytes("unity_engine::Vector3"),
        base_type: bytes(""),
        value_layout: bytes("f32x3"),
        transport_tag: RILS_VALUE_INLINE_VALUE,
        kind: RILS_HOST_TYPE_VALUE,
        reserved: 0,
    }];
    assert_eq!(
        unsafe { rils_runtime_register_host_types_v2(runtime, types.as_ptr(), types.len()) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let scalar_parameters = [RilsHostParameter {
        logical_type: bytes(""),
        transport_tag: RILS_VALUE_F32,
        reserved: 0,
    }; 3];
    let vector_parameter = [RilsHostParameter {
        logical_type: bytes("unity_engine::Vector3"),
        transport_tag: RILS_VALUE_INLINE_VALUE,
        reserved: 0,
    }];
    let functions = [
        RilsHostFunctionV2 {
            function_id: 201,
            name: bytes("unity_engine::vector3::new"),
            capability: bytes("unity.math"),
            parameters: scalar_parameters.as_ptr(),
            parameter_count: scalar_parameters.len(),
            return_parameter: vector_parameter[0],
            receiver: 0,
            reserved: 0,
        },
        RilsHostFunctionV2 {
            function_id: 202,
            name: bytes("unity_engine::vector3::component_sum"),
            capability: bytes("unity.math"),
            parameters: vector_parameter.as_ptr(),
            parameter_count: vector_parameter.len(),
            return_parameter: RilsHostParameter {
                logical_type: bytes(""),
                transport_tag: RILS_VALUE_F32,
                reserved: 0,
            },
            receiver: 0,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe {
            rils_runtime_register_host_functions_v2(runtime, functions.as_ptr(), functions.len())
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(inline_value_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    assert_eq!(
        rils_runtime_set_output_callback(
            runtime,
            Some(output_callback),
            (&mut output as *mut Vec<(String, bool)>).cast(),
        ),
        RILS_STATUS_OK
    );
    assert_eq!(
        rils_runtime_set_host_value_formatter(
            runtime,
            Some(inline_value_formatter),
            ptr::null_mut(),
        ),
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_allow_standard_library(runtime), RILS_STATUS_OK);
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("unity.math")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);

    let source = r#"
        let value = unity_engine::vector3::new(1.25f32, 2.5f32, 3.75f32);
        println!("value={}", value);
        unity_engine::vector3::component_sum(value)
    "#;
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(
                runtime,
                bytes("inline-value.rils"),
                bytes(source),
                &mut module,
            )
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(result.tag, RILS_VALUE_F32);
    assert_eq!(f32::from_bits(result.low as u32), 7.5);
    assert_eq!(
        output,
        [("value=Vector3(1.25, 2.5, 3.75)".to_string(), true)]
    );
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn dispatches_real_host_enums_and_preserves_unknown_flags_combinations() {
    let runtime = rils_runtime_create();
    let camera_variants = [
        RilsHostEnumVariant {
            name: bytes("Game"),
            raw_low: 1,
            raw_high: 0,
        },
        RilsHostEnumVariant {
            name: bytes("SceneView"),
            raw_low: 2,
            raw_high: 0,
        },
    ];
    let flag_variants = [
        RilsHostEnumVariant {
            name: bytes("None"),
            raw_low: 0,
            raw_high: 0,
        },
        RilsHostEnumVariant {
            name: bytes("HideInHierarchy"),
            raw_low: 1,
            raw_high: 0,
        },
        RilsHostEnumVariant {
            name: bytes("HideInInspector"),
            raw_low: 2,
            raw_high: 0,
        },
    ];
    let types = [
        RilsHostTypeV3 {
            name: bytes("unity_engine::CameraType"),
            base_type: bytes(""),
            value_layout: bytes(""),
            enum_variants: camera_variants.as_ptr(),
            enum_variant_count: camera_variants.len(),
            transport_tag: RILS_VALUE_I32,
            kind: RILS_HOST_TYPE_ENUM,
            enum_flags: 0,
            reserved: 0,
        },
        RilsHostTypeV3 {
            name: bytes("unity_engine::HideFlags"),
            base_type: bytes(""),
            value_layout: bytes(""),
            enum_variants: flag_variants.as_ptr(),
            enum_variant_count: flag_variants.len(),
            transport_tag: RILS_VALUE_I32,
            kind: RILS_HOST_TYPE_ENUM,
            enum_flags: 1,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe { rils_runtime_register_host_types_v3(runtime, types.as_ptr(), types.len()) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let parameters = [RilsHostParameter {
        logical_type: bytes("unity_engine::CameraType"),
        transport_tag: RILS_VALUE_I32,
        reserved: 0,
    }];
    let function = RilsHostFunctionV2 {
        function_id: 301,
        name: bytes("unity_engine::camera::flags_for"),
        capability: bytes("unity.camera"),
        parameters: parameters.as_ptr(),
        parameter_count: parameters.len(),
        return_parameter: RilsHostParameter {
            logical_type: bytes("unity_engine::HideFlags"),
            transport_tag: RILS_VALUE_I32,
            reserved: 0,
        },
        receiver: 0,
        reserved: 0,
    };
    assert_eq!(
        unsafe { rils_runtime_register_host_functions_v2(runtime, &function, 1) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(enum_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("unity.camera")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);

    let source = r#"
        use unity_engine::{CameraType, HideFlags};
        let flags = unity_engine::camera::flags_for(CameraType::Game);
        match flags {
            HideFlags::None => 0,
            _ => 1,
        }
    "#;
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(runtime, bytes("host-enum.rils"), bytes(source), &mut module)
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(result.tag, RILS_VALUE_I32);
    assert_eq!(result.low, 1);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn restores_named_host_types_for_trait_call_arguments() {
    let runtime = rils_runtime_create();
    let types = [
        RilsHostType {
            name: bytes("unity_engine::Object"),
            base_type: bytes(""),
            transport_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
        RilsHostType {
            name: bytes("unity_engine::GameObject"),
            base_type: bytes("unity_engine::Object"),
            transport_tag: RILS_VALUE_HOST_HANDLE,
            reserved: 0,
        },
    ];
    assert_eq!(
        unsafe { rils_runtime_register_host_types(runtime, types.as_ptr(), types.len()) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);
    let source = r#"
        trait Behaviour: Default {
            fn tick(&mut self, host: unity_engine::GameObject) -> i64;
        }
        #[derive(Default)]
        struct State;
        impl Behaviour for State {
            fn tick(&mut self, host: unity_engine::GameObject) -> i64 {
                42i64
            }
        }
    "#;
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(
                runtime,
                bytes("typed-trait-argument.rils"),
                bytes(source),
                &mut module,
            )
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut state = 0;
    assert_eq!(
        unsafe { rils_script_value_create_default(runtime, instance, bytes("State"), &mut state) },
        RILS_STATUS_OK
    );
    let argument = RilsValue {
        tag: RILS_VALUE_HOST_HANDLE,
        low: 77,
        high: (3_u64 << 32) | 9,
        ..RilsValue::default()
    };
    let argument_type = RilsHostParameter {
        logical_type: bytes("unity_engine::GameObject"),
        transport_tag: RILS_VALUE_HOST_HANDLE,
        reserved: 0,
    };
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe {
            rils_script_value_call_trait(
                runtime,
                instance,
                state,
                bytes("Behaviour"),
                bytes("tick"),
                &argument,
                &argument_type,
                1,
                &mut result,
            )
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(result.tag, RILS_VALUE_I64);
    assert_eq!(result.low, 42);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn registers_and_exports_canonical_host_manifest() {
    let mut contract = HostContract::new();
    contract.register_module("unity_engine::math", 3).unwrap();
    contract
        .register_function(
            100,
            "unity_engine::math::add",
            FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
            "unity.math",
        )
        .unwrap();
    let manifest = contract.to_manifest_bytes().unwrap();
    let runtime = rils_runtime_create();
    // SAFETY: The manifest bytes remain readable for the registration call.
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&manifest)) },
        RILS_STATUS_OK
    );

    let mut size = 0;
    // SAFETY: The output size remains writable for the call.
    assert_eq!(
        unsafe { rils_runtime_host_manifest_size(runtime, &mut size) },
        RILS_STATUS_OK
    );
    let mut exported = vec![0; size];
    let mut written = 0;
    // SAFETY: The output buffer and count remain writable for the call.
    assert_eq!(
        unsafe {
            rils_runtime_write_host_manifest(
                runtime,
                exported.as_mut_ptr(),
                exported.len(),
                &mut written,
            )
        },
        RILS_STATUS_OK
    );
    assert_eq!(written, size);
    assert_eq!(exported, manifest);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn merges_compatible_host_manifest_fragments() {
    let mut first = HostContract::new();
    first
        .register_function(
            301,
            "unity::object::is_valid",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::Bool),
            "unity.object",
        )
        .unwrap();
    let mut second = HostContract::new();
    second
        .register_function(
            302,
            "unity::object::instance_id",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::I32),
            "unity.object",
        )
        .unwrap();
    let first_bytes = first.to_manifest_bytes().unwrap();
    let second_bytes = second.to_manifest_bytes().unwrap();
    let runtime = rils_runtime_create();
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&first_bytes)) },
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&second_bytes)) },
        RILS_STATUS_OK
    );
    let mut expected = first;
    expected.merge(&second).unwrap();
    let expected_bytes = expected.to_manifest_bytes().unwrap();
    let mut size = 0;
    assert_eq!(
        unsafe { rils_runtime_host_manifest_size(runtime, &mut size) },
        RILS_STATUS_OK
    );
    let mut exported = vec![0; size];
    let mut written = 0;
    assert_eq!(
        unsafe {
            rils_runtime_write_host_manifest(
                runtime,
                exported.as_mut_ptr(),
                exported.len(),
                &mut written,
            )
        },
        RILS_STATUS_OK
    );
    assert_eq!(exported, expected_bytes);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn rejects_conflicting_host_manifest_fragments_without_partial_registration() {
    let mut first = HostContract::new();
    first
        .register_function(
            401,
            "unity::object::get_id",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::I32),
            "unity.object",
        )
        .unwrap();
    let mut conflicting = HostContract::new();
    conflicting
        .register_function(
            402,
            "unity::object::get_id",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::Bool),
            "unity.object",
        )
        .unwrap();
    let first_bytes = first.to_manifest_bytes().unwrap();
    let conflicting_bytes = conflicting.to_manifest_bytes().unwrap();
    let runtime = rils_runtime_create();
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&first_bytes)) },
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&conflicting_bytes)) },
        RILS_STATUS_INVALID_ARGUMENT
    );
    assert!(current_error_message().contains("conflict"));

    let mut size = 0;
    assert_eq!(
        unsafe { rils_runtime_host_manifest_size(runtime, &mut size) },
        RILS_STATUS_OK
    );
    let mut exported = vec![0; size];
    let mut written = 0;
    assert_eq!(
        unsafe {
            rils_runtime_write_host_manifest(
                runtime,
                exported.as_mut_ptr(),
                exported.len(),
                &mut written,
            )
        },
        RILS_STATUS_OK
    );
    assert_eq!(&exported[..written], first_bytes.as_slice());
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn string_handles_round_trip_utf8_and_reject_consumed_handles() {
    for value in ["", "Rils", "你好，Unity 👋"] {
        let mut handle = 0;
        assert_eq!(
            unsafe { rils_string_create(bytes(value), &mut handle) },
            RILS_STATUS_OK
        );
        let mut size = usize::MAX;
        assert_eq!(
            unsafe { rils_string_size(handle, &mut size) },
            RILS_STATUS_OK
        );
        assert_eq!(size, value.len());
        let mut buffer = vec![0; size];
        let mut written = usize::MAX;
        assert_eq!(
            unsafe { rils_string_write(handle, buffer.as_mut_ptr(), buffer.len(), &mut written) },
            RILS_STATUS_OK
        );
        assert_eq!(written, value.len());
        assert_eq!(buffer, value.as_bytes());
        assert_eq!(rils_string_destroy(handle), RILS_STATUS_OK);
        assert_eq!(
            unsafe { rils_string_size(handle, &mut size) },
            RILS_STATUS_INVALID_HANDLE
        );
    }
}

#[test]
fn failed_calls_destroy_all_transferred_string_arguments() {
    let runtime = rils_runtime_create();
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(
                runtime,
                bytes("string-cleanup.rils"),
                bytes("pub fn consume(flag: bool, text: string) { }"),
                &mut module,
            )
        },
        RILS_STATUS_OK
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut string = 0;
    assert_eq!(
        unsafe { rils_string_create(bytes("transferred"), &mut string) },
        RILS_STATUS_OK
    );
    let arguments = [
        RilsValue {
            tag: RILS_VALUE_BOOL,
            low: 2,
            ..RilsValue::default()
        },
        RilsValue {
            tag: RILS_VALUE_STRING,
            low: string,
            ..RilsValue::default()
        },
    ];
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe {
            rils_instance_call(
                runtime,
                instance,
                bytes("consume"),
                arguments.as_ptr(),
                arguments.len(),
                &mut result,
            )
        },
        RILS_STATUS_INVALID_ARGUMENT
    );
    let mut size = 0;
    assert_eq!(
        unsafe { rils_string_size(string, &mut size) },
        RILS_STATUS_INVALID_HANDLE
    );
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
}

#[test]
fn dispatches_strings_through_owned_abi_handles() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            401,
            "host::echo",
            FunctionSignature::fixed(vec![Type::String], Type::String),
            "host.string",
        )
        .unwrap();
    let manifest = contract.to_manifest_bytes().unwrap();
    let runtime = rils_runtime_create();
    // SAFETY: The manifest bytes remain readable for the call.
    assert_eq!(
        unsafe { rils_runtime_register_host_manifest(runtime, raw_bytes(&manifest)) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(
        rils_runtime_set_host_dispatcher(runtime, Some(string_dispatcher), ptr::null_mut()),
        RILS_STATUS_OK
    );
    assert_eq!(
        unsafe { rils_runtime_allow_capability(runtime, bytes("host.string")) },
        RILS_STATUS_OK
    );
    assert_eq!(rils_runtime_freeze_host_registry(runtime), RILS_STATUS_OK);
    let mut module = 0;
    assert_eq!(
        unsafe {
            rils_module_compile(
                runtime,
                bytes("host-string.rils"),
                bytes(r#"host::echo("你好，Unity 👋") == "received:你好，Unity 👋""#),
                &mut module,
            )
        },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut result = RilsValue::default();
    assert_eq!(
        unsafe { rils_instance_execute(runtime, instance, &mut result) },
        RILS_STATUS_OK,
        "{}",
        current_error_message()
    );
    assert_eq!(result.tag, RILS_VALUE_BOOL);
    assert_eq!(result.low, 1);
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
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
fn discovers_trait_entries_and_calls_persistent_script_values() {
    let runtime = rils_runtime_create();
    let source = r#"
        trait Behaviour: Default { fn tick(&mut self, amount: i32) -> i32; }
        #[derive(Default)]
        struct State { value: i32 }
        impl Behaviour for State {
            fn tick(&mut self, amount: i32) -> i32 {
                self.value = self.value + amount;
                self.value
            }
        }
    "#;
    let source_path = std::env::temp_dir().join(format!(
        "rils-capi-trait-source-test-{}.rils",
        std::process::id()
    ));
    std::fs::write(&source_path, source).unwrap();
    let source_name = source_path.to_string_lossy();
    let mut module = 0;
    assert_eq!(
        unsafe { rils_module_compile_file(runtime, bytes(&source_name), &mut module) },
        RILS_STATUS_OK
    );
    let mut count = 0;
    assert_eq!(
        unsafe {
            rils_module_trait_implementation_count(
                runtime,
                module,
                bytes("Behaviour"),
                bytes(&source_name),
                &mut count,
            )
        },
        RILS_STATUS_OK
    );
    assert_eq!(count, 1);
    let mut name_size = 0;
    assert_eq!(
        unsafe {
            rils_module_trait_implementation_name_size(
                runtime,
                module,
                bytes("Behaviour"),
                bytes(&source_name),
                0,
                &mut name_size,
            )
        },
        RILS_STATUS_OK
    );
    let mut name = vec![0; name_size];
    let mut written = 0;
    assert_eq!(
        unsafe {
            rils_module_write_trait_implementation_name(
                runtime,
                module,
                bytes("Behaviour"),
                bytes(&source_name),
                0,
                name.as_mut_ptr(),
                name.len(),
                &mut written,
            )
        },
        RILS_STATUS_OK
    );
    assert_eq!(std::str::from_utf8(&name).unwrap(), "State");

    let mut instance = 0;
    assert_eq!(
        unsafe { rils_instance_create(runtime, module, &mut instance) },
        RILS_STATUS_OK
    );
    let mut state = 0;
    assert_eq!(
        unsafe { rils_script_value_create_default(runtime, instance, bytes("State"), &mut state) },
        RILS_STATUS_OK
    );
    for (amount, expected) in [(2, 2), (3, 5)] {
        let argument = RilsValue {
            tag: RILS_VALUE_I32,
            low: amount,
            ..RilsValue::default()
        };
        let argument_type = RilsHostParameter {
            logical_type: bytes(""),
            transport_tag: RILS_VALUE_I32,
            reserved: 0,
        };
        let mut result = RilsValue::default();
        assert_eq!(
            unsafe {
                rils_script_value_call_trait(
                    runtime,
                    instance,
                    state,
                    bytes("Behaviour"),
                    bytes("tick"),
                    &argument,
                    &argument_type,
                    1,
                    &mut result,
                )
            },
            RILS_STATUS_OK
        );
        assert_eq!(result.tag, RILS_VALUE_I32);
        assert_eq!(result.low, expected);
    }
    assert_eq!(rils_module_destroy(runtime, module), RILS_STATUS_OK);
    assert_eq!(
        rils_script_value_destroy(runtime, state),
        RILS_STATUS_INVALID_HANDLE
    );
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);
    std::fs::remove_file(source_path).unwrap();
}

#[test]
fn loads_and_executes_bytecode_from_memory() {
    let image = rils_bytecode::compile("40 + 2")
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
        unsafe { rils_module_compile(runtime, bytes("export.rils"), bytes("40 + 2"), &mut module) },
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
        assert_eq!(from_ffi_value(encoded, None).unwrap(), expected);
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
fn compile_file_reports_the_dependency_source_name() {
    let directory = std::env::temp_dir().join(format!(
        "rils-capi-source-id-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let entry = directory.join("main.rils");
    let dependency = directory.join("broken.rils");
    std::fs::write(&entry, "mod broken; 42").unwrap();
    std::fs::write(&dependency, "pub fn value() -> i32 { missing }").unwrap();

    let runtime = rils_runtime_create();
    let mut module = 0;
    let entry_text = entry.to_str().unwrap();
    // SAFETY: The path and output pointer remain valid for the duration of the call.
    assert_eq!(
        unsafe { rils_module_compile_file(runtime, bytes(entry_text), &mut module) },
        RILS_STATUS_COMPILE_ERROR
    );
    let name = rils_last_error_source_name();
    // SAFETY: Error strings remain borrowed until the next non-getter ABI call.
    let name =
        unsafe { std::str::from_utf8_unchecked(slice::from_raw_parts(name.data, name.length)) };
    assert_eq!(name, dependency.to_string_lossy());
    assert_eq!(rils_runtime_destroy(runtime), RILS_STATUS_OK);

    std::fs::remove_dir_all(directory).unwrap();
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
        unsafe { rils_module_compile(runtime, bytes("stress.rils"), bytes(source), &mut module) },
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
