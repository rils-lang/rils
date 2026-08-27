use super::*;

fn example_contract() -> HostContract {
    let mut contract = HostContract::new();
    contract.register_module("unity_engine::time", 2).unwrap();
    contract
        .register_function(
            7,
            "unity_engine::time::frame_count",
            FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
            "unity.time",
        )
        .unwrap();
    contract
}

#[test]
fn contract_rejects_duplicate_names_ids_and_non_portable_types() {
    let mut contract = example_contract();
    assert!(
        contract
            .register_function(
                8,
                "unity_engine::time::frame_count",
                FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
                "unity.time",
            )
            .unwrap_err()
            .contains("already declared")
    );
    assert!(
        contract
            .register_function(
                7,
                "unity_engine::time::delta_time",
                FunctionSignature::fixed(Vec::new(), Type::Float(FloatType::F32)),
                "unity.time",
            )
            .unwrap_err()
            .contains("id 7")
    );
    assert!(
        HostContract::new()
            .register_function(
                1,
                "unity_engine::bad",
                FunctionSignature::fixed(
                    vec![Type::Option(Box::new(Type::Integer(IntegerType::I32)))],
                    Type::Unit,
                ),
                "unity",
            )
            .unwrap_err()
            .contains("not supported")
    );
}

#[test]
fn manifest_v5_round_trips_every_portable_scalar() {
    let mut contract = HostContract::new();
    contract.register_module("host::scalar", 1).unwrap();
    let scalars = vec![
        Type::Bool,
        Type::Integer(IntegerType::I8),
        Type::Integer(IntegerType::I16),
        Type::Integer(IntegerType::I32),
        Type::Integer(IntegerType::I64),
        Type::Integer(IntegerType::I128),
        Type::Integer(IntegerType::Isize),
        Type::Integer(IntegerType::U8),
        Type::Integer(IntegerType::U16),
        Type::Integer(IntegerType::U32),
        Type::Integer(IntegerType::U64),
        Type::Integer(IntegerType::U128),
        Type::Integer(IntegerType::Usize),
        Type::Float(FloatType::F32),
        Type::Float(FloatType::F64),
        Type::Char,
        Type::String,
    ];
    contract
        .register_function(
            11,
            "host::scalar::round_trip",
            FunctionSignature::fixed(scalars, Type::Char),
            "host.scalar",
        )
        .unwrap();

    let bytes = contract.to_manifest_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        HOST_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(HostContract::from_manifest_bytes(&bytes).unwrap(), contract);
    assert_eq!(
        HostContract::from_manifest_json(&contract.to_manifest_json().unwrap()).unwrap(),
        contract
    );
    assert!(binary_v2::encode_legacy_v4(&contract).is_err());
}

#[test]
fn manifest_v5_round_trips_host_enums_and_flags() {
    let mut contract = HostContract::new();
    contract
        .register_enum_type(
            "unity_engine::CameraType",
            IntegerType::I32,
            false,
            [
                ("Game".to_owned(), 1),
                ("SceneView".to_owned(), 2),
                ("All".to_owned(), u128::from(u32::MAX)),
            ],
        )
        .unwrap();
    contract
        .register_enum_type(
            "unity_engine::HideFlags",
            IntegerType::I32,
            true,
            [
                ("None".to_owned(), 0),
                ("HideInHierarchy".to_owned(), 1),
                ("HideInInspector".to_owned(), 2),
            ],
        )
        .unwrap();
    contract
        .register_function(
            21,
            "unity_engine::camera::set_type",
            FunctionSignature::fixed(
                vec![Type::named("unity_engine::CameraType")],
                Type::named("unity_engine::HideFlags"),
            ),
            "unity",
        )
        .unwrap();

    let bytes = contract.to_manifest_bytes().unwrap();
    assert_eq!(HostContract::from_manifest_bytes(&bytes).unwrap(), contract);
    let json = contract.to_manifest_json().unwrap();
    assert!(json.contains("\"kind\": \"enum\""));
    assert!(json.contains("\"flags\": true"));
    assert_eq!(HostContract::from_manifest_json(&json).unwrap(), contract);
    assert!(binary_v2::encode_legacy_v4(&contract).is_err());
}

#[test]
fn manifest_round_trips_canonically_and_verifies_hash() {
    let contract = example_contract();
    let json = contract.to_manifest_json().unwrap();
    let parsed = HostContract::from_manifest_json(&json).unwrap();
    assert_eq!(parsed, contract);
    assert_eq!(parsed.to_manifest_json().unwrap(), json);
    assert_eq!(parsed.contract_hash().len(), 32);
    assert!(json.contains("\"id\": \"0x0000000000000007\""));

    let corrupted = json.replace("frame_count", "fixed_count");
    assert!(
        HostContract::from_manifest_json(&corrupted)
            .unwrap_err()
            .contains("hash mismatch")
    );
}

#[test]
fn binary_manifest_round_trips_canonically_and_rejects_corruption() {
    let contract = example_contract();
    let manifest = contract.to_manifest_bytes().unwrap();
    assert_eq!(&manifest[..8], &HOST_MANIFEST_MAGIC);
    assert_eq!(
        HostContract::from_manifest_bytes(&manifest).unwrap(),
        contract
    );
    assert_eq!(
        HostContract::from_manifest_bytes(&manifest)
            .unwrap()
            .to_manifest_bytes()
            .unwrap(),
        manifest
    );

    let mut corrupted = manifest.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(
        HostContract::from_manifest_bytes(&corrupted)
            .unwrap_err()
            .contains("hash mismatch")
    );
    assert!(
        HostContract::from_manifest_bytes(&manifest[..manifest.len() - 1])
            .unwrap_err()
            .contains("length mismatch")
    );
}

#[test]
fn named_host_types_round_trip_with_inheritance_and_transport() {
    let mut contract = HostContract::new();
    contract
        .register_type(
            "unity_engine::Object",
            None::<&str>,
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_type(
            "unity_engine::GameObject",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_function_with_options_and_receiver(
            90,
            "unity_engine::object::is_valid",
            FunctionSignature::fixed(vec![Type::named("unity_engine::Object")], Type::Bool),
            "unity.object",
            HostCallKind::Direct,
            HostThreadAffinity::MainThread,
            Some(HostReceiver::Ref),
        )
        .unwrap();

    let bytes = contract.to_manifest_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        HOST_MANIFEST_FORMAT_VERSION
    );
    let decoded = HostContract::from_manifest_bytes(&bytes).unwrap();
    assert_eq!(decoded, contract);
    assert!(decoded.is_type_assignable("unity_engine::Object", "unity_engine::GameObject"));
    let json = decoded.to_manifest_json().unwrap();
    assert!(json.contains("\"transport\": \"HostHandle\""));
    assert!(json.contains("\"base\": \"unity_engine::Object\""));
    assert_eq!(HostContract::from_manifest_json(&json).unwrap(), contract);
}

#[test]
fn inline_value_types_round_trip_with_canonical_layouts() {
    let mut contract = HostContract::new();
    contract
        .register_value_type("unity_engine::Vector3", HostValueLayout::F32x3)
        .unwrap();
    contract
        .register_function(
            91,
            "unity_engine::vector3::zero",
            FunctionSignature::fixed(Vec::new(), Type::named("unity_engine::Vector3")),
            "unity.vector3",
        )
        .unwrap();

    let bytes = contract.to_manifest_bytes().unwrap();
    assert_eq!(HostContract::from_manifest_bytes(&bytes).unwrap(), contract);
    let json = contract.to_manifest_json().unwrap();
    assert!(json.contains("\"kind\": \"value\""));
    assert!(json.contains("\"layout\": \"fields(f32,f32,f32)\""));
    assert!(json.contains("\"transport\": \"InlineValue\""));
    assert_eq!(HostContract::from_manifest_json(&json).unwrap(), contract);

    let color32 = HostValueLayout::from_fields(&[
        HostValueFieldType::U8,
        HostValueFieldType::U8,
        HostValueFieldType::U8,
        HostValueFieldType::U8,
    ])
    .unwrap();
    assert_eq!(color32.byte_len(), 4);
    assert_eq!(color32.canonical_name(), "fields(u8,u8,u8,u8)");
    assert_eq!(
        HostValueLayout::parse("fields(u8,u8,u8,u8)").unwrap(),
        color32
    );
    assert_eq!(
        HostValueLayout::parse("f32x3").unwrap(),
        HostValueLayout::F32x3
    );
    assert!(
        HostValueLayout::from_fields(&[HostValueFieldType::F64; 3])
            .unwrap_err()
            .contains("16-byte ABI payload")
    );

    assert!(
        contract
            .register_type(
                "unity_engine::Broken",
                None::<&str>,
                HostTypeTransport::InlineValue,
            )
            .unwrap_err()
            .contains("register_value_type")
    );
}

#[test]
fn binary_v2_manifests_remain_loadable() {
    let contract = example_contract();
    let bytes = binary_v2::encode_legacy_v2(&contract).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(HostContract::from_manifest_bytes(&bytes).unwrap(), contract);
}

#[test]
fn binary_v3_manifests_remain_loadable() {
    let mut contract = HostContract::new();
    contract
        .register_value_type("unity_engine::Vector3", HostValueLayout::F32x3)
        .unwrap();
    let legacy = binary_v2::encode_legacy_v3(&contract).unwrap();
    assert_eq!(
        u32::from_le_bytes(legacy[8..12].try_into().unwrap()),
        HOST_MANIFEST_V3_FORMAT_VERSION
    );
    assert_eq!(
        HostContract::from_manifest_bytes(&legacy).unwrap(),
        contract
    );
}

#[test]
fn named_host_types_reject_missing_bases_and_cycles() {
    let mut missing = HostContract::new();
    missing
        .register_type(
            "unity_engine::GameObject",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    assert!(
        missing
            .to_manifest_bytes()
            .unwrap_err()
            .contains("unknown host type")
    );

    let mut cyclic = HostContract::new();
    cyclic
        .register_type(
            "unity_engine::Object",
            Some("unity_engine::GameObject"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    cyclic
        .register_type(
            "unity_engine::GameObject",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    assert!(cyclic.to_manifest_bytes().unwrap_err().contains("cycle"));
}

#[test]
fn binary_v1_manifests_remain_loadable_and_upgrade_to_current_version() {
    let contract = example_contract();
    let legacy = encode_binary_manifest(&contract).unwrap();
    assert_eq!(u32::from_le_bytes(legacy[8..12].try_into().unwrap()), 1);
    let decoded = HostContract::from_manifest_bytes(&legacy).unwrap();
    assert_eq!(decoded, contract);
    assert_eq!(
        u32::from_le_bytes(
            decoded.to_manifest_bytes().unwrap()[8..12]
                .try_into()
                .unwrap()
        ),
        HOST_MANIFEST_FORMAT_VERSION
    );
}

#[test]
fn overloads_round_trip_and_mapped_signature_collisions_are_rejected() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            100,
            "unity_engine::math::pick",
            FunctionSignature::fixed(
                vec![Type::Integer(IntegerType::I32)],
                Type::Integer(IntegerType::I32),
            ),
            "unity.math",
        )
        .unwrap();
    contract
        .register_function(
            101,
            "unity_engine::math::pick",
            FunctionSignature::fixed(
                vec![Type::Float(FloatType::F32)],
                Type::Float(FloatType::F32),
            ),
            "unity.math",
        )
        .unwrap();

    assert_eq!(
        contract.functions_named("unity_engine::math::pick").count(),
        2
    );
    assert!(contract.function("unity_engine::math::pick").is_none());
    assert!(
        contract
            .register_function(
                102,
                "unity_engine::math::pick",
                FunctionSignature::fixed(vec![Type::Integer(IntegerType::I32)], Type::String,),
                "unity.math",
            )
            .unwrap_err()
            .contains("mapped parameter signature")
    );

    let bytes = contract.to_manifest_bytes().unwrap();
    let decoded = HostContract::from_manifest_bytes(&bytes).unwrap();
    assert_eq!(decoded, contract);
    let json = contract.to_manifest_json().unwrap();
    assert_eq!(HostContract::from_manifest_json(&json).unwrap(), contract);
}

#[test]
fn binary_manifest_verifier_rejects_unknown_type_after_valid_hash() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            2,
            "unity_engine::math::abs",
            FunctionSignature::fixed(
                vec![Type::Integer(IntegerType::I32)],
                Type::Integer(IntegerType::I32),
            ),
            "unity.math",
        )
        .unwrap();
    let mut manifest = contract.to_manifest_bytes().unwrap();
    // v5 appends a four-byte enum-variant count after the parameter table.
    let parameter_reference_high_byte = manifest.len() - 5;
    manifest[parameter_reference_high_byte] = 0xff;
    let hash = fnv1a128_parts(&[&manifest[..48], &manifest[HOST_MANIFEST_HEADER_SIZE..]]);
    manifest[48..HOST_MANIFEST_HEADER_SIZE].copy_from_slice(&hash.to_le_bytes());
    assert!(
        HostContract::from_manifest_bytes(&manifest)
            .unwrap_err()
            .contains("type reference")
    );
}

#[test]
fn manifest_hash_is_independent_of_registration_order() {
    let mut left = HostContract::new();
    left.register_function(
        2,
        "unity_engine::debug::enabled",
        FunctionSignature::fixed(Vec::new(), Type::Bool),
        "unity.debug",
    )
    .unwrap();
    left.register_function(
        1,
        "unity_engine::debug::flush",
        FunctionSignature::fixed(Vec::new(), Type::Unit),
        "unity.debug",
    )
    .unwrap();

    let mut right = HostContract::new();
    right
        .register_function(
            1,
            "unity_engine::debug::flush",
            FunctionSignature::fixed(Vec::new(), Type::Unit),
            "unity.debug",
        )
        .unwrap();
    right
        .register_function(
            2,
            "unity_engine::debug::enabled",
            FunctionSignature::fixed(Vec::new(), Type::Bool),
            "unity.debug",
        )
        .unwrap();

    assert_eq!(left.contract_hash(), right.contract_hash());
    assert_eq!(
        left.to_manifest_bytes().unwrap(),
        right.to_manifest_bytes().unwrap()
    );
    assert_eq!(
        left.to_manifest_json().unwrap(),
        right.to_manifest_json().unwrap()
    );
}

#[test]
fn fragments_merge_canonically_and_reject_conflicts() {
    let mut first = HostContract::new();
    first
        .register_function(
            1,
            "unity_engine::time::frame_count",
            FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
            "unity.time",
        )
        .unwrap();
    let mut second = HostContract::new();
    second
        .register_function(
            2,
            "game::score::get",
            FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::I32)),
            "game.score",
        )
        .unwrap();

    let mut left = first.clone();
    left.merge(&second).unwrap();
    let mut right = second.clone();
    right.merge(&first).unwrap();
    assert_eq!(
        left.to_manifest_bytes().unwrap(),
        right.to_manifest_bytes().unwrap()
    );
    left.merge(&first).unwrap();

    let mut conflicting = HostContract::new();
    conflicting
        .register_function(
            2,
            "game::score::other",
            FunctionSignature::fixed(Vec::new(), Type::Unit),
            "game.score",
        )
        .unwrap();
    assert!(second.merge(&conflicting).unwrap_err().contains("id 2"));
}

#[test]
fn manifest_rejects_unknown_fields_and_future_call_kinds() {
    let json = example_contract().to_manifest_json().unwrap();
    let unknown = json.replace("\"version\": 2", "\"version\": 2, \"typo\": true");
    assert!(
        HostContract::from_manifest_json(&unknown)
            .unwrap_err()
            .contains("unknown host module field")
    );
    let command = json.replace("\"direct\"", "\"command\"");
    assert!(
        HostContract::from_manifest_json(&command)
            .unwrap_err()
            .contains("unsupported host call kind")
    );
}

#[test]
fn bundled_manifest_example_is_valid() {
    let manifest = include_str!("../../../../examples/unity-host-manifest.json");
    let contract = HostContract::from_manifest_json(manifest).unwrap();
    assert_eq!(contract.functions().len(), 1);
    assert_eq!(contract.functions().next().unwrap().function_id, 100);
    assert!(
        contract
            .to_manifest_json()
            .unwrap()
            .contains("contract_hash")
    );
}
