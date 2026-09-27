use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    environment::{AccessError, Environment, StorageSlot},
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    value::{NativeChildren, NativeObject, NativeType, ReferenceValue, native_ops},
};
use rils_stdlib::stdlib::range::Range;

fn native<T: 'static>(descriptor: Rc<NativeType>, value: T) -> Value {
    Value::Native(NativeObject::new(descriptor, value).unwrap())
}

fn native_buffer_descriptor() -> Rc<NativeType> {
    Rc::new(
        NativeType::new::<Vec<i32>>(Type::named("NativeBuffer"))
            .register_method(native_ops::CLONE, |context| {
                let clone = context.receiver::<Vec<i32>, _>(Clone::clone)?;
                Ok(Value::Native(context.new_object(clone)?))
            })
            .register_method(native_ops::EQUAL, |context| {
                let [Value::Native(other)] = context.arguments() else {
                    return Err("expected one native argument".into());
                };
                Ok(Value::Bool(context.receiver::<Vec<i32>, _>(|left| {
                    other.with::<Vec<i32>, _>(|right| left == right)
                })??))
            }),
    )
}

#[test]
fn native_stdlib_range_keeps_its_rust_layout_and_rils_place_rules() {
    let range_type = Type::Named {
        name: "Range".into(),
        arguments: vec![Type::I32],
    };
    let descriptor = Rc::new(NativeType::new::<Range<i32>>(range_type.clone()));
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(native(descriptor, Range::from_bounds(2, 4)));
    let slot = Rc::new(RefCell::new(slot));

    let first = ReferenceValue::new_storage(slot.clone(), true);
    let second = ReferenceValue::new_storage(slot.clone(), true);
    assert_eq!(slot.borrow_mut().take().err(), Some(AccessError::Borrowed));

    let first_handle = first.read().unwrap();
    let Value::Native(first_handle) = first_handle else {
        unreachable!()
    };
    assert_eq!(
        first_handle
            .with_mut::<Range<i32>, _>(|range| range.next())
            .unwrap(),
        Some(2)
    );
    let second_handle = second.read().unwrap();
    let Value::Native(second_handle) = second_handle else {
        unreachable!()
    };
    assert_eq!(
        second_handle
            .with_mut::<Range<i32>, _>(|range| range.next())
            .unwrap(),
        Some(3)
    );
    assert_eq!(
        second_handle.with::<Range<i32>, _>(Range::current).unwrap(),
        4
    );

    assert!(range_type.accepts(&Value::Native(second_handle.clone())));
    assert!(!Type::named("Vec").accepts(&Value::Native(second_handle)));
    drop(first);
    drop(second);
    assert!(matches!(slot.borrow_mut().take(), Ok(Value::Native(_))));
    assert_eq!(slot.borrow_mut().take().err(), Some(AccessError::Moved));
}

#[test]
fn copy_and_clone_are_distinct_native_operations() {
    let copy_descriptor =
        Rc::new(NativeType::new::<i32>(Type::named("NativeNumber")).with_copy::<i32>());
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(native(copy_descriptor, 7_i32));
    let Value::Native(copy) = slot.take().unwrap() else {
        unreachable!()
    };
    assert!(copy.is_inline());
    assert_eq!(copy.with::<i32, _>(|number| *number).unwrap(), 7);
    let Value::Native(original) = slot.read().unwrap() else {
        unreachable!()
    };
    assert_eq!(original.with::<i32, _>(|number| *number).unwrap(), 7);

    let clone_descriptor = native_buffer_descriptor();
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(native(clone_descriptor, vec![1, 2]));
    assert_eq!(slot.take().unwrap().type_name(), "NativeBuffer");
    assert_eq!(slot.take().err(), Some(AccessError::Moved));
}

#[test]
fn native_handles_share_one_payload_but_explicit_clone_has_own_storage() {
    let descriptor = native_buffer_descriptor();
    let value = native(descriptor.clone(), vec![1, 2]);
    let shared = value.clone();
    let owned = value.clone_owned().unwrap();
    let Value::Native(shared) = shared else {
        unreachable!()
    };
    shared
        .with_mut::<Vec<i32>, _>(|buffer| buffer.push(3))
        .unwrap();
    assert_eq!(value, native(descriptor.clone(), vec![1, 2, 3]));
    assert_eq!(owned, native(descriptor, vec![1, 2]));

    let Value::Native(value) = value else {
        unreachable!()
    };
    assert!(
        value
            .with_mut::<Vec<i32>, _>(|_| value.with::<Vec<i32>, _>(Vec::len))
            .unwrap()
            .is_err()
    );
    assert!(
        NativeObject::new(
            Rc::new(NativeType::new::<i32>(Type::named("NativeNumber"))),
            "wrong payload",
        )
        .is_err()
    );
}

#[test]
fn moving_a_native_payload_drops_it_once() {
    struct DropProbe(Rc<Cell<usize>>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let descriptor = Rc::new(NativeType::new::<DropProbe>(Type::named("DropProbe")));
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(native(descriptor, DropProbe(drops.clone())));
    let handle = slot.read().unwrap();
    let moved = slot.take().unwrap();
    assert!(moved.clone_owned().is_err());
    drop(handle);
    assert_eq!(drops.get(), 0);
    drop(moved);
    assert_eq!(drops.get(), 1);
}

#[test]
fn native_containers_report_nested_lexical_references() {
    struct NativeBag {
        values: Vec<Value>,
        active: bool,
        partial: bool,
    }
    impl NativeChildren<Value> for NativeBag {
        fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
            self.values.iter().for_each(visit);
        }

        fn has_active_references(&self) -> bool {
            self.active
        }

        fn is_partially_moved(&self) -> bool {
            self.partial
        }
    }

    let environment = Environment::global();
    environment
        .borrow_mut()
        .define("source", Value::from_i32(7), true, None);
    let source = environment.borrow().slot("source").unwrap();
    let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(source, true)));
    let descriptor = Rc::new(
        NativeType::new::<NativeBag>(Type::named("NativeBag")).with_children::<NativeBag>(),
    );
    let value = native(
        descriptor,
        NativeBag {
            values: vec![reference],
            active: false,
            partial: false,
        },
    );
    assert!(value.contains_reference());
    assert!(value.contains_local_reference(&environment));
    assert!(!value.has_active_references());
    assert!(!value.is_partially_moved());

    let Value::Native(object) = &value else {
        unreachable!()
    };
    object
        .with_mut::<NativeBag, _>(|_| {
            assert!(value.contains_reference());
            assert!(value.contains_local_reference(&environment));
            assert!(value.has_active_references());
            assert!(value.is_partially_moved());
        })
        .unwrap();
    object
        .with_mut::<NativeBag, _>(|bag| {
            bag.active = true;
            bag.partial = true;
        })
        .unwrap();
    assert!(value.has_active_references());
    assert!(value.is_partially_moved());
}

#[test]
fn native_iterator_dispatch_follows_registration_instead_of_a_type_name() {
    struct Ticks(i32);
    let descriptor = Rc::new(
        NativeType::new::<Ticks>(Type::named("Ticks")).register_method(
            native_ops::NEXT,
            |context| {
                let item = context.receiver_mut::<Ticks, _>(|ticks| {
                    let item = (ticks.0 < 2).then_some(ticks.0);
                    ticks.0 += 1;
                    item
                })?;
                Ok(Value::Option {
                    value: item.map(|value| Rc::new(Value::from_i32(value))),
                    element_type: Some(Type::I32),
                })
            },
        ),
    );
    let value = native(descriptor, Ticks(0));
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(value).unwrap() else {
        panic!("registered native iterator was not accepted")
    };
    assert!(matches!(
        next_builtin(&mut iterator),
        Some(Ok(Some(value))) if value.as_i32() == Some(0)
    ));
    assert!(matches!(
        next_builtin(&mut iterator),
        Some(Ok(Some(value))) if value.as_i32() == Some(1)
    ));
    assert!(matches!(next_builtin(&mut iterator), Some(Ok(None))));
}
