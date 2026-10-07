use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue, NativeObject, NativeType};

#[test]
fn inline_leaf_access_checks_physical_types_and_keeps_inline_storage() {
    let object = NativeObject::new(
        Rc::new(NativeType::<()>::new::<i32>(Type::I32).with_copy::<i32>()),
        42_i32,
    )
    .unwrap();
    object
        .with_leaf(|leaf| {
            assert_eq!(leaf.rils_type(), &Type::I32);
            assert_eq!(leaf.with_rust::<i32, _>(|value| *value), Ok(42));
            assert!(leaf.with_rust::<u32, _>(|_| ()).is_err());
        })
        .unwrap();
    assert!(object.is_inline());
}

#[test]
fn leaf_callbacks_hold_shared_storage_guards_and_release_them() {
    let object = NativeObject::new(
        Rc::new(NativeType::<()>::new::<String>(Type::String)),
        "original".to_owned(),
    )
    .unwrap();
    let alias = object.clone();
    object
        .with_leaf(|leaf| {
            assert_eq!(leaf.with_rust::<String, _>(String::len), Ok(8));
            assert!(alias.with_mut::<String, _>(String::clear).is_err());
            assert!(alias.with_leaf(|_| ()).is_ok());
        })
        .unwrap();
    alias
        .with_mut::<String, _>(|value| value.push('!'))
        .unwrap();
    assert_eq!(object.with::<String, _>(String::len), Ok(9));
    object
        .with_mut::<String, _>(|_| assert!(alias.with_leaf(|_| ()).is_err()))
        .unwrap();
}

#[test]
fn over_aligned_non_clone_leaves_remain_in_original_storage_and_drop_once() {
    #[repr(align(64))]
    struct Probe {
        number: i32,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let descriptor = Rc::new(NativeType::<()>::new::<Probe>(Type::named("Probe")));
    let object = NativeObject::new(
        descriptor,
        Probe {
            number: 42,
            drops: drops.clone(),
        },
    )
    .unwrap();
    let address = object
        .with::<Probe, _>(|probe| probe as *const Probe)
        .unwrap();
    assert_eq!((address as usize) % 64, 0);
    object
        .with_leaf(|leaf| {
            leaf.with_rust::<Probe, _>(|probe| {
                assert_eq!(probe as *const Probe, address);
                assert_eq!(probe.number, 42);
            })
            .unwrap();
        })
        .unwrap();
    assert_eq!(drops.get(), 0);
    drop(object);
    assert_eq!(drops.get(), 1);
}

#[test]
fn composed_leaf_access_checks_active_tags_and_moved_fields() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let option = DynamicLayout::option(number.clone()).unwrap();
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![("number".into(), number.clone())],
    )
    .unwrap();
    let mut value = DynamicValue::record(
        record,
        vec![DynamicValue::from_rust(number.clone(), 42_i32).unwrap()],
    )
    .unwrap();
    assert!(value.view().leaf().is_err());
    assert_eq!(
        value
            .view()
            .field(0)
            .unwrap()
            .leaf()
            .unwrap()
            .with_rust::<i32, _>(|v| *v),
        Ok(42)
    );
    let moved = value.take_path_field(&[DynamicPathStep::Field(0)]).unwrap();
    assert!(value.view().field(0).is_err());
    let some = DynamicValue::some(option.clone(), moved).unwrap();
    assert_eq!(
        some.view()
            .option_item()
            .unwrap()
            .leaf()
            .unwrap()
            .with_rust::<i32, _>(|v| *v),
        Ok(42)
    );
    assert!(
        DynamicValue::none(option)
            .unwrap()
            .view()
            .option_item()
            .is_err()
    );
}
