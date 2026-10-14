use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{
        EnumType, StructType, native_layouts, record_codec, record_layout::RecordLayoutResolver,
        storage::TypedStorageContext,
    },
};
use rils_frontend::{
    Span,
    ast::{EnumVariant, GenericParameter, NamedField},
};
use rils_value::{DynamicLayout, DynamicObject, DynamicPathStep, DynamicType, DynamicValue};

#[test]
fn opaque_native_generic_leaf_survives_an_optional_field() {
    struct Payload(i32);
    type Shared = rils_stdlib::stdlib::rc::Rc<Payload>;
    let item_type = Type::named("Payload");
    let shared_type = Type::Named {
        name: "Rc".into(),
        arguments: vec![item_type.clone()],
    };
    let shared_layout = DynamicLayout::of::<Shared>(shared_type.clone());
    assert!(shared_layout.is_rust_value());
    let shared = DynamicValue::from_rust(shared_layout.clone(), Shared::new(Payload(41))).unwrap();
    let object =
        DynamicObject::new(Rc::new(DynamicType::new(shared_layout.clone())), shared).unwrap();
    let optional_layout = DynamicLayout::option(shared_layout).unwrap();
    let optional = rils_execution::value::storage::TypedStorageContext::new(&[], &[])
        .construct_option(
            &Type::Option(Box::new(shared_type.clone())),
            Some(Value::Dynamic(object)),
        )
        .unwrap();
    let native = record_codec::into_native(optional, optional_layout).unwrap();
    let restored = record_codec::from_native(native).unwrap();
    assert!(matches!(restored, Value::Dynamic(_)));
    assert_eq!(
        Type::of_value(&restored),
        Some(Type::Option(Box::new(shared_type)))
    );
    let value = rils_execution::value::dynamic_option::take_owned(restored)
        .unwrap()
        .unwrap();
    let Value::Dynamic(object) = &value else {
        panic!("opaque native leaf must retain its dynamic handle");
    };
    let payload = object
        .with(|value| value.with::<Shared, _>(|shared| std::ops::Deref::deref(shared).as_ref().0))
        .unwrap()
        .unwrap();
    assert_eq!(payload, 41);
}

#[test]
fn generated_number_codecs_round_trip_owned_record_payloads() {
    let cases = [
        (
            Type::Integer(rils_execution::IntegerType::I8),
            Value::from(rils_execution::value::native_i8(-8)),
        ),
        (
            Type::I32,
            Value::from(rils_execution::value::native_i32(42)),
        ),
        (
            Type::USIZE,
            Value::from(rils_execution::value::native_usize(usize::MAX)),
        ),
        (
            Type::F64,
            Value::from(rils_execution::value::native_f64(3.5)),
        ),
    ];
    for (ty, value) in cases {
        let restored = if let Some(layout) = native_layouts::integer::layout(&ty) {
            let field = native_layouts::integer::option_item(&value, &ty, layout)
                .unwrap()
                .unwrap();
            native_layouts::integer::field_value(field, &ty)
                .unwrap()
                .unwrap()
        } else {
            let layout = native_layouts::float::layout(&ty).unwrap();
            let field = native_layouts::float::option_item(&value, &ty, layout)
                .unwrap()
                .unwrap();
            native_layouts::float::field_value(field, &ty)
                .unwrap()
                .unwrap()
        };
        assert_eq!(restored, value, "round trip {ty}");
    }
}

#[test]
fn owned_composite_codec_moves_nested_string_without_value_in_native_bytes() {
    let array_type = Type::Array {
        element: Box::new(Type::Bool),
        length: 2,
    };
    let result_type = Type::Result(Box::new(array_type.clone()), Box::new(Type::String));
    let optional_type = Type::Option(Box::new(Type::String));
    let tuple_type = Type::Tuple(vec![Type::I32, optional_type.clone(), result_type.clone()]);
    let make = || {
        let context = TypedStorageContext::new(&[], &[]);
        let flags = context
            .construct_array(
                &array_type,
                vec![
                    Value::from(rils_execution::value::native_bool(true)),
                    Value::from(rils_execution::value::native_bool(false)),
                ],
            )
            .unwrap();
        let optional = context
            .construct_option(
                &optional_type,
                Some(Value::from(rils_execution::value::native_string("owned"))),
            )
            .unwrap();
        let result = context.construct_result(&result_type, Ok(flags)).unwrap();
        context
            .construct_tuple(
                &tuple_type,
                vec![
                    Value::from(rils_execution::value::native_i32(7)),
                    optional,
                    result,
                ],
            )
            .unwrap()
    };
    let mut resolver = RecordLayoutResolver::new(&[]);
    let layout = resolver.resolve(&tuple_type).unwrap();
    let native = record_codec::into_native(make(), layout).unwrap();
    assert_eq!(native.descriptor().rils_type(), &tuple_type);
    let restored = record_codec::from_native(native).unwrap();
    assert_eq!(restored, make());
}

#[test]
fn owned_composite_codec_rejects_shared_noncopy_items() {
    let ty = Type::Option(Box::new(Type::String));
    let value = TypedStorageContext::new(&[], &[])
        .construct_option(
            &ty,
            Some(Value::from(rils_execution::value::native_string("shared"))),
        )
        .unwrap();
    let alias = value.clone();
    assert!(
        TypedStorageContext::new(&[], &[])
            .construct_tuple(&Type::Tuple(vec![ty]), vec![value])
            .is_err()
    );
    drop(alias);
}

fn definition(name: &str, fields: Vec<(&str, Type)>) -> Rc<StructType> {
    Rc::new(StructType {
        name: name.into(),
        opaque_native: false,
        generic_parameters: vec![],
        fields: fields
            .into_iter()
            .map(|(name, type_annotation)| NamedField {
                name: name.into(),
                type_annotation,
                span: Span::default(),
            })
            .collect(),
        field_indices: Default::default(),
        methods: RefCell::default(),
        trait_methods: RefCell::default(),
        implemented_traits: RefCell::default(),
        associated_types: RefCell::default(),
    })
}

fn instance(definition: Rc<StructType>, values: Vec<Value>) -> Value {
    instance_with_declarations(definition.clone(), values, &[definition])
}

fn instance_with_declarations(
    definition: Rc<StructType>,
    values: Vec<Value>,
    declarations: &[Rc<StructType>],
) -> Value {
    let fields = definition
        .fields
        .iter()
        .zip(values)
        .map(|(field, value)| (field.name.clone(), value))
        .collect();
    TypedStorageContext::new(declarations, &[])
        .construct_record(&Type::named(&definition.name), None, fields)
        .unwrap()
}

#[test]
fn nested_user_struct_fields_are_inline_in_native_bytes() {
    let inner = definition("Inner", vec![("text", Type::String), ("count", Type::I32)]);
    let outer = definition(
        "Outer",
        vec![
            ("inner", Type::named("Inner")),
            ("backup", Type::Option(Box::new(Type::named("Inner")))),
        ],
    );
    let make_inner = |text| {
        instance(
            inner.clone(),
            vec![
                Value::from(rils_execution::value::native_string(text)),
                Value::from(rils_execution::value::native_i32(7)),
            ],
        )
    };
    let make_outer = || {
        instance_with_declarations(
            outer.clone(),
            vec![
                make_inner("first"),
                rils_execution::value::storage::TypedStorageContext::new(&[], &[])
                    .construct_option(
                        &Type::Option(Box::new(Type::named("Inner"))),
                        Some(make_inner("second")),
                    )
                    .unwrap(),
            ],
            &[inner.clone(), outer.clone()],
        )
    };
    let declarations = vec![inner.clone(), outer.clone()];
    let mut resolver = RecordLayoutResolver::new(&declarations);
    let layout = resolver.resolve(&Type::named("Outer")).unwrap();
    let mut codec = record_codec::NativeRecordCodec::new();
    let native = codec.into_native(make_outer(), layout).unwrap();
    let text = native
        .with_field_path::<rils_stdlib::stdlib::string::String, _>(&[0, 0], |text| {
            std::string::String::from(text.clone())
        })
        .unwrap();
    assert_eq!(text, "first");
    assert_eq!(codec.from_native(native).unwrap(), make_outer());
}

#[test]
fn generic_enum_record_payload_keeps_nested_struct_inline() {
    let inner = definition("Item", vec![("text", Type::String)]);
    let choice = Rc::new(EnumType {
        host_definition: None,
        name: "Choice".into(),
        generic_parameters: vec![GenericParameter {
            name: "T".into(),
            is_const: false,
            bounds: vec![],
            span: Span::default(),
        }],
        variants: vec![
            EnumVariant::Unit {
                name: "Empty".into(),
                span: Span::default(),
            },
            EnumVariant::Record {
                name: "Filled".into(),
                fields: vec![NamedField {
                    name: "item".into(),
                    type_annotation: Type::Variable("T".into()),
                    span: Span::default(),
                }],
                span: Span::default(),
            },
        ],
        methods: RefCell::default(),
        trait_methods: RefCell::default(),
        implemented_traits: RefCell::default(),
        associated_types: RefCell::default(),
    });
    let structs = vec![inner.clone()];
    let enums = vec![choice.clone()];
    let ty = Type::Named {
        name: "Choice".into(),
        arguments: vec![Type::named("Item")],
    };
    let make = || {
        TypedStorageContext::new(&structs, &enums)
            .construct_record(
                &ty,
                Some("Filled"),
                HashMap::from([(
                    "item".into(),
                    instance(
                        inner.clone(),
                        vec![Value::from(rils_execution::value::native_string("inside"))],
                    ),
                )]),
            )
            .unwrap()
    };
    let mut resolver = RecordLayoutResolver::with_enums(&structs, &enums);
    let layout = resolver.resolve(&ty).unwrap();
    let mut codec = record_codec::NativeRecordCodec::new();
    let native = codec.into_native(make(), layout).unwrap();
    let path = [
        DynamicPathStep::Variant(1),
        DynamicPathStep::Field(0),
        DynamicPathStep::Field(0),
    ];
    let text = native
        .with_path::<rils_stdlib::stdlib::string::String, _>(&path, |text| {
            std::string::String::from(text.clone())
        })
        .unwrap();
    assert_eq!(text, "inside");
    assert_eq!(codec.from_native(native).unwrap(), make());
}

#[test]
fn generic_struct_fields_use_concrete_native_layouts() {
    let holder = Rc::new(StructType {
        name: "Holder".into(),
        opaque_native: false,
        generic_parameters: vec![GenericParameter {
            name: "T".into(),
            is_const: false,
            bounds: vec![],
            span: Span::default(),
        }],
        fields: vec![NamedField {
            name: "item".into(),
            type_annotation: Type::Variable("T".into()),
            span: Span::default(),
        }],
        field_indices: Default::default(),
        methods: RefCell::default(),
        trait_methods: RefCell::default(),
        implemented_traits: RefCell::default(),
        associated_types: RefCell::default(),
    });
    let declarations = vec![holder.clone()];
    let ty = Type::Named {
        name: "Holder".into(),
        arguments: vec![Type::String],
    };
    let make = || {
        TypedStorageContext::new(&declarations, &[])
            .construct_record(
                &ty,
                None,
                HashMap::from([(
                    "item".into(),
                    Value::from(rils_execution::value::native_string("generic")),
                )]),
            )
            .unwrap()
    };
    let mut resolver = RecordLayoutResolver::new(&declarations);
    let layout = resolver.resolve(&ty).unwrap();
    let mut codec = record_codec::NativeRecordCodec::new();
    let native = codec.into_native(make(), layout).unwrap();
    assert_eq!(
        native.descriptor().record_fields().unwrap()[0]
            .layout()
            .rils_type(),
        &Type::String
    );
    assert_eq!(codec.from_native(native).unwrap(), make());
}

#[test]
fn stdlib_vec_declaration_registers_nested_native_record_layout() {
    let inner = definition("Element", vec![("text", Type::String)]);
    let element_type = Type::named("Element");
    let ty = Type::Named {
        name: "Vec".into(),
        arguments: vec![element_type.clone()],
    };
    let make = || {
        sequence(
            &ty,
            vec![
                instance(
                    inner.clone(),
                    vec![Value::from(rils_execution::value::native_string("first"))],
                ),
                instance(
                    inner.clone(),
                    vec![Value::from(rils_execution::value::native_string("second"))],
                ),
            ],
            std::slice::from_ref(&inner),
        )
    };
    let definitions = vec![inner.clone()];
    let mut resolver = RecordLayoutResolver::new(&definitions);
    let layout = resolver.resolve(&ty).unwrap();
    assert_eq!(layout.sequence_item().unwrap().rils_type(), &element_type);
    let mut codec = record_codec::NativeRecordCodec::new();
    let mut native = codec.into_native(make(), layout).unwrap();
    assert_eq!(native.sequence_len(), Ok(2));
    let path = [DynamicPathStep::Index(1), DynamicPathStep::Field(0)];
    let text = native
        .with_path::<rils_stdlib::stdlib::string::String, _>(&path, |text| {
            std::string::String::from(text.clone())
        })
        .unwrap();
    assert_eq!(text, "second");
    let moved = native.take_path_field(&path).unwrap();
    assert_eq!(
        codec.from_native(moved).unwrap(),
        Value::from(rils_execution::value::native_string("second"))
    );
    assert!(
        codec
            .replace_path_field(
                &mut native,
                &path,
                Value::from(rils_execution::value::native_string("third"))
            )
            .unwrap()
            .is_none()
    );
    let replaced = codec
        .replace_path_field(
            &mut native,
            &path,
            Value::from(rils_execution::value::native_string("second")),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        codec.from_native(replaced).unwrap(),
        Value::from(rils_execution::value::native_string("third"))
    );
    let Value::Dynamic(restored) = codec.from_native(native).unwrap() else {
        panic!("decoded Vec should retain native storage");
    };
    assert_eq!(restored.with(|value| value.sequence_len()), Ok(Ok(2)));
    let first = restored.with(|value| {
        value.with_path::<rils_stdlib::stdlib::string::String, _>(
            &[DynamicPathStep::Index(0), DynamicPathStep::Field(0)],
            |text| std::string::String::from(text.clone()),
        )
    });
    assert_eq!(first, Ok(Ok("first".into())));
}

#[test]
fn stdlib_vecdeque_layout_round_trips_nested_optional_records() {
    let item = definition("QueuedItem", vec![("text", Type::String)]);
    let option_type = Type::Option(Box::new(Type::named("QueuedItem")));
    let ty = Type::Named {
        name: "VecDeque".into(),
        arguments: vec![option_type.clone()],
    };
    let make = || {
        let definitions = [item.clone()];
        let context = TypedStorageContext::new(&definitions, &[]);
        sequence(
            &ty,
            vec![
                context
                    .construct_option(
                        &option_type,
                        Some(instance(
                            item.clone(),
                            vec![Value::from(rils_execution::value::native_string("front"))],
                        )),
                    )
                    .unwrap(),
                context.construct_option(&option_type, None).unwrap(),
            ],
            &definitions,
        )
    };
    let declarations = vec![item.clone()];
    let mut resolver = RecordLayoutResolver::new(&declarations);
    let layout = resolver.resolve(&ty).unwrap();
    assert!(native_layouts::vec_deque::matches(&ty));
    assert_eq!(layout.sequence_item().unwrap().rils_type(), &option_type);
    let mut codec = record_codec::NativeRecordCodec::new();
    let native = codec.into_native(make(), layout).unwrap();
    let path = [
        DynamicPathStep::Index(0),
        DynamicPathStep::Some,
        DynamicPathStep::Field(0),
    ];
    assert_eq!(
        native.with_path::<rils_stdlib::stdlib::string::String, _>(&path, |text| {
            std::string::String::from(text.clone())
        }),
        Ok("front".into())
    );
    let Value::Dynamic(restored) = codec.from_native(native).unwrap() else {
        panic!("VecDeque should retain native storage")
    };
    assert_eq!(restored.descriptor().layout().rils_type(), &ty);
    assert_eq!(restored.with(|value| value.sequence_len()), Ok(Ok(2)));
    assert_eq!(
        restored.with(|value| value.view().sequence_item(0)?.option_is_some()),
        Ok(Ok(true))
    );
    assert_eq!(
        restored.with(|value| value.view().sequence_item(1)?.option_is_some()),
        Ok(Ok(false))
    );
}

#[test]
fn stdlib_binary_heap_layout_keeps_owned_elements_and_empty_type() {
    let ty = Type::Named {
        name: "BinaryHeap".into(),
        arguments: vec![Type::I32],
    };
    let mut resolver = RecordLayoutResolver::new(&[]);
    let layout = resolver.resolve(&ty).unwrap();
    assert!(native_layouts::binary_heap::matches(&ty));
    for values in [
        vec![],
        vec![
            Value::from(rils_execution::value::native_i32(7)),
            Value::from(rils_execution::value::native_i32(-4)),
            Value::from(rils_execution::value::native_i32(2)),
        ],
    ] {
        let expected_len = values.len();
        let heap = sequence(&ty, values, &[]);
        let mut codec = record_codec::NativeRecordCodec::new();
        let native = codec.into_native(heap, layout.clone()).unwrap();
        let Value::Dynamic(restored) = codec.from_native(native).unwrap() else {
            panic!("BinaryHeap should retain native storage")
        };
        assert_eq!(restored.descriptor().layout().rils_type(), &ty);
        assert_eq!(
            restored.with(|value| value.sequence_len()),
            Ok(Ok(expected_len))
        );
    }
}

#[test]
fn standard_collection_fields_round_trip_without_value_payloads() {
    let option_type = Type::Option(Box::new(Type::I32));
    let cases = [
        (
            "HashSet",
            vec![Type::I32],
            Value::from(rils_execution::value::native_i32(3)),
        ),
        (
            "BTreeSet",
            vec![Type::String],
            Value::from(rils_execution::value::native_string("key")),
        ),
        (
            "HashMap",
            vec![Type::String, option_type.clone()],
            TypedStorageContext::new(&[], &[])
                .construct_tuple(
                    &Type::Tuple(vec![Type::String, option_type.clone()]),
                    vec![
                        Value::from(rils_execution::value::native_string("key")),
                        TypedStorageContext::new(&[], &[])
                            .construct_option(
                                &option_type,
                                Some(Value::from(rils_execution::value::native_i32(9))),
                            )
                            .unwrap(),
                    ],
                )
                .unwrap(),
        ),
        (
            "BTreeMap",
            vec![Type::I32, Type::Bool],
            TypedStorageContext::new(&[], &[])
                .construct_tuple(
                    &Type::Tuple(vec![Type::I32, Type::Bool]),
                    vec![
                        Value::from(rils_execution::value::native_i32(4)),
                        Value::from(rils_execution::value::native_bool(true)),
                    ],
                )
                .unwrap(),
        ),
    ]
    .map(|(name, arguments, entry)| {
        let ty = Type::Named {
            name: name.into(),
            arguments,
        };
        let value = sequence(&ty, vec![entry], &[]);
        (ty, value)
    });
    for (ty, value) in cases {
        let mut resolver = RecordLayoutResolver::new(&[]);
        let layout = resolver.resolve(&ty).unwrap();
        assert!(layout.sequence_item().is_some());
        let mut codec = record_codec::NativeRecordCodec::new();
        let native = codec.into_native(value, layout).unwrap();
        assert_eq!(native.sequence_len(), Ok(1));
        let Value::Dynamic(restored) = codec.from_native(native).unwrap() else {
            panic!("{ty} should retain native storage");
        };
        assert_eq!(restored.descriptor().layout().rils_type(), &ty);
        assert_eq!(restored.with(|value| value.sequence_len()), Ok(Ok(1)));
    }
}

#[test]
fn nominal_codec_rejects_a_layout_with_swapped_field_names() {
    let pair = definition("Pair", vec![("first", Type::I32), ("second", Type::I32)]);
    let integer = native_layouts::integer::layout(&Type::I32).unwrap();
    let wrong = rils_value::DynamicLayout::record(
        Type::named("Pair"),
        vec![
            ("second".into(), integer.clone()),
            ("first".into(), integer),
        ],
    )
    .unwrap();
    let value = instance(
        pair,
        vec![
            Value::from(rils_execution::value::native_i32(1)),
            Value::from(rils_execution::value::native_i32(2)),
        ],
    );
    let error = record_codec::NativeRecordCodec::new()
        .into_native(value, wrong)
        .err()
        .unwrap();
    assert!(error.contains("different native layout"));
}

#[test]
fn decoded_tuple_preserves_nominal_identity_and_explicit_copy_policy() {
    let item = definition("Item", vec![("number", Type::I32)]);
    let declarations = [item.clone()];
    let context = TypedStorageContext::new(&declarations, &[]);
    let ty = Type::Tuple(vec![Type::named("Item")]);
    let tuple = context
        .construct_tuple(
            &ty,
            vec![instance(
                item.clone(),
                vec![Value::from(rils_execution::value::native_i32(7))],
            )],
        )
        .unwrap();
    let optional = context
        .construct_option(&Type::Option(Box::new(ty)), Some(tuple))
        .unwrap();
    let tuple = rils_execution::value::dynamic_option::take_owned(optional)
        .unwrap()
        .unwrap();
    assert!(!tuple.is_copy()); // All fields are Copy, but Item did not implement Copy.
    let Value::Dynamic(object) = tuple else {
        panic!("native tuple")
    };
    let child = rils_execution::value::native_instance::NativeInstancePlace::new(object)
        .unwrap()
        .field("0")
        .unwrap()
        .take()
        .unwrap();
    let Value::Dynamic(child) = child else {
        panic!("native record")
    };
    let Some(rils_execution::declaration::Declaration::StructType(restored)) =
        rils_execution::value::native_instance::definition(&child)
    else {
        panic!("retained declaration")
    };
    assert!(Rc::ptr_eq(&restored, &item));
}

#[test]
fn empty_native_arrays_keep_transitive_element_declarations_without_runtime_context() {
    let inner = definition("Inner", vec![("number", Type::I32)]);
    let item = definition("Item", vec![("child", Type::named("Inner"))]);
    let definitions = [inner, item.clone()];
    let ty = Type::Array {
        element: Box::new(Type::named("Item")),
        length: 0,
    };
    let context = TypedStorageContext::new(&definitions, &[]);
    let payload = DynamicValue::record(context.layout(&ty).unwrap(), vec![]).unwrap();
    let value = record_codec::NativeRecordCodec::with_definitions(&definitions, &[])
        .from_native(payload)
        .unwrap();
    let option = context
        .construct_option(&Type::Option(Box::new(ty.clone())), Some(value))
        .unwrap();
    let value = rils_execution::value::dynamic_option::take_owned(option)
        .unwrap()
        .unwrap();
    assert_eq!(Type::of_value(&value), Some(ty));
    let symbol = rils_builtins::builtin_member("Vec", "from")
        .unwrap()
        .native_symbol
        .unwrap();
    let converted = rils_execution::runtime_builtins::call_native_owned_symbol(
        symbol,
        vec![value],
        &Default::default(),
    )
    .unwrap()
    .unwrap();
    let Value::Dynamic(object) = converted else {
        panic!("native Vec")
    };
    let layout = object.descriptor().layout().sequence_item().unwrap();
    assert_eq!(layout.rils_type(), &Type::named("Item"));
    assert!(!layout.is_copy());
    assert_eq!(
        layout.record_fields().unwrap()[0].layout().rils_type(),
        &Type::named("Inner")
    );
}

fn sequence(ty: &Type, values: Vec<Value>, definitions: &[Rc<StructType>]) -> Value {
    let layout = RecordLayoutResolver::new(definitions).resolve(ty).unwrap();
    let mut codec = record_codec::NativeRecordCodec::with_definitions(definitions, &[]);
    let items = values
        .into_iter()
        .map(|v| {
            codec
                .into_native(v, layout.sequence_item().unwrap().clone())
                .unwrap()
        })
        .collect();
    let value = DynamicValue::sequence(layout, items).unwrap();
    rils_execution::value::native_instance::from_native(value, Rc::new(codec)).unwrap()
}
