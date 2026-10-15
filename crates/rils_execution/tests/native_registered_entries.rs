use rils_execution::{
    Type, Value,
    environment::Environment,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    value::{ReferenceValue, native_instance::NativeInstancePlace, storage::TypedStorageContext},
};
use rils_value::{NativeChildren, NativeObject, NativeType};
use std::{cell::Cell, rc::Rc};

fn map(tree: bool, value: Value) -> Value {
    let ty = Type::of_value(&value).unwrap();
    let tuple = TypedStorageContext::new(&[], &[])
        .construct_tuple(
            &Type::Tuple(vec![Type::I32, ty.clone()]),
            vec![Value::from(rils_execution::value::native_i32(7)), value],
        )
        .unwrap();
    let Value::Dynamic(tuple) = tuple else {
        panic!("native entry")
    };
    let codec = tuple
        .descriptor()
        .metadata::<rils_execution::value::record_codec::NativeRecordCodec>()
        .unwrap();
    let layout = rils_value::DynamicLayout::sequence(
        Type::Named {
            name: if tree { "BTreeMap" } else { "HashMap" }.into(),
            arguments: vec![Type::I32, ty],
        },
        tuple.descriptor().layout_handle(),
    );
    rils_execution::value::native_instance::from_native(
        rils_value::DynamicValue::sequence(
            layout,
            vec![tuple.into_value().map_err(|e| e.1).unwrap()],
        )
        .unwrap(),
        codec,
    )
    .unwrap()
}
fn entry(source: Value) -> Value {
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(source).unwrap() else {
        panic!("iterator");
    };
    next_builtin(&mut iterator).unwrap().unwrap().unwrap()
}
fn place(value: Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("native tuple");
    };
    NativeInstancePlace::new(object).unwrap()
}

struct Payload {
    text: String,
    drops: Rc<Cell<usize>>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
#[test]
fn custom_map_values_keep_bytes_methods_and_registration_across_nested_tuples() {
    for tree in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(
            NativeType::new::<Payload>(Type::named("Payload")).register_method(
                "length",
                |context| {
                    context.receiver::<Payload, _>(|value| {
                        Value::from(rils_execution::value::native_usize(value.text.len()))
                    })
                },
            ),
        );
        let text = "original".to_owned();
        let address = text.as_ptr();
        let payload = Value::Native(
            NativeObject::new(
                descriptor.clone(),
                Payload {
                    text,
                    drops: drops.clone(),
                },
            )
            .unwrap(),
        );
        let value = entry(map(tree, payload));
        assert!(!value.is_copy());
        let ty = Type::Tuple(vec![Type::of_value(&value).unwrap()]);
        let nested = TypedStorageContext::new(&[], &[])
            .construct_tuple(&ty, vec![value])
            .unwrap();
        let inner = place(nested).field("0").unwrap().take().unwrap();
        let fields = place(inner);
        let field = fields.field("1").unwrap();
        let value = field.take().unwrap();
        let Value::Native(object) = value else {
            panic!("registered native leaf");
        };
        assert!(Rc::ptr_eq(&descriptor, &object.descriptor_handle()));
        assert_eq!(
            object.with::<Payload, _>(|value| value.text.as_ptr()),
            Ok(address)
        );
        assert_eq!(
            object
                .call("length", &[])
                .unwrap()
                .unwrap()
                .get_cloned::<usize>()
                .ok(),
            Some(8)
        );
        assert_eq!(drops.get(), 0);
        // Restore a moved field without reading its old payload.
        field.assign(Value::Native(object)).unwrap();
        drop(field);
        drop(fields);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn abandoned_native_map_iterators_drop_non_clone_payloads_once() {
    for tree in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(NativeType::new::<Payload>(Type::named("Payload")));
        let payload = Value::from(
            NativeObject::new(
                descriptor,
                Payload {
                    text: "owned".into(),
                    drops: drops.clone(),
                },
            )
            .unwrap(),
        );
        let source = map(tree, payload);
        assert_eq!(drops.get(), 0);
        let IntoIteratorResult::Ready(iterator) = into_iterator(source).unwrap() else {
            panic!("native map iterator")
        };
        assert_eq!(drops.get(), 0);
        drop(iterator);
        assert_eq!(drops.get(), 1);
    }
}

struct Children {
    child: Value,
    active: Rc<Cell<bool>>,
    partial: Rc<Cell<bool>>,
}
impl NativeChildren<Value> for Children {
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.child);
    }
    fn has_active_references(&self) -> bool {
        self.active.get()
    }
    fn is_partially_moved(&self) -> bool {
        self.partial.get()
    }
}
#[test]
fn composed_registered_children_keep_lexical_origin_and_state_checks() {
    for tree in [false, true] {
        let environment = Environment::global();
        environment.borrow_mut().define(
            "number",
            Value::from(rils_execution::value::native_i32(1)),
            true,
            Some(Type::I32),
        );
        let slot = environment.borrow().slot("number").unwrap();
        let reference =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        let active = Rc::new(Cell::new(false));
        let partial = Rc::new(Cell::new(false));
        let descriptor = Rc::new(
            NativeType::new::<Children>(Type::named("Children")).with_children::<Children>(),
        );
        let payload = Value::Native(
            NativeObject::new(
                descriptor.clone(),
                Children {
                    child: reference,
                    active: active.clone(),
                    partial: partial.clone(),
                },
            )
            .unwrap(),
        );
        let value = entry(map(tree, payload));
        assert!(value.contains_reference());
        assert!(value.contains_local_reference(&environment));
        assert!(!value.contains_local_reference(&Environment::global()));
        let fields = place(value.clone());
        active.set(true);
        assert!(value.has_active_references());
        assert!(fields.field("1").unwrap().take().is_err());
        active.set(false);
        partial.set(true);
        assert!(value.is_partially_moved());
        assert!(fields.field("1").unwrap().take().is_err());
        partial.set(false);
        assert!(!value.is_partially_moved());
        let child = fields.field("1").unwrap().take().unwrap();
        assert!(!value.contains_reference());
        assert!(child.contains_local_reference(&environment));
        drop(child);
        drop(fields);
        drop(value);
        assert!(slot.borrow_mut().take().is_ok());
    }
}

#[test]
fn entry_copy_requires_explicit_native_registration() {
    #[derive(Clone, Copy)]
    struct Number(i32);
    for tree in [false, true] {
        for copy in [false, true] {
            let mut descriptor = NativeType::new::<Number>(Type::named("Number"));
            if copy {
                descriptor = descriptor.with_copy::<Number>();
            }
            let value = entry(map(
                tree,
                Value::Native(NativeObject::new(Rc::new(descriptor), Number(42)).unwrap()),
            ));
            assert_eq!(value.is_copy(), copy);
            if copy {
                assert!(value.clone_owned().is_ok());
            }
            let Value::Native(child) = place(value).field("1").unwrap().take().unwrap() else {
                panic!("leaf");
            };
            assert_eq!(child.with::<Number, _>(|value| value.0), Ok(42));
        }
    }
}

#[test]
fn conflicting_native_registrations_are_rejected_instead_of_losing_operations() {
    let make = |result| {
        Value::Native(
            NativeObject::new(
                Rc::new(
                    NativeType::new::<i32>(Type::named("Custom"))
                        .register_method("method", move |_| {
                            Ok(Value::from(rils_execution::value::native_i32(result)))
                        }),
                ),
                1_i32,
            )
            .unwrap(),
        )
    };
    let ty = Type::Tuple(vec![Type::named("Custom"), Type::named("Custom")]);
    let error = TypedStorageContext::new(&[], &[])
        .construct_tuple(&ty, vec![make(1), make(2)])
        .err()
        .unwrap();
    assert!(error.contains("conflicting native registration"));
}

#[test]
fn map_insert_rejects_conflicting_registration_before_replacing_existing_value() {
    use rils_execution::runtime_builtins::{NativeOwnedContext, call_native_owned_symbol};

    for tree in [false, true] {
        let descriptor = |answer| {
            Rc::new(
                NativeType::new::<i32>(Type::named("RegisteredValue"))
                    .register_method("answer", move |_| {
                        Ok(Value::from(rils_execution::value::native_i32(answer)))
                    }),
            )
        };
        let original = descriptor(11);
        let replacement = descriptor(29);
        let environment = Environment::global();
        environment.borrow_mut().define(
            "map",
            map(
                tree,
                NativeObject::new(original.clone(), 11_i32).unwrap().into(),
            ),
            true,
            None,
        );
        let slot = environment.borrow().slot("map").unwrap();
        let receiver =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        let owner = if tree { "BTreeMap" } else { "HashMap" };
        let symbol = rils_builtins::builtin_member(owner, "insert")
            .unwrap()
            .native_symbol
            .unwrap();
        let error = call_native_owned_symbol(
            symbol,
            vec![
                receiver,
                rils_execution::value::native_i32(7).into(),
                NativeObject::new(replacement, 29_i32).unwrap().into(),
            ],
            &NativeOwnedContext::default(),
        )
        .unwrap()
        .unwrap_err();
        assert!(
            error.contains("conflicting native registration"),
            "{owner}: {error}"
        );

        let source = slot.borrow_mut().take().unwrap();
        let stored = place(entry(source)).field("1").unwrap().take().unwrap();
        let Value::Native(stored) = stored else {
            panic!("registered native value must keep its registration");
        };
        assert!(Rc::ptr_eq(&original, &stored.descriptor_handle()));
        assert_eq!(stored.with::<i32, _>(|value| *value), Ok(11));
    }
}

#[test]
fn first_map_insert_retains_registration_in_the_original_owner_context() {
    use rils_execution::runtime_builtins::{
        NativeOwnedContext, call_native_owned_symbol, call_native_symbol,
    };
    use rils_execution::value::record_codec::NativeRecordCodec;
    use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

    for tree in [false, true] {
        for has_context in [false, true] {
            let owner = if tree { "BTreeMap" } else { "HashMap" };
            let descriptor = Rc::new(
                NativeType::new::<i32>(Type::named("LateRegistration"))
                    .with_copy::<i32>()
                    .register_method("answer", |_| {
                        Ok(Value::from(rils_execution::value::native_i32(41)))
                    }),
            );
            let item = DynamicLayout::aggregate(
                Type::Tuple(vec![Type::I32, Type::named("LateRegistration")]),
                vec![
                    ("0".into(), DynamicLayout::copy_of::<i32>(Type::I32)),
                    ("1".into(), descriptor.layout_handle()),
                ],
            )
            .unwrap();
            let layout = DynamicLayout::sequence(
                Type::Named {
                    name: owner.into(),
                    arguments: vec![Type::I32, Type::named("LateRegistration")],
                },
                item,
            );
            let payload = DynamicValue::sequence(layout.clone(), Vec::new()).unwrap();
            let context = Rc::new(NativeRecordCodec::new());
            let snapshot = context.as_ref().clone();
            let value = if has_context {
                rils_execution::value::native_instance::from_native(payload, context).unwrap()
            } else {
                Value::Dynamic(
                    DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap(),
                )
            };
            let environment = Environment::global();
            environment.borrow_mut().define("map", value, true, None);
            let slot = environment.borrow().slot("map").unwrap();
            let receiver = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)));
            let symbol = |method| {
                rils_builtins::builtin_member(owner, method)
                    .unwrap()
                    .native_symbol
                    .unwrap()
            };
            call_native_owned_symbol(
                symbol("insert"),
                vec![
                    receiver.clone(),
                    rils_execution::value::native_i32(7).into(),
                    NativeObject::new(descriptor.clone(), 9_i32).unwrap().into(),
                ],
                &NativeOwnedContext::default(),
            )
            .unwrap()
            .unwrap();

            for method in ["get_cloned", "remove"] {
                let value = call_native_symbol(
                    symbol(method),
                    &[
                        receiver.clone(),
                        rils_execution::value::native_i32(7).into(),
                    ],
                )
                .unwrap()
                .unwrap();
                let Value::Native(value) = rils_execution::value::dynamic_option::take_owned(value)
                    .unwrap()
                    .unwrap()
                else {
                    panic!(
                        "{owner}::{method} lost inserted native registration (context={has_context})"
                    );
                };
                assert!(Rc::ptr_eq(&descriptor, &value.descriptor_handle()));
                assert_eq!(
                    value
                        .call("answer", &[])
                        .unwrap()
                        .unwrap()
                        .get_cloned::<i32>(),
                    Ok(41)
                );
            }
            // Explicit commits update the receiver's context, not earlier detached snapshots.
            assert!(matches!(
                snapshot
                    .from_native(
                        DynamicValue::from_rust(descriptor.layout_handle(), 1_i32).unwrap()
                    )
                    .unwrap(),
                Value::Dynamic(_)
            ));
        }
    }
}

#[test]
fn native_field_assignment_commits_new_registered_operations() {
    use rils_execution::value::record_codec::NativeRecordCodec;
    use rils_value::{DynamicLayout, DynamicValue};

    for via_reference in [false, true] {
        let descriptor =
            Rc::new(NativeType::new::<i32>(Type::named("FieldValue")).with_copy::<i32>());
        let layout = DynamicLayout::aggregate(
            Type::Tuple(vec![Type::named("FieldValue")]),
            vec![("0".into(), descriptor.layout_handle())],
        )
        .unwrap();
        let payload = DynamicValue::record(
            layout,
            vec![DynamicValue::from_rust(descriptor.layout_handle(), 1_i32).unwrap()],
        )
        .unwrap();
        let value = rils_execution::value::native_instance::from_native(
            payload,
            Rc::new(NativeRecordCodec::new()),
        )
        .unwrap();
        let field = place(value).field("0").unwrap();
        let replacement = NativeObject::new(descriptor.clone(), 9_i32).unwrap().into();
        if via_reference {
            field
                .borrow(true, None)
                .unwrap()
                .write(replacement)
                .unwrap();
        } else {
            field.assign(replacement).unwrap();
        }
        let Value::Native(stored) = field.take().unwrap() else {
            panic!("field assignment lost native operations");
        };
        assert!(Rc::ptr_eq(&descriptor, &stored.descriptor_handle()));
        assert_eq!(stored.with::<i32, _>(|value| *value), Ok(9));
    }
}
