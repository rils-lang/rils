use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::Environment,
    value::{
        NativeType, OwnedIteratorValue, ReferenceValue, native_instance::NativeInstancePlace,
        native_ops, record_codec::NativeRecordCodec, storage::TypedStorageContext,
    },
};
use rils_value::DynamicValue;

fn advance(value: &Value) -> Value {
    let Value::Native(object) = value else {
        panic!("native iterator expected")
    };
    object.call(native_ops::NEXT, &[]).unwrap().unwrap()
}

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn native_iteration_moves_non_clone_payloads_and_drops_the_unconsumed_tail() {
    for length in [0, 1, 3] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(NativeType::new::<Probe>(Type::named("Probe")));
        let layout = descriptor.layout_handle();
        let mut codec = NativeRecordCodec::new();
        codec.register_native_type(descriptor.clone()).unwrap();
        let items = (0..length)
            .map(|_| DynamicValue::from_rust(layout.clone(), Probe(drops.clone())).unwrap())
            .collect();
        let iterator = Value::from_owned_iterator(
            OwnedIteratorValue::from_native(items, layout, codec).unwrap(),
        );
        assert!(!iterator.is_copy());
        assert!(iterator.clone_owned().is_err());
        let first = advance(&iterator);
        assert_eq!(drops.get(), 0);
        let first = rils_execution::value::dynamic_option::take_owned(first).unwrap();
        if length > 0 {
            let Value::Native(first) = first.unwrap() else {
                panic!("registered payload lost")
            };
            assert!(Rc::ptr_eq(&first.descriptor_handle(), &descriptor));
            drop(first);
            assert_eq!(drops.get(), 1);
        } else {
            assert!(first.is_none());
            assert_eq!(
                Type::of_value(&advance(&iterator)),
                Some(Type::Option(Box::new(Type::named("Probe"))))
            );
        }
        drop(iterator);
        assert_eq!(drops.get(), length);
    }
}

#[test]
fn owned_iterator_native_children_preserve_local_reference_provenance() {
    let environment = Environment::global();
    environment.borrow_mut().define(
        "number",
        Value::from(rils_execution::value::native_i32(42)),
        true,
        None,
    );
    let slot = environment.borrow().slot("number").unwrap();
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)));
    let ty = Type::of_value(&reference).unwrap();
    let layout = rils_execution::runtime_builtins::NativeOwnedContext::default()
        .layout(&ty)
        .unwrap();
    let mut codec = NativeRecordCodec::new();
    let item = codec.into_native(reference, layout.clone()).unwrap();
    let iterator = Value::from_owned_iterator(
        OwnedIteratorValue::from_native(vec![item], layout, codec).unwrap(),
    );
    assert!(iterator.contains_reference());
    assert!(iterator.contains_local_reference(&environment));
    let tuple_type = Type::Tuple(vec![Type::of_value(&iterator).unwrap()]);
    let tuple = TypedStorageContext::new(&[], &[])
        .construct_tuple(&tuple_type, vec![iterator])
        .unwrap();
    assert!(tuple.contains_local_reference(&environment));
    let Value::Dynamic(tuple) = tuple else {
        unreachable!()
    };
    let iterator = NativeInstancePlace::new(tuple)
        .unwrap()
        .field("0")
        .unwrap()
        .take()
        .unwrap();
    let output = advance(&iterator);
    assert!(
        !iterator.contains_reference(),
        "exhausted storage must release its source"
    );
    assert!(
        output.contains_local_reference(&environment),
        "yielded reference retains provenance"
    );
    assert!(
        !advance(&iterator)
            .with_native_view(|view| view.option_is_some())
            .unwrap()
            .unwrap()
    );
}

#[test]
fn iterator_construction_rejects_wrong_layouts_before_advancing() {
    let layout = rils_value::DynamicLayout::copy_of::<i32>(Type::I32);
    let wrong =
        DynamicValue::from_rust(rils_value::DynamicLayout::copy_of::<bool>(Type::Bool), true)
            .unwrap();
    assert!(
        OwnedIteratorValue::from_native(vec![wrong], layout, NativeRecordCodec::new()).is_err()
    );
}
