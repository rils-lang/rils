use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    value::{
        EnumType, HostObject, HostType, StructType,
        native_instance::{NativeInstancePlace, enum_variant},
        storage::TypedStorageContext,
    },
};
use rils_frontend::{ast::Stmt, lex, parse};
use rils_value::DynamicPathStep;

fn declarations() -> (Vec<Rc<StructType>>, Vec<Rc<EnumType>>) {
    let program = parse(
        lex(include_str!(
            "fixtures/native_construction/declarations.rils"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    for statement in program.statements {
        match statement {
            Stmt::Struct {
                name,
                generic_parameters,
                fields,
                ..
            } => structs.push(Rc::new(StructType {
                name,
                generic_parameters,
                fields,
                field_indices: Default::default(),
                opaque_native: false,
                methods: RefCell::default(),
                trait_methods: RefCell::default(),
                implemented_traits: RefCell::default(),
                associated_types: RefCell::default(),
            })),
            Stmt::Enum {
                name,
                generic_parameters,
                variants,
                ..
            } => enums.push(Rc::new(EnumType {
                host_definition: None,
                name,
                generic_parameters,
                variants,
                methods: RefCell::default(),
                trait_methods: RefCell::default(),
                implemented_traits: RefCell::default(),
                associated_types: RefCell::default(),
            })),
            _ => panic!("fixture contains only declarations"),
        }
    }
    (structs, enums)
}

fn generic(name: &str, argument: Type) -> Type {
    Type::Named {
        name: name.into(),
        arguments: vec![argument],
    }
}

fn place(value: &Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("constructor retained a legacy value")
    };
    NativeInstancePlace::new(object.clone()).unwrap()
}

#[test]
fn all_shapes_construct_native_instances_without_legacy_children() {
    let (structs, enums) = declarations();
    let context = TypedStorageContext::new(&structs, &enums);
    let ty = generic("Choice", Type::String);
    for variant in ["Empty", "Tuple", "Record"] {
        let value = match variant {
            "Empty" => context.construct_unit_variant(&ty, variant),
            "Tuple" => {
                context.construct_tuple_variant(&ty, variant, vec![Value::from_string("owned")])
            }
            _ => context.construct_record(
                &ty,
                Some(variant),
                HashMap::from([("item".into(), Value::from_string("owned"))]),
            ),
        }
        .unwrap();
        assert_eq!(Type::of_value(&value), Some(ty.clone()));
        assert!(
            !value.is_copy(),
            "Copy requires an explicit impl even for Empty"
        );
        let active = enum_variant(&value).unwrap().unwrap();
        assert_eq!(active.name(), variant);
        if variant != "Empty" {
            let field = if variant == "Tuple" { "0" } else { "item" };
            assert_eq!(
                place(&value)
                    .project(DynamicPathStep::Variant(active.index))
                    .unwrap()
                    .field(field)
                    .unwrap()
                    .take()
                    .unwrap()
                    .as_string()
                    .as_deref(),
                Some("owned")
            );
        }
    }
    let ty = generic("Phantom", Type::String);
    let value = context.construct_record(&ty, None, HashMap::new()).unwrap();
    assert_eq!(Type::of_value(&value), Some(ty));
    assert!(!value.is_copy());
}

#[test]
fn constructors_reject_unresolved_types_missing_fields_and_wrong_variant_shapes() {
    let (structs, enums) = declarations();
    let context = TypedStorageContext::new(&structs, &enums);
    for argument in [
        Type::Unknown,
        Type::Variable("T".into()),
        Type::Option(Box::new(Type::Unknown)),
    ] {
        assert!(
            context
                .construct_record(&generic("Phantom", argument), None, HashMap::new())
                .is_err()
        );
    }
    assert!(
        context
            .construct_record(&Type::named("Phantom"), None, HashMap::new())
            .is_err()
    );
    let holder = generic("Holder", Type::I32);
    for fields in [
        HashMap::new(),
        HashMap::from([("wrong".into(), Value::from_i32(42))]),
        HashMap::from([("item".into(), Value::from_string("wrong type"))]),
    ] {
        assert!(context.construct_record(&holder, None, fields).is_err());
    }
    let choice = generic("Choice", Type::I32);
    assert!(
        context
            .construct_record(&choice, None, HashMap::new())
            .is_err()
    );
    assert!(
        context
            .construct_record(&choice, Some("Tuple"), HashMap::new())
            .is_err()
    );
    assert!(
        context
            .construct_tuple_variant(&choice, "Empty", vec![])
            .is_err()
    );
    assert!(
        context
            .construct_tuple_variant(&choice, "Tuple", vec![])
            .is_err()
    );
    assert!(context.construct_unit_variant(&choice, "Record").is_err());
    assert!(context.construct_unit_variant(&choice, "Absent").is_err());
}

#[test]
fn owned_host_children_preserve_identity_and_drop_once_on_success_or_failure() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let (structs, enums) = declarations();
    let host = Rc::new(HostType {
        name: "Host".into(),
        base_types: HashSet::new(),
        copy: false,
        methods: RefCell::default(),
    });
    let hosts = [host.clone()];
    let context = TypedStorageContext::with_hosts(&structs, &enums, &hosts);
    for valid in [true, false] {
        let drops = Rc::new(Cell::new(0));
        let object = Rc::new(HostObject {
            type_definition: host.clone(),
            payload: Rc::new(Probe(drops.clone())),
        });
        let identity = Rc::as_ptr(&object);
        let other_drops = Rc::new(Cell::new(0));
        let second = if valid {
            Value::HostObject(Rc::new(HostObject {
                type_definition: host.clone(),
                payload: Rc::new(Probe(other_drops.clone())),
            }))
        } else {
            Value::from_i32(42)
        };
        let result = context.construct_record(
            &Type::named("Pair"),
            None,
            HashMap::from([
                ("first".into(), Value::HostObject(object)),
                ("second".into(), second),
            ]),
        );
        if valid {
            let value = result.unwrap();
            assert_eq!(drops.get(), 0);
            let moved = place(&value).field("first").unwrap().take().unwrap();
            let Value::HostObject(ref extracted) = moved else {
                panic!("host child")
            };
            assert_eq!(Rc::as_ptr(extracted), identity);
            drop(value);
            assert_eq!(drops.get(), 0);
            assert_eq!(other_drops.get(), 1);
            drop(moved);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn shared_non_copy_input_is_rejected_without_moving_original_children() {
    let (structs, enums) = declarations();
    let context = TypedStorageContext::new(&structs, &enums);
    let child_type = generic("Holder", Type::String);
    let child = context
        .construct_record(
            &child_type,
            None,
            HashMap::from([("item".into(), Value::from_string("original"))]),
        )
        .unwrap();
    assert!(
        context
            .construct_record(
                &generic("Holder", child_type),
                None,
                HashMap::from([("item".into(), child.clone())])
            )
            .is_err()
    );
    assert_eq!(
        place(&child)
            .field("item")
            .unwrap()
            .take()
            .unwrap()
            .as_string()
            .as_deref(),
        Some("original")
    );
}
