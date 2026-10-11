use rils_execution::{
    Type, Value,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol},
    value::{native_instance::NativeInstancePlace, record_codec::NativeRecordCodec},
};
use rils_value::{DynamicLayout, DynamicValue};
use std::{cell::Cell, rc::Rc};

fn array(ty: Type, fields: Vec<DynamicValue>, item: Rc<DynamicLayout>) -> Value {
    let layout = DynamicLayout::array(item, fields.len()).unwrap();
    assert_eq!(layout.rils_type(), &ty);
    NativeRecordCodec::new()
        .from_native(DynamicValue::record(layout, fields).unwrap())
        .unwrap()
}

#[test]
fn array_consumption_is_lazy_and_drops_nonclone_elements_once() {
    struct Payload(Rc<Cell<usize>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let leaf = DynamicLayout::of::<Payload>(Type::named("Payload"));
    let ty = Type::Array {
        element: Box::new(Type::named("Payload")),
        length: 2,
    };
    let value = array(
        ty.clone(),
        (0..2)
            .map(|_| DynamicValue::from_rust(leaf.clone(), Payload(drops.clone())).unwrap())
            .collect(),
        leaf,
    );
    assert!(matches!(&value, Value::Dynamic(_)));
    assert_eq!(Type::of_value(&value), Some(ty));
    assert!(!value.is_copy());
    let alias = value.clone();
    assert!(into_iterator(alias).is_err());
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(value).unwrap() else {
        panic!("array iterator")
    };
    assert_eq!(drops.get(), 0);
    let item = next_builtin(&mut iterator).unwrap().unwrap().unwrap();
    drop(iterator);
    assert_eq!(drops.get(), 1);
    drop(item);
    assert_eq!(drops.get(), 2);
}

#[test]
fn arrays_move_to_vec_without_cloning_and_copy_storage_is_independent() {
    type Text = rils_stdlib::stdlib::string::String;
    let leaf = DynamicLayout::of::<Text>(Type::String);
    let raw = "owned buffer".to_owned();
    let address = raw.as_ptr() as usize;
    let text = Text::from(raw);
    let value = array(
        Type::Array {
            element: Box::new(Type::String),
            length: 1,
        },
        vec![DynamicValue::from_rust(leaf.clone(), text).unwrap()],
        leaf,
    );
    let symbol = rils_builtins::builtin_member("Vec", "from")
        .unwrap()
        .native_symbol
        .unwrap();
    let vector = call_native_owned_symbol(symbol, vec![value], &NativeOwnedContext::default())
        .unwrap()
        .unwrap();
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(vector).unwrap() else {
        panic!("vec iterator")
    };
    let moved = next_builtin(&mut iterator).unwrap().unwrap().unwrap();
    assert_eq!(
        rils_execution::RilsValue::new(moved)
            .with_ref::<String, _>(|text| text.as_ptr() as usize)
            .unwrap(),
        address
    );
    assert!(next_builtin(&mut iterator).unwrap().unwrap().is_none());

    let leaf = rils_execution::value::native_layouts::integer::layout(&Type::I32).unwrap();
    let value = array(
        Type::Array {
            element: Box::new(Type::I32),
            length: 1,
        },
        vec![
            NativeRecordCodec::new()
                .into_native(Value::from_i32(1), leaf.clone())
                .unwrap(),
        ],
        leaf,
    );
    let copy = value.clone_owned().unwrap();
    let Value::Dynamic(original) = value else {
        panic!("native array")
    };
    NativeInstancePlace::new(original)
        .unwrap()
        .field("0")
        .unwrap()
        .assign(Value::from_i32(2))
        .unwrap();
    let Value::Dynamic(copy) = copy else {
        panic!("native array copy")
    };
    assert_eq!(
        NativeInstancePlace::new(copy)
            .unwrap()
            .field("0")
            .unwrap()
            .take()
            .unwrap(),
        Value::from_i32(1)
    );
}

#[test]
fn empty_arrays_preserve_element_copy_policy_and_generic_checks() {
    let layout = DynamicLayout::array(DynamicLayout::of::<String>(Type::String), 0).unwrap();
    assert!(!layout.is_copy());
    let value = NativeRecordCodec::new()
        .from_native(DynamicValue::record(layout, vec![]).unwrap())
        .unwrap();
    assert!(!value.is_copy());
    assert!(
        Type::Array {
            element: Box::new(Type::String),
            length: 0
        }
        .accepts(&value)
    );
    assert!(
        !Type::Array {
            element: Box::new(Type::I32),
            length: 0
        }
        .accepts(&value)
    );
    assert!(
        !Type::Array {
            element: Box::new(Type::String),
            length: 1
        }
        .accepts(&value)
    );
    assert!(
        Type::ArrayParameter {
            element: Box::new(Type::String),
            length: "N".into()
        }
        .accepts(&value)
    );
    assert!(
        !Type::ArrayParameter {
            element: Box::new(Type::I32),
            length: "N".into()
        }
        .accepts(&value)
    );
    assert!(Type::Slice(Box::new(Type::String)).accepts(&value));
    assert!(!Type::Slice(Box::new(Type::I32)).accepts(&value));
    assert!(
        DynamicLayout::array(DynamicLayout::copy_of::<i32>(Type::I32), 0)
            .unwrap()
            .is_copy()
    );
}
