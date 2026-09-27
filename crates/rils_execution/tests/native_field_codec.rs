use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    value::{
        BTreeMapValue, BTreeSetValue, BinaryHeapValue, EnumInstance, EnumPayload, EnumType,
        FieldSlot, HashKey, HashMapValue, HashSetValue, IndexedStorage, StructFields,
        StructInstance, StructType, VecDequeValue, native_layouts, record_codec,
        record_layout::RecordLayoutResolver,
    },
};
use rils_frontend::{
    Span,
    ast::{EnumVariant, GenericParameter, NamedField},
};
use rils_value::DynamicPathStep;

#[test]
fn generated_number_codecs_round_trip_owned_record_payloads() {
    let cases = [
        (
            Type::Integer(rils_execution::IntegerType::I8),
            Value::from_i8(-8),
        ),
        (Type::I32, Value::I32(42)),
        (Type::USIZE, Value::Usize(usize::MAX)),
        (Type::F64, Value::F64(3.5)),
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

fn indexed(values: Vec<(Value, Type)>, element_type: Option<Type>) -> Rc<IndexedStorage> {
    Rc::new(IndexedStorage {
        elements: RefCell::new(
            values
                .into_iter()
                .map(|(value, type_annotation)| FieldSlot {
                    value: Some(value),
                    type_annotation,
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(element_type),
        active_iterators: Default::default(),
    })
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
        let flags = Value::Array(indexed(
            vec![
                (Value::Bool(true), Type::Bool),
                (Value::Bool(false), Type::Bool),
            ],
            Some(Type::Bool),
        ));
        Value::Tuple(indexed(
            vec![
                (Value::I32(7), Type::I32),
                (
                    Value::Option {
                        value: Some(Rc::new(Value::from_string("owned"))),
                        element_type: Some(Type::String),
                    },
                    optional_type.clone(),
                ),
                (
                    Value::Result {
                        value: Ok(Rc::new(flags)),
                        ok_type: Some(array_type.clone()),
                        error_type: Some(Type::String),
                    },
                    result_type.clone(),
                ),
            ],
            None,
        ))
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
    let mut resolver = RecordLayoutResolver::new(&[]);
    let ty = Type::Option(Box::new(Type::String));
    let layout = resolver.resolve(&ty).unwrap();
    let item = Rc::new(Value::from_string("shared"));
    let alias = item.clone();
    let value = Value::Option {
        value: Some(item),
        element_type: Some(Type::String),
    };
    assert!(record_codec::into_native(value, layout).is_err());
    drop(alias);
}

fn definition(name: &str, fields: Vec<(&str, Type)>) -> Rc<StructType> {
    Rc::new(StructType {
        name: name.into(),
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
    let slots = definition
        .fields
        .iter()
        .zip(values)
        .map(|(field, value)| {
            (
                field.name.clone(),
                FieldSlot {
                    value: Some(value),
                    type_annotation: field.type_annotation.clone(),
                    references: 0,
                },
            )
        })
        .collect();
    Value::Struct(Rc::new(StructInstance {
        fields: RefCell::new(StructFields::from_map(definition.clone(), slots).unwrap()),
        type_definition: definition,
        type_arguments: vec![],
    }))
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
    let make_inner = |text| instance(inner.clone(), vec![Value::from_string(text), Value::I32(7)]);
    let make_outer = || {
        instance(
            outer.clone(),
            vec![
                make_inner("first"),
                Value::Option {
                    value: Some(Rc::new(make_inner("second"))),
                    element_type: Some(Type::named("Inner")),
                },
            ],
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
    let make = || {
        Value::Enum(Rc::new(EnumInstance {
            type_definition: choice.clone(),
            variant: "Filled".into(),
            payload: EnumPayload::Record(std::collections::HashMap::from([(
                "item".into(),
                instance(inner.clone(), vec![Value::from_string("inside")]),
            )])),
            type_arguments: vec![Type::named("Item")],
        }))
    };
    let structs = vec![inner.clone()];
    let enums = vec![choice.clone()];
    let mut resolver = RecordLayoutResolver::with_enums(&structs, &enums);
    let ty = Type::Named {
        name: "Choice".into(),
        arguments: vec![Type::named("Item")],
    };
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
    let make = || {
        let slots = std::collections::HashMap::from([(
            "item".into(),
            FieldSlot {
                value: Some(Value::from_string("generic")),
                type_annotation: Type::String,
                references: 0,
            },
        )]);
        Value::Struct(Rc::new(StructInstance {
            type_definition: holder.clone(),
            fields: RefCell::new(StructFields::from_map(holder.clone(), slots).unwrap()),
            type_arguments: vec![Type::String],
        }))
    };
    let declarations = vec![holder.clone()];
    let ty = Type::Named {
        name: "Holder".into(),
        arguments: vec![Type::String],
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
        Value::Vec(indexed(
            vec![
                (
                    instance(inner.clone(), vec![Value::from_string("first")]),
                    element_type.clone(),
                ),
                (
                    instance(inner.clone(), vec![Value::from_string("second")]),
                    element_type.clone(),
                ),
            ],
            Some(element_type.clone()),
        ))
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
        Value::from_string("second")
    );
    assert!(
        codec
            .replace_path_field(&mut native, &path, Value::from_string("third"))
            .unwrap()
            .is_none()
    );
    let replaced = codec
        .replace_path_field(&mut native, &path, Value::from_string("second"))
        .unwrap()
        .unwrap();
    assert_eq!(
        codec.from_native(replaced).unwrap(),
        Value::from_string("third")
    );
    assert_eq!(codec.from_native(native).unwrap(), make());
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
        Value::VecDeque(Rc::new(VecDequeValue {
            elements: RefCell::new(VecDeque::from([
                Value::Option {
                    value: Some(Rc::new(instance(
                        item.clone(),
                        vec![Value::from_string("front")],
                    ))),
                    element_type: Some(Type::named("QueuedItem")),
                },
                Value::Option {
                    value: None,
                    element_type: Some(Type::named("QueuedItem")),
                },
            ])),
            element_type: RefCell::new(Some(option_type.clone())),
        }))
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
    let Value::VecDeque(restored) = codec.from_native(native).unwrap() else {
        panic!("VecDeque round trip")
    };
    assert_eq!(*restored.element_type.borrow(), Some(option_type));
    let elements = restored.elements.borrow();
    assert_eq!(elements.len(), 2);
    assert!(matches!(&elements[0], Value::Option { value: Some(_), .. }));
    assert!(matches!(&elements[1], Value::Option { value: None, .. }));
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
    for values in [vec![], vec![Value::I32(7), Value::I32(-4), Value::I32(2)]] {
        let expected = values.clone();
        let heap = Value::BinaryHeap(Rc::new(BinaryHeapValue {
            elements: RefCell::new(values),
            element_type: RefCell::new(Some(Type::I32)),
        }));
        let mut codec = record_codec::NativeRecordCodec::new();
        let native = codec.into_native(heap, layout.clone()).unwrap();
        let Value::BinaryHeap(restored) = codec.from_native(native).unwrap() else {
            panic!("BinaryHeap round trip")
        };
        assert_eq!(*restored.element_type.borrow(), Some(Type::I32));
        assert_eq!(*restored.elements.borrow(), expected);
    }
}

#[test]
fn standard_collection_fields_round_trip_without_value_payloads() {
    let option_type = Type::Option(Box::new(Type::I32));
    let some = Value::Option {
        value: Some(Rc::new(Value::I32(9))),
        element_type: Some(Type::I32),
    };
    let cases = vec![
        (
            Type::Named {
                name: "HashSet".into(),
                arguments: vec![Type::I32],
            },
            Value::HashSet(Rc::new(HashSetValue {
                borrowed: Cell::new(0),
                entries: RefCell::new(HashSet::from(
                    [HashKey::from_value(&Value::I32(3)).unwrap()],
                )),
                element_type: RefCell::new(Type::I32),
            })),
        ),
        (
            Type::Named {
                name: "BTreeSet".into(),
                arguments: vec![Type::String],
            },
            Value::BTreeSet(Rc::new(BTreeSetValue {
                borrowed: Cell::new(0),
                entries: RefCell::new(BTreeSet::from([HashKey::from_ordered_value(
                    &Value::from_string("key"),
                )
                .unwrap()])),
                element_type: RefCell::new(Type::String),
            })),
        ),
        (
            Type::Named {
                name: "HashMap".into(),
                arguments: vec![Type::String, option_type.clone()],
            },
            Value::HashMap(Rc::new(HashMapValue {
                borrowed: Cell::new(0),
                entries: RefCell::new(HashMap::from([(
                    HashKey::from_value(&Value::from_string("key")).unwrap(),
                    FieldSlot {
                        value: Some(some),
                        type_annotation: option_type.clone(),
                        references: 0,
                    },
                )])),
                key_type: RefCell::new(Type::String),
                value_type: RefCell::new(option_type),
            })),
        ),
        (
            Type::Named {
                name: "BTreeMap".into(),
                arguments: vec![Type::I32, Type::Bool],
            },
            Value::BTreeMap(Rc::new(BTreeMapValue {
                borrowed: Cell::new(0),
                entries: RefCell::new(BTreeMap::from([(
                    HashKey::from_ordered_value(&Value::I32(4)).unwrap(),
                    FieldSlot {
                        value: Some(Value::Bool(true)),
                        type_annotation: Type::Bool,
                        references: 0,
                    },
                )])),
                key_type: RefCell::new(Type::I32),
                value_type: RefCell::new(Type::Bool),
            })),
        ),
    ];
    for (ty, value) in cases {
        let expected = value.clone_owned().unwrap();
        let mut resolver = RecordLayoutResolver::new(&[]);
        let layout = resolver.resolve(&ty).unwrap();
        assert!(layout.sequence_item().is_some());
        let mut codec = record_codec::NativeRecordCodec::new();
        let native = codec.into_native(value, layout).unwrap();
        assert_eq!(native.sequence_len(), Ok(1));
        assert_eq!(codec.from_native(native).unwrap(), expected, "{ty}");
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
    let value = instance(pair, vec![Value::I32(1), Value::I32(2)]);
    let error = record_codec::NativeRecordCodec::new()
        .into_native(value, wrong)
        .err()
        .unwrap();
    assert!(error.contains("field order"));
}
