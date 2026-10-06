use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    value::{
        EnumType, FieldSlot, StructFields, StructType,
        record_layout::{NativeLayoutProvider, RecordLayoutResolver},
    },
};
use rils_frontend::{
    Span,
    ast::{EnumVariant, GenericParameter, NamedField},
};
use rils_value::{DynamicLayout, DynamicValue};

fn definition(name: &str, generics: &[&str], fields: Vec<(&str, Type)>) -> Rc<StructType> {
    Rc::new(StructType {
        field_indices: Default::default(),
        name: name.into(),
        opaque_native: false,
        generic_parameters: generics
            .iter()
            .map(|name| GenericParameter {
                is_const: false,
                name: (*name).into(),
                bounds: vec![],
                span: Span::default(),
            })
            .collect(),
        fields: fields
            .into_iter()
            .map(|(name, type_annotation)| NamedField {
                name: name.into(),
                type_annotation,
                span: Span::default(),
            })
            .collect(),
        methods: RefCell::new(HashMap::new()),
        trait_methods: RefCell::new(HashMap::new()),
        implemented_traits: RefCell::new(HashSet::new()),
        associated_types: RefCell::new(HashMap::new()),
    })
}

fn named(name: &str, arguments: Vec<Type>) -> Type {
    Type::Named {
        name: name.into(),
        arguments,
    }
}

#[test]
fn nominal_layout_copy_requires_an_explicit_valid_declaration() {
    for (declared, field_type, expected) in [
        (false, Type::I32, Some(false)),
        (true, Type::I32, Some(true)),
        (false, Type::String, Some(false)),
        (true, Type::String, None),
    ] {
        let record = definition("Record", &[], vec![("field", field_type)]);
        if declared {
            record.implemented_traits.borrow_mut().insert("Copy".into());
        }
        let definitions = [record];
        let result = RecordLayoutResolver::new(&definitions).resolve(&Type::named("Record"));
        if let Some(copy) = expected {
            assert_eq!(result.unwrap().is_copy(), copy);
        } else {
            assert!(result.err().unwrap().contains("non-Copy fields"));
        }
    }
    let inner = definition("Inner", &[], vec![("field", Type::I32)]);
    let outer = definition("Outer", &[], vec![("inner", Type::named("Inner"))]);
    outer.implemented_traits.borrow_mut().insert("Copy".into());
    let definitions = [inner, outer];
    assert!(
        RecordLayoutResolver::new(&definitions)
            .resolve(&Type::named("Outer"))
            .err()
            .unwrap()
            .contains("non-Copy fields")
    );
}

struct VecLayoutProvider;

impl NativeLayoutProvider for VecLayoutProvider {
    fn layout(
        &self,
        ty: &Type,
        resolve_child: &mut dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String>,
    ) -> Option<Result<Rc<DynamicLayout>, String>> {
        let Type::Named { name, arguments } = ty else {
            return None;
        };
        if name != "Vec" || arguments.len() != 1 {
            return None;
        }
        Some(resolve_child(&arguments[0]).map(|item| DynamicLayout::sequence(ty.clone(), item)))
    }
}

#[test]
fn declaration_provider_resolves_nested_generic_collection_fields() {
    let definitions = vec![definition(
        "Boxed",
        &["T"],
        vec![("items", named("Vec", vec![Type::Variable("T".into())]))],
    )];
    let provider = VecLayoutProvider;
    let mut resolver = RecordLayoutResolver::with_provider(&definitions, &[], Some(&provider));
    let boxed = resolver.resolve(&named("Boxed", vec![Type::I32])).unwrap();
    let sequence = boxed.record_fields().unwrap()[0].layout();
    assert_eq!(sequence.sequence_item().unwrap().rils_type(), &Type::I32);
}

#[test]
fn resolves_generic_enum_payloads_as_tagged_native_layouts() {
    let enums = vec![Rc::new(EnumType {
        name: "Choice".into(),
        generic_parameters: vec![GenericParameter {
            is_const: false,
            name: "T".into(),
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
        methods: RefCell::new(HashMap::new()),
        trait_methods: RefCell::new(HashMap::new()),
        implemented_traits: RefCell::new(HashSet::new()),
        associated_types: RefCell::new(HashMap::new()),
    })];
    let structs = vec![definition(
        "Outer",
        &[],
        vec![("choice", named("Choice", vec![Type::I32]))],
    )];
    let mut resolver = RecordLayoutResolver::with_enums(&structs, &enums);
    let choice_type = named("Choice", vec![Type::I32]);
    let choice = resolver.resolve(&choice_type).unwrap();
    assert_eq!(choice.variant_alternatives().unwrap().len(), 2);
    let filled = choice.variant_alternatives().unwrap()[1].clone();
    let item = DynamicValue::from_rust(resolver.resolve(&Type::I32).unwrap(), 17_i32).unwrap();
    let payload = DynamicValue::record(filled, vec![item]).unwrap();
    let selected = DynamicValue::variant(choice, 1, payload).unwrap();
    let mut outer = DynamicValue::record(
        resolver.resolve(&Type::named("Outer")).unwrap(),
        vec![selected],
    )
    .unwrap();
    let (index, mut payload) = outer.take_field_path(&[0]).unwrap().take_variant().unwrap();
    assert_eq!(index, 1);
    assert_eq!(
        payload.take_field(0).unwrap().with::<i32, _>(|n| *n),
        Ok(17)
    );
}

#[test]
fn resolves_concrete_generic_struct_fields_from_runtime_declarations() {
    let declarations = vec![
        definition(
            "Holder",
            &["T"],
            vec![
                ("item", Type::Variable("T".into())),
                ("backup", Type::Option(Box::new(Type::Variable("T".into())))),
            ],
        ),
        definition(
            "Outer",
            &[],
            vec![("inner", named("Holder", vec![Type::I32]))],
        ),
    ];
    let mut resolver = RecordLayoutResolver::new(&declarations);
    let outer_type = Type::named("Outer");
    let outer = resolver.resolve(&outer_type).unwrap();
    assert!(Rc::ptr_eq(&outer, &resolver.resolve(&outer_type).unwrap()));
    let inner = resolver.resolve(&named("Holder", vec![Type::I32])).unwrap();
    assert_eq!(inner.record_field_index("item"), Some(0));
    assert_eq!(
        inner.record_fields().unwrap()[1].layout().rils_type(),
        &Type::Option(Box::new(Type::I32))
    );
    assert!(Rc::ptr_eq(
        &inner,
        &resolver.resolve(&named("Holder", vec![Type::I32])).unwrap()
    ));
    assert_eq!(
        outer.record_fields().unwrap()[0].layout().rils_type(),
        &named("Holder", vec![Type::I32])
    );

    let integer = resolver.resolve(&Type::I32).unwrap();
    let optional = resolver
        .resolve(&Type::Option(Box::new(Type::I32)))
        .unwrap();
    let inner_value = DynamicValue::record(
        inner,
        vec![
            DynamicValue::from_rust(integer, 23_i32).unwrap(),
            DynamicValue::none(optional).unwrap(),
        ],
    )
    .unwrap();
    let mut outer_value = DynamicValue::record(outer, vec![inner_value]).unwrap();
    assert_eq!(
        outer_value.with_field_path::<i32, _>(&[0, 0], |n| *n),
        Ok(23)
    );
    outer_value
        .with_field_path_mut::<i32, _>(&[0, 0], |n| *n += 1)
        .unwrap();
    let mut inner_value = outer_value.take_field(0).unwrap();
    assert_eq!(
        inner_value.take_field(0).unwrap().with::<i32, _>(|n| *n),
        Ok(24)
    );
}

#[test]
fn nests_tuples_and_arrays_in_record_bytes_without_value_slots() {
    let pair_type = Type::Tuple(vec![Type::I32, Type::String]);
    let array_type = Type::Array {
        element: Box::new(pair_type.clone()),
        length: 2,
    };
    let declarations = vec![definition(
        "Group",
        &[],
        vec![("items", array_type.clone())],
    )];
    let mut resolver = RecordLayoutResolver::new(&declarations);
    let integer = resolver.resolve(&Type::I32).unwrap();
    let string = resolver.resolve(&Type::String).unwrap();
    let tuple = resolver.resolve(&pair_type).unwrap();
    let array = resolver.resolve(&array_type).unwrap();
    let pair = |number, label: &str| {
        DynamicValue::record(
            tuple.clone(),
            vec![
                DynamicValue::from_rust(integer.clone(), number).unwrap(),
                DynamicValue::from_rust(
                    string.clone(),
                    rils_stdlib::stdlib::string::String::from(label.to_owned()),
                )
                .unwrap(),
            ],
        )
        .unwrap()
    };
    let items = DynamicValue::record(array, vec![pair(1, "one"), pair(2, "two")]).unwrap();
    let mut group = DynamicValue::record(
        resolver.resolve(&Type::named("Group")).unwrap(),
        vec![items],
    )
    .unwrap();
    assert_eq!(group.with_field_path::<i32, _>(&[0, 1, 0], |n| *n), Ok(2));
    group
        .with_field_path_mut::<i32, _>(&[0, 1, 0], |n| *n = 3)
        .unwrap();
    assert_eq!(group.with_field_path::<i32, _>(&[0, 1, 0], |n| *n), Ok(3));
    let moved = group.take_field_path(&[0, 1, 0]).unwrap();
    assert_eq!(moved.with::<i32, _>(|n| *n), Ok(3));
    assert!(group.with_field_path::<i32, _>(&[0, 1, 0], |n| *n).is_err());
    assert_eq!(group.with_field_path::<i32, _>(&[0, 0, 0], |n| *n), Ok(1));
    group.put_field_path(&[0, 1, 0], moved).unwrap();
    assert_eq!(group.with_field_path::<i32, _>(&[0, 1, 0], |n| *n), Ok(3));
    assert!(group.with_field_path::<u64, _>(&[0, 1, 0], |n| *n).is_err());
    assert!(group.with_field_path::<i32, _>(&[0, 2, 0], |n| *n).is_err());
}

#[test]
fn rejects_recursive_or_unregistered_field_layouts() {
    let declarations = vec![
        definition("Loop", &[], vec![("again", Type::named("Loop"))]),
        definition(
            "Unsupported",
            &[],
            vec![("items", Type::Slice(Box::new(Type::I32)))],
        ),
        definition(
            "Generic",
            &["T"],
            vec![("item", Type::Variable("T".into()))],
        ),
    ];
    let mut resolver = RecordLayoutResolver::new(&declarations);
    assert!(
        resolver
            .resolve(&Type::named("Loop"))
            .err()
            .unwrap()
            .contains("recursive inline native layout")
    );
    assert!(
        resolver
            .resolve(&Type::named("Unsupported"))
            .err()
            .unwrap()
            .contains("no native layout")
    );
    assert!(
        resolver
            .resolve(&Type::named("Generic"))
            .err()
            .unwrap()
            .contains("requires 1 type arguments")
    );
    assert!(
        resolver
            .resolve(&Type::named("Missing"))
            .err()
            .unwrap()
            .contains("no user struct or enum declaration")
    );
}

#[test]
fn runtime_record_slots_follow_declaration_order_and_reject_missing_fields() {
    let definition = definition(
        "Pair",
        &[],
        vec![("first", Type::I32), ("second", Type::I32)],
    );
    let slots = HashMap::from([
        (
            "second".into(),
            FieldSlot::new(Type::I32, Value::from_i32(2)),
        ),
        (
            "first".into(),
            FieldSlot::new(Type::I32, Value::from_i32(1)),
        ),
    ]);
    let fields = StructFields::from_map(definition.clone(), slots).unwrap();
    assert_eq!(definition.field_index("first"), Some(0));
    assert_eq!(definition.field_index("second"), Some(1));
    assert_eq!(fields.get_index(0).unwrap().value, Some(Value::from_i32(1)));
    assert_eq!(
        fields.get("second").unwrap().value,
        Some(Value::from_i32(2))
    );
    assert!(fields.get("missing").is_none());
    assert!(StructFields::from_map(definition, HashMap::new()).is_err());
}
