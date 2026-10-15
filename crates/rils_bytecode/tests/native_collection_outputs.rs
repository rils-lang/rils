use std::{cell::RefCell, rc::Rc};

use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{
    Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{ReferenceValue, dynamic_option, native_instance::NativeInstancePlace},
};

fn symbol(owner: &str, method: &str) -> &'static str {
    rils_builtins::builtin_member(owner, method)
        .unwrap()
        .native_symbol
        .unwrap()
}

fn borrow(value: Value) -> Value {
    let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    slot.borrow_mut().initialize(value);
    Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)))
}

#[track_caller]
fn assert_item(option: Value) {
    let item = dynamic_option::take_owned_with_definitions(option, &[], &[])
        .unwrap()
        .unwrap();
    let definition = rils_execution::value::native_instance::value_definition(&item)
        .unwrap()
        .expect("output must retain Item without the original execution context");
    assert!(
        matches!(definition, rils_execution::declaration::Declaration::StructType(definition) if definition.name == "Item")
    );
}

#[test]
fn collection_outputs_retain_declarations_after_leaving_the_execution_context() {
    let source = include_str!("fixtures/native_collection_outputs.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        rils_runtime::eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        let Value::Dynamic(tuple) = value else {
            panic!("native tuple expected")
        };
        for (index, owner) in [(0, "HashMap"), (1, "BTreeMap")] {
            let map = NativeInstancePlace::new(tuple.clone())
                .unwrap()
                .field(&index.to_string())
                .unwrap()
                .take()
                .unwrap();
            let receiver = borrow(map);
            let key = borrow(Value::from(rils_execution::value::native_i32(1)));
            assert_item(
                call_native_symbol(
                    symbol(owner, "get_cloned"),
                    &[receiver.clone(), key.clone()],
                )
                .unwrap()
                .unwrap(),
            );
            if owner == "HashMap" {
                let iterator = call_native_symbol(
                    symbol(owner, "values_cloned"),
                    std::slice::from_ref(&receiver),
                )
                .unwrap()
                .unwrap();
                let Value::Native(iterator) = iterator else {
                    panic!("owned iterator expected")
                };
                assert_item(
                    iterator
                        .call(rils_execution::value::native_ops::NEXT, &[])
                        .unwrap()
                        .unwrap(),
                );
                assert!(
                    iterator
                        .call(rils_execution::value::native_ops::NEXT, &[])
                        .unwrap()
                        .unwrap()
                        .with_native_view(|view| view.option_is_some())
                        .unwrap()
                        .is_ok_and(|present| !present)
                );
            }
            let borrowed =
                call_native_symbol(symbol(owner, "iter"), std::slice::from_ref(&receiver))
                    .unwrap()
                    .unwrap();
            let Value::Native(borrowed) = borrowed else {
                panic!("native borrowed iterator expected")
            };
            {
                let Value::Dynamic(pair) = rils_execution::value::native_ops::next(&borrowed)
                    .unwrap()
                    .unwrap()
                    .unwrap()
                else {
                    panic!("native reference pair expected")
                };
                let item = NativeInstancePlace::new(pair)
                    .unwrap()
                    .field("1")
                    .unwrap()
                    .take()
                    .unwrap();
                assert!(matches!(
                    rils_execution::value::native_instance::value_definition(&item).unwrap(),
                    Some(rils_execution::declaration::Declaration::StructType(definition)) if definition.name == "Item"
                ));
            }
            drop(borrowed);
            assert_item(
                call_native_symbol(symbol(owner, "remove"), &[receiver, key])
                    .unwrap()
                    .unwrap(),
            );
        }
        let set = NativeInstancePlace::new(tuple)
            .unwrap()
            .field("2")
            .unwrap()
            .take()
            .unwrap();
        let receiver = borrow(set);
        let union = call_native_symbol(symbol("HashSet", "union"), &[receiver.clone(), receiver])
            .unwrap()
            .unwrap();
        let iterator = call_native_owned_symbol(
            symbol("HashSet", "into_iter"),
            vec![union],
            &NativeOwnedContext::default(),
        )
        .unwrap()
        .unwrap();
        let Value::Native(iterator) = iterator else {
            panic!("owned set iterator expected")
        };
        assert_item(
            iterator
                .call(rils_execution::value::native_ops::NEXT, &[])
                .unwrap()
                .unwrap(),
        );
    }
}
