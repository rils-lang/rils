use std::rc::Rc;

use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{
    Value,
    declaration::Declaration,
    runtime_builtins::{
        NativeOwnedContext, call_native_owned_symbol, call_native_symbol,
        requires_owned_native_call,
    },
    value::{
        ReferenceValue, native_i32,
        native_instance::{NativeInstancePlace, value_definition},
    },
};

fn call(owner: &str, method: &str, value: Value) -> Result<Value, String> {
    let symbol = rils_builtins::builtin_member(owner, method)
        .unwrap()
        .native_symbol
        .unwrap();
    if requires_owned_native_call(symbol) {
        call_native_owned_symbol(symbol, vec![value], &NativeOwnedContext::default()).unwrap()
    } else {
        call_native_symbol(symbol, &[value]).unwrap()
    }
}

fn assert_item(value: &Value) {
    assert!(
        matches!(value_definition(value).unwrap(), Some(Declaration::StructType(definition)) if definition.name == "Item")
    );
}

#[test]
fn projected_borrows_keep_owner_and_declarations_without_call_context() {
    let source = include_str!("fixtures/native_borrow_context.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        rils_runtime::eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        let Value::Dynamic(holder) = value else {
            panic!("native holder expected")
        };
        let place = NativeInstancePlace::new(holder).unwrap();
        let cell = Value::from_reference(Rc::new(
            place.field("cell").unwrap().borrow(false, None).unwrap(),
        ));
        let first = call("RefCell", "borrow_mut", cell.clone()).unwrap();
        assert_item(&first);
        let reference = first.as_reference().unwrap();
        let second = Rc::new(reference.reborrow(true).unwrap());
        assert!(second.read().is_err());
        assert_item(&Value::from_reference(second.clone()));
        second
            .project_native_field("value")
            .unwrap()
            .unwrap()
            .write(Value::from(native_i32(19)))
            .unwrap();
        assert_eq!(
            first
                .with_native_view(|view| view.field(0)?.with_rust::<i32, _>(|value| *value))
                .unwrap()
                .unwrap(),
            19
        );
        assert!(place.field("cell").unwrap().take().is_err());
        drop(second);
        drop(reference);
        drop(first);
        drop(cell);
        // Ending both borrows releases the original projected owner.
        assert!(place.field("cell").unwrap().take().is_ok());

        let items = Value::from_reference(Rc::new(
            place.field("items").unwrap().borrow(false, None).unwrap(),
        ));
        let Value::Native(iterator) = call("Vec", "iter", items).unwrap() else {
            panic!("native iterator expected")
        };
        let item = rils_execution::value::native_ops::next(&iterator)
            .unwrap()
            .unwrap()
            .unwrap();
        assert_item(&item);
        assert_eq!(
            item.with_native_view(|view| view.field(0)?.with_rust::<i32, _>(|value| *value))
                .unwrap()
                .unwrap(),
            7
        );
    }
}

#[test]
fn cell_reference_rejects_other_native_storage() {
    assert!(ReferenceValue::new_dynamic_cell(&Value::from(native_i32(1)), true).is_err());
}
