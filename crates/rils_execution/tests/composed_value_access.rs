use std::{cell::Cell, rc::Rc};

use rils_execution::{Type, Value, value::DynamicObject};
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

struct Owned {
    drops: Rc<Cell<usize>>,
    text: String,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn inspecting_non_clone_children_preserves_identity_and_ownership() {
    for kind in ["option", "result", "sequence"] {
        let drops = Rc::new(Cell::new(0));
        let leaf = DynamicLayout::of::<Owned>(Type::named("Owned"));
        let item = DynamicValue::from_rust(
            leaf.clone(),
            Owned {
                drops: drops.clone(),
                text: "borrowed".into(),
            },
        )
        .unwrap();
        let original = item
            .view()
            .with_rust::<Owned, _>(|value| value.text.as_ptr())
            .unwrap();
        let payload = match kind {
            "option" => DynamicValue::some(DynamicLayout::option(leaf).unwrap(), item).unwrap(),
            "result" => DynamicValue::variant(
                DynamicLayout::variant(
                    Type::Result(
                        Box::new(Type::named("Owned")),
                        Box::new(Type::named("Owned")),
                    ),
                    vec![leaf.clone(), leaf],
                )
                .unwrap(),
                1,
                item,
            )
            .unwrap(),
            _ => DynamicValue::sequence(
                DynamicLayout::sequence(Type::named("Sequence"), leaf),
                vec![item],
            )
            .unwrap(),
        };
        let value: Value =
            DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload)
                .unwrap()
                .into();
        value
            .with_native_view(|view| {
                let child = match kind {
                    "option" => {
                        assert!(view.option_is_some().unwrap());
                        view.option_item().unwrap()
                    }
                    "result" => {
                        assert_eq!(view.variant_index().unwrap(), 1);
                        view.variant_payload().unwrap()
                    }
                    _ => {
                        assert_eq!(view.sequence_len().unwrap(), 1);
                        assert!(view.sequence_item(1).is_err());
                        view.sequence_item(0).unwrap()
                    }
                };
                child
                    .with_rust::<Owned, _>(|item| {
                        assert_eq!(item.text, "borrowed");
                        assert_eq!(item.text.as_ptr(), original);
                    })
                    .unwrap();
                assert!(child.with_rust::<i32, _>(|_| ()).is_err());
            })
            .unwrap();
        assert_eq!(drops.get(), 0);
        use rils_execution::{
            environment::StorageSlot,
            value::{ReferenceValue, borrowed_sum::Branch, sum},
        };
        let mut slot = StorageSlot::uninitialized(true);
        slot.initialize(value);
        let slot = Rc::new(std::cell::RefCell::new(slot));
        let reference =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
        assert_eq!(
            sum::branch(&reference).unwrap(),
            match kind {
                "option" => Some(Branch::Some),
                "result" => Some(Branch::Err),
                _ => None,
            }
        );
        reference
            .with_native_view(|view| {
                let child = match kind {
                    "option" => view.option_item().unwrap(),
                    "result" => view.variant_payload().unwrap(),
                    _ => view.sequence_item(0).unwrap(),
                };
                assert_eq!(
                    child
                        .with_rust::<Owned, _>(|item| item.text.as_ptr())
                        .unwrap(),
                    original
                );
            })
            .unwrap();
        assert!(slot.borrow_mut().take().is_err());
        assert_eq!(drops.get(), 0);
        drop(reference);
        drop(slot.borrow_mut().take().unwrap());
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn empty_layouts_expose_tags_and_bounds_without_reading_payloads() {
    let leaf = DynamicLayout::copy_of::<i32>(Type::I32);
    let payload = DynamicValue::none(DynamicLayout::option(leaf.clone()).unwrap()).unwrap();
    let option: Value =
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload)
            .unwrap()
            .into();
    option
        .with_native_view(|view| {
            assert!(!view.option_is_some().unwrap());
            assert!(view.option_item().is_err());
            assert!(view.sequence_len().is_err());
        })
        .unwrap();
    let payload = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Sequence"), leaf),
        vec![],
    )
    .unwrap();
    let sequence: Value =
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload)
            .unwrap()
            .into();
    sequence
        .with_native_view(|view| {
            assert_eq!(view.sequence_len().unwrap(), 0);
            assert!(view.sequence_item(0).is_err());
            assert!(view.variant_index().is_err());
        })
        .unwrap();
}
