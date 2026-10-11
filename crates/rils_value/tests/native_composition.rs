use std::{cell::Cell, mem, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, NativeChildren, NativeObject, NativeType};

struct Probe {
    text: String,
    drops: Rc<Cell<usize>>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn non_clone_leaf_moves_through_tuple_and_option_and_keeps_its_registration() {
    for recover in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(
            NativeType::<usize>::new::<Probe>(Type::named("Probe"))
                .register_method("length", |context| {
                    context.receiver::<Probe, _>(|probe| probe.text.len())
                }),
        );
        let text = "same allocation".to_owned();
        let address = text.as_ptr();
        let object = NativeObject::new(
            descriptor.clone(),
            Probe {
                text,
                drops: drops.clone(),
            },
        )
        .unwrap();
        assert!(Rc::ptr_eq(&descriptor, &object.descriptor_handle()));
        let leaf = descriptor.layout_handle();
        assert_eq!(leaf.layout(), std::alloc::Layout::new::<Probe>());
        assert!(leaf.is_rust_type::<Probe>());
        assert!(!leaf.is_rust_type::<NativeObject<usize>>());
        let tuple = DynamicLayout::aggregate(
            Type::Tuple(vec![Type::named("Probe")]),
            vec![("0".into(), leaf)],
        )
        .unwrap();
        let option = DynamicLayout::option(tuple.clone()).unwrap();
        let value = object.into_dynamic().ok().unwrap();
        assert_eq!(
            value.with::<Probe, _>(|probe| probe.text.as_ptr()),
            Ok(address)
        );
        let tuple = DynamicValue::record(tuple, vec![value]).unwrap();
        let option = DynamicValue::some(option, tuple).unwrap();
        assert_eq!(drops.get(), 0);
        assert!(option.copy_owned().is_err());
        if recover {
            let mut tuple = option.take_option().unwrap().unwrap();
            let value = tuple.take_field(0).unwrap();
            drop(tuple);
            let object = descriptor.from_dynamic(value).ok().unwrap();
            assert!(Rc::ptr_eq(&descriptor, &object.descriptor_handle()));
            assert_eq!(object.call("length", &[]).unwrap(), Ok(15));
            assert_eq!(
                object.with::<Probe, _>(|probe| probe.text.as_ptr()),
                Ok(address)
            );
            assert_eq!(drops.get(), 0);
            drop(object);
        } else {
            drop(option);
        }
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn shared_non_copy_move_failure_returns_the_original_owner() {
    let drops = Rc::new(Cell::new(0));
    let object = NativeObject::<()>::new(
        Rc::new(NativeType::new::<Probe>(Type::named("Probe"))),
        Probe {
            text: "shared".into(),
            drops: drops.clone(),
        },
    )
    .unwrap();
    let alias = object.clone();
    let (object, error) = *object.into_dynamic().err().unwrap();
    assert!(error.contains("shared"));
    assert_eq!(drops.get(), 0);
    drop(alias);
    drop(object.into_dynamic().ok().unwrap());
    assert_eq!(drops.get(), 1);
}

#[test]
fn copy_requires_explicit_registration_even_for_rust_copy_types() {
    for declared_copy in [false, true] {
        let mut descriptor = NativeType::<()>::new::<i32>(Type::I32);
        if declared_copy {
            descriptor = descriptor.with_copy::<i32>();
        }
        let descriptor = Rc::new(descriptor);
        let layout = descriptor.layout_handle();
        assert_eq!(layout.is_copy(), declared_copy);
        // Empty containers must still retain their element's Copy policy.
        assert_eq!(
            DynamicLayout::array(layout.clone(), 0).unwrap().is_copy(),
            declared_copy
        );
        assert_eq!(
            DynamicLayout::option(layout).unwrap().is_copy(),
            declared_copy
        );
        let object = NativeObject::new(descriptor.clone(), 17_i32).unwrap();
        let dynamic = object.into_dynamic().ok().unwrap();
        assert_eq!(dynamic.is_inline(), declared_copy);
        assert_eq!(dynamic.copy_owned().is_ok(), declared_copy);
        let object = descriptor.from_dynamic(dynamic).ok().unwrap();
        assert_eq!(object.descriptor().is_copy(), declared_copy);
        assert_eq!(object.into_rust::<i32>().ok(), Some(17));
    }
}

#[derive(Clone, Copy)]
#[repr(align(64))]
struct Aligned([u8; 64]);

#[test]
fn shared_large_copy_is_detached_and_alignment_survives_composition() {
    let descriptor =
        Rc::new(NativeType::<()>::new::<Aligned>(Type::named("Aligned")).with_copy::<Aligned>());
    let object = NativeObject::new(descriptor.clone(), Aligned([7; 64])).unwrap();
    let alias = object.clone();
    let value = object.into_dynamic().ok().unwrap();
    alias
        .with_mut::<Aligned, _>(|value| value.0[0] = 9)
        .unwrap();
    let tuple = DynamicLayout::aggregate(
        Type::Tuple(vec![Type::named("Aligned")]),
        vec![("0".into(), descriptor.layout_handle())],
    )
    .unwrap();
    assert_eq!(
        tuple.record_fields().unwrap()[0].offset() % mem::align_of::<Aligned>(),
        0
    );
    let mut tuple = DynamicValue::record(tuple, vec![value]).unwrap();
    let value = tuple.take_field(0).unwrap();
    assert_eq!(value.with::<Aligned, _>(|value| value.0[0]), Ok(7));
    let recovered = descriptor.from_dynamic(value).ok().unwrap();
    recovered
        .with_mut::<Aligned, _>(|value| value.0[0] = 11)
        .unwrap();
    assert_eq!(alias.with::<Aligned, _>(|value| value.0[0]), Ok(9));
}

#[test]
fn layout_mismatch_returns_payload_for_retry_without_drop() {
    let drops = Rc::new(Cell::new(0));
    let descriptor = Rc::new(NativeType::<()>::new::<Probe>(Type::named("Probe")));
    let other_name = Rc::new(NativeType::<()>::new::<Probe>(Type::named("Other")));
    let other_rust_type = Rc::new(NativeType::<()>::new::<String>(Type::named("Probe")));
    let value = NativeObject::new(
        descriptor.clone(),
        Probe {
            text: "retry".into(),
            drops: drops.clone(),
        },
    )
    .unwrap()
    .into_dynamic()
    .ok()
    .unwrap();
    let (value, _) = *other_name.from_dynamic(value).err().unwrap();
    let (value, _) = *other_rust_type.from_dynamic(value).err().unwrap();
    assert_eq!(drops.get(), 0);
    drop(descriptor.from_dynamic(value).ok().unwrap());
    assert_eq!(drops.get(), 1);

    let copy = Rc::new(NativeType::<()>::new::<i32>(Type::I32).with_copy::<i32>());
    let no_copy = Rc::new(NativeType::<()>::new::<i32>(Type::I32));
    for (source, target) in [(&copy, &no_copy), (&no_copy, &copy)] {
        let value = NativeObject::new(source.clone(), 42_i32)
            .unwrap()
            .into_dynamic()
            .ok()
            .unwrap();
        let (value, _) = *target.from_dynamic(value).err().unwrap();
        assert_eq!(
            source
                .from_dynamic(value)
                .ok()
                .unwrap()
                .into_rust::<i32>()
                .ok(),
            Some(42)
        );
    }
}

struct Children {
    active: bool,
    partial: bool,
    child: i32,
}

impl NativeChildren<i32> for Children {
    fn visit_values(&self, visit: &mut dyn FnMut(&i32)) {
        visit(&self.child);
    }
    fn has_active_references(&self) -> bool {
        self.active
    }
    fn is_partially_moved(&self) -> bool {
        self.partial
    }
}

#[test]
fn child_state_rejects_moves_and_visitors_survive_recovery() {
    let descriptor =
        Rc::new(NativeType::new::<Children>(Type::named("Children")).with_children::<Children>());
    for (active, partial) in [(true, false), (false, true)] {
        let object = NativeObject::new(
            descriptor.clone(),
            Children {
                active,
                partial,
                child: 42,
            },
        )
        .unwrap();
        let (object, error) = *object.into_dynamic().err().unwrap();
        assert!(error.contains(if active {
            "referenced"
        } else {
            "partially moved"
        }));
        assert!(object.any_child(|value| *value == 42));
        object
            .with_mut::<Children, _>(|value| {
                value.active = false;
                value.partial = false;
            })
            .unwrap();
        let value = object.into_dynamic().ok().unwrap();
        let object = descriptor.from_dynamic(value).ok().unwrap();
        assert!(object.any_child(|value| *value == 42));
    }
}

#[test]
fn zst_and_standard_payloads_round_trip_without_special_type_adapters() {
    fn round_trip<T: PartialEq + std::fmt::Debug + 'static>(ty: Type, value: T, expected: T) {
        let descriptor = Rc::new(NativeType::<()>::new::<T>(ty));
        let native = NativeObject::new(descriptor.clone(), value).unwrap();
        let dynamic = native.into_dynamic().ok().unwrap();
        assert_eq!(
            dynamic.descriptor().layout(),
            std::alloc::Layout::new::<T>()
        );
        assert_eq!(
            descriptor
                .from_dynamic(dynamic)
                .ok()
                .unwrap()
                .into_rust::<T>()
                .ok()
                .unwrap(),
            expected
        );
    }
    round_trip(Type::Unit, (), ());
    round_trip(Type::USIZE, usize::MAX, usize::MAX);
    round_trip(Type::String, "owned".to_owned(), "owned".to_owned());
    // A registered Rust Option is a Rust leaf, not the composed Option ABI.
    round_trip(
        Type::Option(Box::new(Type::I32)),
        Some(42_i32),
        Some(42_i32),
    );
}

#[test]
fn a_composed_option_is_not_reinterpreted_as_a_rust_option() {
    let ty = Type::Option(Box::new(Type::I32));
    let descriptor = Rc::new(NativeType::<()>::new::<Option<i32>>(ty.clone()));
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let option = DynamicLayout::option(item.clone()).unwrap();
    let value = DynamicValue::some(option, DynamicValue::from_rust(item, 7_i32).unwrap()).unwrap();
    let (value, error) = *descriptor.from_dynamic(value).err().unwrap();
    assert!(error.contains("layout"));
    assert_eq!(value.descriptor().rils_type(), &ty);
    assert_eq!(
        value
            .take_option()
            .unwrap()
            .unwrap()
            .into_rust::<i32>()
            .ok(),
        Some(7)
    );
}

#[test]
fn active_access_to_registered_children_rejects_the_move_without_panicking() {
    let descriptor =
        Rc::new(NativeType::new::<Children>(Type::named("Children")).with_children::<Children>());
    let object = NativeObject::new(
        descriptor.clone(),
        Children {
            active: false,
            partial: false,
            child: 7,
        },
    )
    .unwrap();
    let alias = object.clone();
    let (object, _) = alias
        .with_mut::<Children, _>(|_| *object.into_dynamic().err().unwrap())
        .unwrap();
    drop(alias);
    let object = descriptor
        .from_dynamic(object.into_dynamic().ok().unwrap())
        .ok()
        .unwrap();
    assert!(object.any_child(|child| *child == 7));
}
