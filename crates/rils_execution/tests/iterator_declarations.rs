use rils_execution::{
    Type,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    runtime_builtins::{
        NativeOwnedContext, call_native_owned_symbol, call_native_symbol,
        requires_owned_native_call,
    },
    value::{native_instance, record_codec::NativeRecordCodec},
};
use rils_value::{DynamicLayout, DynamicValue};
use std::rc::Rc;

fn item(layout: Rc<DynamicLayout>, index: i32) -> DynamicValue {
    if let Some(fields) = layout.record_fields() {
        let values = fields
            .iter()
            .map(|field| item(field.layout_handle(), index))
            .collect();
        DynamicValue::record(layout, values).unwrap()
    } else {
        DynamicValue::from_rust(layout, index).unwrap()
    }
}

#[test]
fn sequence_iteration_matches_declared_trait_calls_without_a_type_list() {
    let context = NativeOwnedContext::default();
    let mut tested = 0;
    for declaration in rils_builtins::BUILTINS {
        let Some(member) = declaration
            .members
            .iter()
            .find(|member| member.trait_name == Some("IntoIterator") && member.name == "into_iter")
        else {
            continue;
        };
        let Some(symbol) = member.native_symbol else {
            continue;
        };
        let ty = Type::Named {
            name: declaration.path.into(),
            arguments: declaration
                .type_parameters
                .iter()
                .map(|_| Type::I32)
                .collect(),
        };
        let Ok(layout) = context.layout(&ty) else {
            continue;
        };
        let Some(element) = layout.sequence_item() else {
            continue;
        };
        let Type::Function { return_type, .. } =
            rils_frontend::standard_library::builtin_trait_member_type(
                "IntoIterator",
                &ty,
                "into_iter",
            )
            .unwrap()
        else {
            panic!("method signature")
        };
        tested += 1;
        for length in [0, 1, 4] {
            let make = || {
                native_instance::from_native(
                    DynamicValue::sequence(
                        layout.clone(),
                        (0..length).map(|i| item(element.clone(), i)).collect(),
                    )
                    .unwrap(),
                    Rc::new(NativeRecordCodec::new()),
                )
                .unwrap()
            };
            let IntoIteratorResult::Ready(mut automatic) = into_iterator(make()).unwrap() else {
                panic!("{ty}")
            };
            let mut explicit = if requires_owned_native_call(symbol) {
                call_native_owned_symbol(symbol, vec![make()], &context)
            } else {
                call_native_symbol(symbol, &[make()])
            }
            .unwrap()
            .unwrap();
            assert_eq!(
                Type::of_value(&automatic).as_ref(),
                Some(return_type.as_ref()),
                "{ty}"
            );
            for _ in 0..length {
                assert_eq!(
                    next_builtin(&mut automatic).unwrap().unwrap(),
                    next_builtin(&mut explicit).unwrap().unwrap(),
                    "{ty}"
                );
            }
            for _ in 0..2 {
                assert!(next_builtin(&mut automatic).unwrap().unwrap().is_none());
            }
            // Existing iterators retain their current cursor under the blanket conversion.
            assert!(matches!(
                into_iterator(automatic).unwrap(),
                IntoIteratorResult::Ready(_)
            ));
        }
    }
    assert!(
        tested >= 7,
        "expected the declared sequence families, tested {tested}"
    );
}

#[test]
fn undeclared_native_sequences_remain_available_for_user_trait_dispatch() {
    let layout = DynamicLayout::sequence(
        Type::named("UserSequence"),
        DynamicLayout::copy_of::<i32>(Type::I32),
    );
    let value = native_instance::from_native(
        DynamicValue::sequence(layout, vec![]).unwrap(),
        Rc::new(NativeRecordCodec::new()),
    )
    .unwrap();
    assert!(matches!(
        into_iterator(value).unwrap(),
        IntoIteratorResult::UserDefined(_)
    ));
}

#[test]
fn borrowed_values_do_not_enter_the_owning_trait_path() {
    use rils_execution::{Value, environment::StorageSlot, value::ReferenceValue};
    use std::cell::RefCell;
    let value = NativeOwnedContext::default()
        .empty_collection(&Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::I32],
        })
        .unwrap();
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(value);
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(
        Rc::new(RefCell::new(slot)),
        false,
    )));
    let IntoIteratorResult::UserDefined(reference) = into_iterator(reference).unwrap() else {
        panic!("a borrowed value must not consume its referent");
    };
    assert_eq!(
        call_native_symbol(
            rils_builtins::builtin_member("Vec", "len")
                .unwrap()
                .native_symbol
                .unwrap(),
            &[reference],
        )
        .unwrap()
        .unwrap()
        .get_cloned::<usize>()
        .unwrap(),
        0
    );
}
