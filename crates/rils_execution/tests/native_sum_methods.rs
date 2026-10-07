use std::rc::Rc;

use rils_execution::{
    RilsValue, Type, Value,
    environment::Environment,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{DynamicObject, ReferenceValue},
};
use rils_value::{DynamicPathStep, DynamicType, DynamicValue};

fn symbol(owner: &str, name: &str) -> &'static str {
    rils_builtins::builtin_member(owner, name)
        .unwrap()
        .native_symbol
        .unwrap()
}

#[test]
fn sum_outputs_are_native_and_shared_non_copy_inputs_are_rejected() {
    let context = NativeOwnedContext::default();
    let ty = Type::Option(Box::new(Type::String));
    for method in ["or", "xor"] {
        let left = context
            .storage()
            .construct_option(&ty, Some(Value::from_string("owned")))
            .unwrap();
        let right = context.storage().construct_option(&ty, None).unwrap();
        let output =
            call_native_owned_symbol(symbol("Option", method), vec![left, right], &context)
                .unwrap()
                .unwrap();
        assert!(matches!(&output, Value::Dynamic(_)));
        assert_eq!(
            output
                .as_option()
                .unwrap()
                .0
                .unwrap()
                .as_string()
                .as_deref(),
            Some("owned")
        );
    }
    let value = context
        .storage()
        .construct_option(&ty, Some(Value::from_string("retained")))
        .unwrap();
    let error = call_native_owned_symbol(symbol("Option", "unwrap"), vec![value.clone()], &context)
        .unwrap()
        .unwrap_err();
    assert!(error.contains("shared"));
    assert_eq!(
        value.as_option().unwrap().0.unwrap().as_string().as_deref(),
        Some("retained")
    );
    assert!(
        call_native_symbol(symbol("Option", "unwrap"), &[value])
            .unwrap()
            .unwrap_err()
            .contains("owned argument frame")
    );

    let result_type = Type::Result(Box::new(Type::I32), Box::new(Type::String));
    for (method, branch, present) in [
        ("ok", Ok(Value::from_i32(42)), true),
        ("ok", Err(Value::from_string("error")), false),
        ("err", Ok(Value::from_i32(42)), false),
        ("err", Err(Value::from_string("error")), true),
    ] {
        let receiver = context
            .storage()
            .construct_result(&result_type, branch)
            .unwrap();
        let output = call_native_owned_symbol(symbol("Result", method), vec![receiver], &context)
            .unwrap()
            .unwrap();
        assert!(matches!(&output, Value::Dynamic(_)));
        assert_eq!(output.as_option().unwrap().0.is_some(), present);
    }
}

#[test]
fn invalid_replacements_preserve_the_native_target() {
    let context = NativeOwnedContext::default();
    let ty = Type::Option(Box::new(Type::String));
    let value = context
        .storage()
        .construct_option(&ty, Some(Value::from_string("retained")))
        .unwrap();
    let environment = Environment::global();
    environment
        .borrow_mut()
        .define("value", value, true, Some(ty));
    let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(
        environment.borrow().slot("value").unwrap(),
        true,
    )));
    let error = call_native_owned_symbol(
        symbol("Option", "replace"),
        vec![reference.clone(), Value::from_i32(1)],
        &context,
    )
    .unwrap()
    .unwrap_err();
    assert!(error.contains("string"));
    let previous = call_native_owned_symbol(symbol("Option", "take"), vec![reference], &context)
        .unwrap()
        .unwrap();
    assert!(matches!(&previous, Value::Dynamic(_)));
    assert_eq!(
        previous
            .as_option()
            .unwrap()
            .0
            .unwrap()
            .as_string()
            .as_deref(),
        Some("retained")
    );
}

#[test]
fn mutable_sum_calls_reject_partial_payloads_before_replacing_the_target() {
    let context = NativeOwnedContext::default();
    let item_type = Type::Tuple(vec![Type::I32, Type::I32]);
    let layout = context
        .layout(&Type::Option(Box::new(item_type.clone())))
        .unwrap();
    let item = DynamicValue::record(
        context.layout(&item_type).unwrap(),
        [1, 42]
            .into_iter()
            .map(|value| {
                DynamicValue::from_rust(context.layout(&Type::I32).unwrap(), value).unwrap()
            })
            .collect(),
    )
    .unwrap();
    let mut payload = DynamicValue::some(layout.clone(), item).unwrap();
    payload
        .take_path_field(&[DynamicPathStep::Some, DynamicPathStep::Field(0)])
        .unwrap();
    let value =
        Value::Dynamic(DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap());
    let environment = Environment::global();
    environment.borrow_mut().define("value", value, true, None);
    let reference = Rc::new(ReferenceValue::new_storage(
        environment.borrow().slot("value").unwrap(),
        true,
    ));
    for method in ["take", "replace"] {
        let mut arguments = vec![Value::Reference(reference.clone())];
        if method == "replace" {
            let item = DynamicValue::record(
                context.layout(&item_type).unwrap(),
                [3, 4]
                    .into_iter()
                    .map(|value| {
                        DynamicValue::from_rust(context.layout(&Type::I32).unwrap(), value).unwrap()
                    })
                    .collect(),
            )
            .unwrap();
            arguments.push(Value::Dynamic(
                DynamicObject::new(
                    Rc::new(DynamicType::new(context.layout(&item_type).unwrap())),
                    item,
                )
                .unwrap(),
            ));
        }
        let error = call_native_owned_symbol(symbol("Option", method), arguments, &context)
            .unwrap()
            .unwrap_err();
        assert!(error.contains("partially moved"));
        RilsValue::new(Value::Reference(reference.clone()))
            .with_native_view(|view| {
                assert!(view.option_is_some().unwrap());
                let item = view.option_item().unwrap();
                assert!(item.field(0).is_err());
                assert_eq!(
                    item.field(1).unwrap().with_rust::<i32, _>(|value| *value),
                    Ok(42)
                );
            })
            .unwrap();
    }
}
