use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    value::{FieldSlot, StructFields, StructType, record_layout::RecordLayoutResolver},
};
use rils_frontend::{
    Span,
    ast::{GenericParameter, NamedField},
};
use rils_value::DynamicValue;

fn definition(name: &str, generics: &[&str], fields: Vec<(&str, Type)>) -> Rc<StructType> {
    Rc::new(StructType {
        field_indices: Default::default(),
        name: name.into(),
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
    let mut inner_value = outer_value.take_field(0).unwrap();
    assert_eq!(
        inner_value.take_field(0).unwrap().with::<i32, _>(|n| *n),
        Ok(23)
    );
}

#[test]
fn rejects_recursive_or_unregistered_field_layouts() {
    let declarations = vec![
        definition("Loop", &[], vec![("again", Type::named("Loop"))]),
        definition(
            "Unsupported",
            &[],
            vec![("items", Type::Tuple(vec![Type::I32]))],
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
            .contains("no user struct declaration")
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
            FieldSlot {
                value: Some(Value::I32(2)),
                type_annotation: Type::I32,
                references: 0,
            },
        ),
        (
            "first".into(),
            FieldSlot {
                value: Some(Value::I32(1)),
                type_annotation: Type::I32,
                references: 0,
            },
        ),
    ]);
    let fields = StructFields::from_map(definition.clone(), slots).unwrap();
    assert_eq!(definition.field_index("first"), Some(0));
    assert_eq!(definition.field_index("second"), Some(1));
    assert_eq!(fields.get_index(0).unwrap().value, Some(Value::I32(1)));
    assert_eq!(fields.get("second").unwrap().value, Some(Value::I32(2)));
    assert!(fields.get("missing").is_none());
    assert!(StructFields::from_map(definition, HashMap::new()).is_err());
}
