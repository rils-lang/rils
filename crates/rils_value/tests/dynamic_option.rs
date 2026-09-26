use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

#[test]
fn generic_option_composes_an_inline_copy_layout() {
    let integer = DynamicLayout::copy_of::<i32>(Type::I32);
    let option = DynamicLayout::option(integer.clone()).unwrap();
    assert_eq!(option.rils_type().to_string(), "Option<i32>");
    assert_eq!(option.layout().align(), std::mem::align_of::<i32>());

    let value = DynamicValue::some(
        option.clone(),
        DynamicValue::from_rust(integer, 17_i32).unwrap(),
    )
    .unwrap();
    assert!(value.is_inline());
    assert_eq!(value.is_some(), Ok(true));
    assert_eq!(
        value.with_option::<i32, _>(|item| item.copied()),
        Ok(Some(17))
    );
    assert!(value.with_option::<usize, _>(|item| item.copied()).is_err());
    let copy = value.copy_owned().unwrap();
    let child = copy.take_option().unwrap().unwrap();
    assert_eq!(child.with::<i32, _>(|value| *value), Ok(17));
    assert_eq!(
        value
            .take_option()
            .unwrap()
            .unwrap()
            .with::<i32, _>(|value| *value),
        Ok(17)
    );

    let empty = DynamicValue::none(option).unwrap();
    assert_eq!(empty.is_some(), Ok(false));
    assert_eq!(empty.with_option::<i32, _>(|item| item.copied()), Ok(None));
    assert!(empty.take_option().unwrap().is_none());
}

#[test]
fn nested_option_moves_and_drops_a_noncopy_payload_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let optional = DynamicLayout::option(probe.clone()).unwrap();
    let nested = DynamicLayout::option(optional.clone()).unwrap();
    let inner = DynamicValue::some(
        optional,
        DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap(),
    )
    .unwrap();
    let outer = DynamicValue::some(nested, inner).unwrap();
    assert!(!outer.is_inline());
    assert!(outer.copy_owned().is_err());
    let inner = outer.take_option().unwrap().unwrap();
    assert_eq!(drops.get(), 0);
    let probe = inner.take_option().unwrap().unwrap();
    assert_eq!(drops.get(), 0);
    assert!(
        probe
            .with::<Probe, _>(|value| Rc::ptr_eq(&value.0, &drops))
            .unwrap()
    );
    drop(probe);
    assert_eq!(drops.get(), 1);
}

#[test]
fn optional_layout_rejects_a_different_child_descriptor() {
    let left = DynamicLayout::copy_of::<i32>(Type::I32);
    let right = DynamicLayout::copy_of::<i32>(Type::I32);
    let optional = DynamicLayout::option(left).unwrap();
    let value = DynamicValue::from_rust(right, 3_i32).unwrap();
    assert!(DynamicValue::some(optional, value).is_err());
}

#[test]
fn none_for_a_noncopy_child_uses_only_an_inline_tag() {
    let string = DynamicLayout::of::<String>(Type::String);
    let optional = DynamicLayout::option(string).unwrap();
    let none = DynamicValue::none(optional).unwrap();
    assert!(none.is_inline());
    assert_eq!(none.is_some(), Ok(false));
}

#[test]
fn nested_none_uses_the_childs_compact_representation() {
    let string = DynamicLayout::of::<String>(Type::String);
    let inner_layout = DynamicLayout::option(string).unwrap();
    let outer_layout = DynamicLayout::option(inner_layout.clone()).unwrap();
    let inner_none = DynamicValue::none(inner_layout).unwrap();
    let outer_some = DynamicValue::some(outer_layout, inner_none).unwrap();
    assert_eq!(outer_some.is_some(), Ok(true));
    let child = outer_some.take_option().unwrap().unwrap();
    assert_eq!(child.is_some(), Ok(false));
}

#[test]
fn dynamic_operation_table_uses_a_checked_receiver_layout() {
    let integer = DynamicLayout::copy_of::<i32>(Type::I32);
    let optional = DynamicLayout::option(integer.clone()).unwrap();
    let operations = DynamicType::<bool>::new(optional.clone())
        .register_method("present", |context| context.option_is_some());
    let mut present = DynamicValue::some(
        optional.clone(),
        DynamicValue::from_rust(integer.clone(), 2_i32).unwrap(),
    )
    .unwrap();
    let mut absent = DynamicValue::none(optional).unwrap();
    let mut wrong = DynamicValue::from_rust(integer, 2_i32).unwrap();
    assert_eq!(
        operations.call(&mut present, "present", &[]),
        Some(Ok(true))
    );
    assert_eq!(
        operations.call(&mut absent, "present", &[]),
        Some(Ok(false))
    );
    assert!(
        operations
            .call(&mut wrong, "present", &[])
            .unwrap()
            .is_err()
    );
    assert!(operations.call(&mut present, "missing", &[]).is_none());
}
