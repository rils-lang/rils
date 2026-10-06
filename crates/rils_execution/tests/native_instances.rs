use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use rils_execution::{
    RilsValue, Type, Value,
    environment::{AccessError, StorageSlot},
    value::{
        EnumInstance, EnumPayload, EnumType, FieldSlot, HostObject, HostType, IndexedStorage,
        ReferenceValue, StructFields, StructInstance, StructType,
        native_instance::{NativeInstancePlace, definition},
        storage::TypedStorageContext,
    },
};
use rils_frontend::{ast::Stmt, lex, parse};
use rils_value::DynamicPathStep;

#[derive(Default)]
struct Declarations {
    structs: Vec<Rc<StructType>>,
    enums: Vec<Rc<EnumType>>,
}

impl Declarations {
    fn fixture() -> Self {
        let program =
            parse(lex(include_str!("fixtures/native_instances/declarations.rils")).unwrap())
                .unwrap();
        let mut declarations = Self::default();
        for statement in program.statements {
            match statement {
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => declarations.structs.push(Rc::new(StructType {
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
                } => declarations.enums.push(Rc::new(EnumType {
                    name,
                    generic_parameters,
                    variants,
                    methods: RefCell::default(),
                    trait_methods: RefCell::default(),
                    implemented_traits: RefCell::default(),
                    associated_types: RefCell::default(),
                })),
                Stmt::Impl {
                    target: Type::Named { name, .. },
                    trait_name: Some(trait_name),
                    ..
                } => {
                    if let Some(definition) = declarations
                        .structs
                        .iter()
                        .find(|definition| definition.name == name)
                    {
                        definition
                            .implemented_traits
                            .borrow_mut()
                            .insert(trait_name);
                    } else if let Some(definition) = declarations
                        .enums
                        .iter()
                        .find(|definition| definition.name == name)
                    {
                        definition
                            .implemented_traits
                            .borrow_mut()
                            .insert(trait_name);
                    }
                }
                _ => panic!("unexpected fixture declaration"),
            }
        }
        declarations
    }

    fn context(&self) -> TypedStorageContext<'_> {
        TypedStorageContext::new(&self.structs, &self.enums)
    }

    fn record(&self, name: &str, arguments: Vec<Type>, fields: Vec<(&str, Value)>) -> Value {
        let definition = self
            .structs
            .iter()
            .find(|definition| definition.name == name)
            .unwrap()
            .clone();
        let fields = fields
            .into_iter()
            .map(|(name, value)| {
                (
                    name.into(),
                    FieldSlot::new(Type::of_value(&value).unwrap(), value),
                )
            })
            .collect();
        Value::Struct(Rc::new(StructInstance {
            fields: RefCell::new(StructFields::from_map(definition.clone(), fields).unwrap()),
            type_definition: definition,
            type_arguments: arguments,
        }))
    }

    fn owned(&self, text: &str) -> Value {
        self.record(
            "Owned",
            vec![],
            vec![
                ("text", Value::from_string(text)),
                ("number", Value::from_i32(42)),
            ],
        )
    }

    fn choice(&self, variant: &str, payload: EnumPayload, item: Type) -> Value {
        Value::Enum(Rc::new(EnumInstance {
            type_definition: self.enums[0].clone(),
            variant: variant.into(),
            payload,
            type_arguments: vec![item],
        }))
    }
}

fn native(declarations: &Declarations, value: Value) -> Value {
    let ty = Type::of_value(&value).unwrap();
    declarations.context().compose_nominal(value, &ty).unwrap()
}

fn place(value: &Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("native instance")
    };
    NativeInstancePlace::new(object.clone()).unwrap()
}

#[test]
fn owner_moves_refills_and_preserves_nested_native_instances() {
    let declarations = Declarations::fixture();
    let value = native(
        &declarations,
        declarations.record(
            "Outer",
            vec![],
            vec![("inner", declarations.owned("original"))],
        ),
    );
    let root = place(&value);
    let inner = root.field("inner").unwrap();
    let text = inner.field("text").unwrap();
    assert_eq!(
        text.take().unwrap().as_string().as_deref(),
        Some("original")
    );
    assert!(text.take().is_err());
    assert!(value.is_partially_moved());
    assert!(value.clone_owned().is_err());
    assert!(root.borrow(false, None).is_err());
    assert!(inner.take().is_err());
    assert_eq!(
        inner.field("number").unwrap().take().unwrap().as_i32(),
        Some(42)
    );
    assert!(text.assign(Value::from_i32(1)).is_err());
    assert!(value.is_partially_moved());
    text.assign(Value::from_string("refilled")).unwrap();
    assert!(!value.is_partially_moved());
    let moved = inner.take().unwrap();
    assert!(
        matches!(moved, Value::Dynamic(_)),
        "moving a user field must retain native storage"
    );
    assert_eq!(Type::of_value(&moved), Some(Type::named("Owned")));
    assert!(value.is_partially_moved());
    inner.assign(moved).unwrap();
    assert!(!value.is_partially_moved());
    assert_eq!(
        inner
            .field("text")
            .unwrap()
            .take()
            .unwrap()
            .as_string()
            .as_deref(),
        Some("refilled")
    );
}

#[test]
fn storage_take_rejects_partial_moves_and_reassignment_keeps_the_descriptor() {
    let declarations = Declarations::fixture();
    let value = native(&declarations, declarations.owned("first"));
    let mut storage = StorageSlot::uninitialized(true);
    storage.initialize(value);
    let root = place(&storage.read().unwrap());
    root.field("text").unwrap().take().unwrap();
    assert!(matches!(storage.take(), Err(AccessError::PartiallyMoved)));
    root.field("text")
        .unwrap()
        .assign(Value::from_string("restored"))
        .unwrap();
    drop(root);
    drop(storage.take().unwrap());
    storage.assign(declarations.owned("second")).unwrap();
    let value = storage.read().unwrap();
    let Value::Dynamic(object) = &value else {
        panic!("reassignment lost native declaration")
    };
    let Some(Value::StructType(actual)) = definition(object) else {
        panic!("struct identity")
    };
    assert!(Rc::ptr_eq(&actual, &declarations.structs[1]));
    assert_eq!(
        place(&value)
            .field("text")
            .unwrap()
            .take()
            .unwrap()
            .as_string()
            .as_deref(),
        Some("second")
    );
}

#[test]
fn copy_owners_are_independent_and_mutable_references_share_one_place() {
    let declarations = Declarations::fixture();
    let value = native(
        &declarations,
        declarations.record(
            "Point",
            vec![],
            vec![("x", Value::from_i32(1)), ("y", Value::from_i32(2))],
        ),
    );
    assert!(value.is_copy());
    let copied = value.clone_owned().unwrap();
    let root = place(&value);
    let first = root.field("x").unwrap().borrow(true, None).unwrap();
    let second = root.field("x").unwrap().borrow(true, None).unwrap();
    first.write(Value::from_i32(41)).unwrap();
    assert_eq!(second.copy_native().unwrap().unwrap().as_i32(), Some(41));
    second.write(Value::from_i32(42)).unwrap();
    assert_eq!(root.field("x").unwrap().take().unwrap().as_i32(), Some(42));
    assert_eq!(
        place(&copied).field("x").unwrap().take().unwrap().as_i32(),
        Some(1)
    );
    let Value::Dynamic(original) = &value else {
        unreachable!()
    };
    let Value::Dynamic(copied) = copied else {
        unreachable!()
    };
    assert!(!original.same_storage(&copied));
}

#[test]
fn leases_block_moves_and_parent_replacement_but_allow_other_fields() {
    let declarations = Declarations::fixture();
    let value = native(
        &declarations,
        declarations.record("Outer", vec![], vec![("inner", declarations.owned("live"))]),
    );
    let inner = place(&value).field("inner").unwrap();
    let text = inner.field("text").unwrap();
    let reference = Rc::new(text.borrow(false, None).unwrap());
    assert!(text.take().unwrap_err().contains("referenced"));
    assert!(text.assign(Value::from_string("blocked")).is_err());
    assert!(inner.assign(declarations.owned("blocked")).is_err());
    inner
        .field("number")
        .unwrap()
        .assign(Value::from_i32(7))
        .unwrap();
    assert_eq!(
        RilsValue::new(Value::Reference(reference.clone()))
            .get_cloned::<String>()
            .unwrap(),
        "live"
    );
    drop(reference);
    inner.assign(declarations.owned("replaced")).unwrap();
    assert!(!value.is_partially_moved());
}

#[test]
fn host_field_handles_keep_the_original_bytes_owner_without_snapshots() {
    let declarations = Declarations::fixture();
    let value = native(
        &declarations,
        declarations.record(
            "Outer",
            vec![],
            vec![("inner", declarations.owned("borrowed"))],
        ),
    );
    let root = place(&value);
    let result = RilsValue::new(value);
    assert_eq!(result.struct_name().as_deref(), Some("Outer"));
    assert_eq!(result.field_name(0).as_deref(), Some("inner"));
    assert!(result.field(1).is_err());
    let inner = result.field(0).unwrap();
    assert_eq!(inner.struct_name().as_deref(), Some("Owned"));
    let text = inner.field(0).unwrap();
    assert_eq!(
        text.with_ref::<String, _>(|text| text.as_str().to_owned())
            .unwrap(),
        "borrowed"
    );
    let number = inner.field(1).unwrap();
    root.field("inner")
        .unwrap()
        .field("number")
        .unwrap()
        .borrow(true, None)
        .unwrap()
        .write(Value::from_i32(9))
        .unwrap();
    assert_eq!(number.get_cloned::<i32>().unwrap(), 9);
    assert!(root.field("inner").unwrap().take().is_err());
    drop(result);
    drop(inner);
    assert_eq!(text.get_cloned::<String>().unwrap(), "borrowed");
    drop(text);
    drop(number);
    assert!(root.field("inner").unwrap().take().is_ok());
}

#[test]
fn generic_enum_variants_and_nested_sum_sequence_paths_track_partial_moves() {
    let declarations = Declarations::fixture();
    let item_type = Type::named("Owned");
    for variant in ["Empty", "Tuple", "Record"] {
        let payload = match variant {
            "Empty" => EnumPayload::Unit,
            "Tuple" => EnumPayload::Tuple(vec![declarations.owned("tuple")]),
            _ => EnumPayload::Record(HashMap::from([(
                "item".into(),
                declarations.owned("record"),
            )])),
        };
        let value = native(
            &declarations,
            declarations.choice(variant, payload, item_type.clone()),
        );
        assert_eq!(
            Type::of_value(&value),
            Some(Type::Named {
                name: "Choice".into(),
                arguments: vec![item_type.clone()]
            })
        );
        assert!(!value.is_partially_moved());
        let index = match variant {
            "Empty" => 0,
            "Tuple" => 1,
            _ => 2,
        };
        let active = place(&value)
            .project(DynamicPathStep::Variant(index))
            .unwrap();
        if index > 0 {
            let item = active.field(if index == 1 { "0" } else { "item" }).unwrap();
            let moved = item.field("text").unwrap().take().unwrap();
            assert!(value.is_partially_moved());
            item.field("text").unwrap().assign(moved).unwrap();
            assert!(!value.is_partially_moved());
        }
        assert!(
            place(&value)
                .project(DynamicPathStep::Variant((index + 1) % 3))
                .is_err()
        );
    }
    let choice_type = Type::Named {
        name: "Choice".into(),
        arguments: vec![item_type.clone()],
    };
    let wrapped = native(
        &declarations,
        declarations.record(
            "Wrapped",
            vec![],
            vec![(
                "item",
                Value::Option {
                    value: Some(Rc::new(declarations.choice(
                        "Record",
                        EnumPayload::Record(HashMap::from([(
                            "item".into(),
                            declarations.owned("wrapped"),
                        )])),
                        item_type.clone(),
                    ))),
                    element_type: Some(choice_type),
                },
            )],
        ),
    );
    let nested = place(&wrapped)
        .field("item")
        .unwrap()
        .project(DynamicPathStep::Some)
        .unwrap()
        .project(DynamicPathStep::Variant(2))
        .unwrap()
        .field("item")
        .unwrap()
        .field("text")
        .unwrap();
    let text = nested.take().unwrap();
    assert!(wrapped.is_partially_moved());
    nested.assign(text).unwrap();
    assert!(!wrapped.is_partially_moved());

    let listed = native(
        &declarations,
        declarations.record(
            "Listed",
            vec![],
            vec![(
                "items",
                Value::Vec(Rc::new(IndexedStorage {
                    active_iterators: Cell::new(0),
                    element_type: RefCell::new(Some(item_type.clone())),
                    elements: RefCell::new(vec![FieldSlot::new(
                        item_type,
                        declarations.owned("listed"),
                    )]),
                })),
            )],
        ),
    );
    let nested = place(&listed)
        .field("items")
        .unwrap()
        .project(DynamicPathStep::Index(0))
        .unwrap()
        .field("text")
        .unwrap();
    let text = nested.take().unwrap();
    assert!(listed.is_partially_moved());
    nested.assign(text).unwrap();
    assert!(!listed.is_partially_moved());
}

#[test]
fn moved_and_replaced_host_fields_drop_exactly_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let declarations = Declarations::fixture();
    let host = Rc::new(HostType {
        name: "Host".into(),
        base_types: HashSet::new(),
        copy: false,
        methods: RefCell::default(),
    });
    let first_drops = Rc::new(Cell::new(0));
    let second_drops = Rc::new(Cell::new(0));
    let replacement_drops = Rc::new(Cell::new(0));
    let host_value = |drops: &Rc<Cell<usize>>| {
        Value::HostObject(Rc::new(HostObject {
            type_definition: host.clone(),
            payload: Rc::new(Probe(drops.clone())),
        }))
    };
    let old = declarations.record(
        "HostHolder",
        vec![],
        vec![
            ("first", host_value(&first_drops)),
            ("second", host_value(&second_drops)),
        ],
    );
    let hosts = [host.clone()];
    let value = TypedStorageContext::with_hosts(&declarations.structs, &declarations.enums, &hosts)
        .compose_nominal(old, &Type::named("HostHolder"))
        .unwrap();
    let root = place(&value);
    let moved = root.field("first").unwrap().take().unwrap();
    root.field("second")
        .unwrap()
        .assign(host_value(&replacement_drops))
        .unwrap();
    assert_eq!(second_drops.get(), 1);
    assert_eq!(first_drops.get(), 0);
    drop(root);
    drop(value);
    assert_eq!(replacement_drops.get(), 1);
    assert_eq!(first_drops.get(), 0);
    drop(moved);
    assert_eq!(first_drops.get(), 1);
}

#[test]
fn root_references_keep_declarations_and_local_origins() {
    let declarations = Declarations::fixture();
    let environment = rils_execution::environment::Environment::global();
    let value = native(&declarations, declarations.owned("root"));
    environment.borrow_mut().define("record", value, true, None);
    let storage = environment.borrow().slot("record").unwrap();
    let reference = Rc::new(ReferenceValue::new_storage(storage.clone(), true));
    let Some(Value::StructType(definition)) = reference.native_type_definition().unwrap() else {
        panic!("native definition")
    };
    assert!(Rc::ptr_eq(&definition, &declarations.structs[1]));
    let text = Rc::new(reference.project_native_field("text").unwrap().unwrap());
    assert!(text.is_local_to(&environment));
    assert!(storage.borrow_mut().take().is_err());
    text.write(Value::from_string("changed")).unwrap();
    assert_eq!(
        RilsValue::new(Value::Reference(text.clone()))
            .get_cloned::<String>()
            .unwrap(),
        "changed"
    );
    drop(text);
    drop(reference);
    assert!(storage.borrow_mut().take().is_ok());
}

#[test]
fn owned_conversion_rejects_shared_non_copy_payloads_instead_of_cloning() {
    let declarations = Declarations::fixture();
    let source = Value::from_string("shared");
    let record = declarations.record(
        "Owned",
        vec![],
        vec![("text", source.clone()), ("number", Value::from_i32(42))],
    );
    assert!(
        declarations
            .context()
            .compose_nominal(record, &Type::named("Owned"))
            .is_err()
    );
    assert_eq!(source.as_string().as_deref(), Some("shared"));
    let value = native(&declarations, declarations.owned("original"));
    let text = place(&value).field("text").unwrap();
    assert!(text.assign(source.clone()).is_err());
    assert_eq!(
        text.take().unwrap().as_string().as_deref(),
        Some("original")
    );

    let shared = native(&declarations, declarations.owned("native shared"));
    let outer = native(
        &declarations,
        declarations.record(
            "Outer",
            vec![],
            vec![("inner", declarations.owned("untouched"))],
        ),
    );
    assert!(
        place(&outer)
            .field("inner")
            .unwrap()
            .assign(shared.clone())
            .is_err()
    );
    assert_eq!(
        place(&shared)
            .field("text")
            .unwrap()
            .take()
            .unwrap()
            .as_string()
            .as_deref(),
        Some("native shared")
    );
    assert_eq!(
        place(&outer)
            .field("inner")
            .unwrap()
            .field("text")
            .unwrap()
            .take()
            .unwrap()
            .as_string()
            .as_deref(),
        Some("untouched")
    );
}

#[test]
fn generic_instance_fields_keep_reference_and_callback_identities() {
    use rils_execution::{FunctionSignature, value::HostFunction};
    let declarations = Declarations::fixture();
    let callback_type = Type::function(vec![Type::I32], Type::I32);
    let callback = Rc::new(HostFunction {
        name: "increment".into(),
        min_arity: 1,
        max_arity: 1,
        signature: Some(FunctionSignature::fixed(vec![Type::I32], Type::I32)),
        function: Rc::new(|arguments| Ok(Value::from_i32(arguments[0].as_i32().unwrap() + 1))),
    });
    let value = native(
        &declarations,
        declarations.record(
            "Holder",
            vec![callback_type.clone()],
            vec![("item", Value::HostFunction(callback.clone()))],
        ),
    );
    assert_eq!(
        Type::of_value(&value),
        Some(Type::Named {
            name: "Holder".into(),
            arguments: vec![callback_type]
        })
    );
    assert!(
        !value.is_copy(),
        "the callback field is Copy, but Holder has no Copy impl"
    );
    let Value::HostFunction(extracted) = place(&value).field("item").unwrap().take().unwrap()
    else {
        panic!("callback")
    };
    assert!(Rc::ptr_eq(&extracted, &callback));
    assert_eq!(
        (extracted.function)(&[Value::from_i32(41)])
            .unwrap()
            .as_i32(),
        Some(42)
    );

    let environment = rils_execution::environment::Environment::global();
    environment
        .borrow_mut()
        .define("number", Value::from_i32(1), true, Some(Type::I32));
    let storage = environment.borrow().slot("number").unwrap();
    let reference = Rc::new(ReferenceValue::new_storage(storage.clone(), true));
    let reference_type = Type::Reference {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    let value = native(
        &declarations,
        declarations.record(
            "Holder",
            vec![reference_type],
            vec![("item", Value::Reference(reference.clone()))],
        ),
    );
    assert!(value.contains_local_reference(&environment));
    let copied = value.clone_owned().unwrap();
    drop(value);
    let Value::Reference(extracted) = place(&copied).field("item").unwrap().take().unwrap() else {
        panic!("reference")
    };
    assert!(Rc::ptr_eq(&extracted, &reference));
    extracted.write(Value::from_i32(42)).unwrap();
    assert_eq!(storage.borrow().read().unwrap().as_i32(), Some(42));
}

#[test]
fn enum_copy_requires_an_explicit_declaration_and_copy_fields_in_every_variant() {
    for declared in [false, true] {
        for eligible in [false, true] {
            let declarations = Declarations::fixture();
            if declared {
                declarations.enums[0]
                    .implemented_traits
                    .borrow_mut()
                    .extend(["Clone".into(), "Copy".into()]);
            }
            for variant in ["Empty", "Tuple", "Record"] {
                let item_type = if eligible {
                    Type::I32
                } else {
                    Type::named("Owned")
                };
                let item = || {
                    if eligible {
                        Value::from_i32(42)
                    } else {
                        declarations.owned("non Copy")
                    }
                };
                let payload = match variant {
                    "Empty" => EnumPayload::Unit,
                    "Tuple" => EnumPayload::Tuple(vec![item()]),
                    _ => EnumPayload::Record(HashMap::from([("item".into(), item())])),
                };
                let value = declarations.choice(variant, payload, item_type);
                let ty = Type::of_value(&value).unwrap();
                let result = declarations.context().compose_nominal(value, &ty);
                if declared && !eligible {
                    assert!(
                        result.err().unwrap().contains("non-Copy fields"),
                        "{variant}: inactive variants must be checked"
                    );
                    continue;
                }
                let value = result.unwrap();
                assert_eq!(
                    value.is_copy(),
                    declared && eligible,
                    "{variant}: explicit Copy policy"
                );
                let mut storage = StorageSlot::uninitialized(false);
                storage.initialize(value);
                drop(storage.take().unwrap());
                assert_eq!(
                    storage.take().is_ok(),
                    declared && eligible,
                    "{variant}: ownership policy"
                );
            }
        }
    }
}
