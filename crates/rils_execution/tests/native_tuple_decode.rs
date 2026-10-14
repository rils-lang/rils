use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{
        dynamic_option, native_instance::NativeInstancePlace, record_codec::NativeRecordCodec,
        storage::TypedStorageContext,
    },
};
use rils_value::{DynamicLayout, DynamicValue};

fn place(value: Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("tuple must retain native storage")
    };
    NativeInstancePlace::new(object).unwrap()
}

#[test]
fn tuple_decode_moves_nonclone_payload_and_drops_each_field_once() {
    struct Payload(Rc<Cell<usize>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let leaf = DynamicLayout::of::<Payload>(Type::named("Payload"));
    let ty = Type::Tuple(vec![Type::named("Payload"), Type::named("Payload")]);
    let layout = DynamicLayout::aggregate(
        ty.clone(),
        vec![("0".into(), leaf.clone()), ("1".into(), leaf.clone())],
    )
    .unwrap();
    let fields = (0..2)
        .map(|_| DynamicValue::from_rust(leaf.clone(), Payload(drops.clone())).unwrap())
        .collect();
    let tuple = NativeRecordCodec::new()
        .from_native(DynamicValue::record(layout, fields).unwrap())
        .unwrap();
    assert_eq!(Type::of_value(&tuple), Some(ty));
    assert!(!tuple.is_copy());
    let owner = place(tuple);
    let first = owner.field("0").unwrap();
    let moved = first.take().unwrap();
    assert!(first.take().is_err());
    assert_eq!(drops.get(), 0);
    drop(first);
    drop(owner);
    assert_eq!(drops.get(), 1);
    drop(moved);
    assert_eq!(drops.get(), 2);
}

#[test]
fn option_decode_and_nested_field_move_keep_tuple_storage_and_independent_copy() {
    let context = TypedStorageContext::new(&[], &[]);
    let ty = Type::Tuple(vec![Type::I32]);
    let tuple = context
        .construct_tuple(&ty, vec![Value::from(rils_execution::value::native_i32(7))])
        .unwrap();
    let optional = context
        .construct_option(&Type::Option(Box::new(ty.clone())), Some(tuple))
        .unwrap();
    let decoded = dynamic_option::take_owned(optional).unwrap().unwrap();
    let copy = place(decoded.clone_owned().unwrap());
    let original = place(decoded);
    original
        .field("0")
        .unwrap()
        .assign(Value::from(rils_execution::value::native_i32(8)))
        .unwrap();
    assert_eq!(
        copy.field("0").unwrap().take().unwrap(),
        Value::from(rils_execution::value::native_i32(7))
    );
    assert_eq!(
        original.field("0").unwrap().take().unwrap(),
        Value::from(rils_execution::value::native_i32(8))
    );

    let inner_type = Type::Tuple(vec![Type::String]);
    let inner = context
        .construct_tuple(
            &inner_type,
            vec![Value::from(rils_execution::value::native_string("owned"))],
        )
        .unwrap();
    let outer = context
        .construct_tuple(
            &Type::Tuple(vec![inner_type.clone(), ty.clone()]),
            vec![
                inner,
                context
                    .construct_tuple(&ty, vec![Value::from(rils_execution::value::native_i32(9))])
                    .unwrap(),
            ],
        )
        .unwrap();
    let outer = place(outer);
    let inner = outer.field("0").unwrap().take().unwrap();
    assert_eq!(Type::of_value(&inner), Some(inner_type));
    assert!(outer.field("0").unwrap().take().is_err());
    let inner = place(inner);
    assert_eq!(
        inner.field("0").unwrap().take().unwrap(),
        Value::from(rils_execution::value::native_string("owned"))
    );
    assert_eq!(
        place(outer.field("1").unwrap().take().unwrap())
            .field("0")
            .unwrap()
            .take()
            .unwrap(),
        Value::from(rils_execution::value::native_i32(9))
    );
}
