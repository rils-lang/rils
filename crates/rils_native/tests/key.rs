use rils_native::{KeyRegistration, NativeKey, NativeRegistry};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, NativeObject, NativeType};
use std::{cell::Cell, rc::Rc};

struct Probe {
    number: i32,
    drops: Rc<Cell<usize>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
static REGISTRY: NativeRegistry = NativeRegistry::with_keys(
    &[],
    &[],
    &[KeyRegistration {
        matches: |ty| ty == &Type::named("Probe"),
        key: |leaf| leaf.with_rust::<Probe, _>(|probe| NativeKey::Signed(probe.number.into())),
        ordered: true,
    }],
);

#[test]
fn keys_borrow_non_clone_payloads_and_preserve_drop_ownership() {
    let drops = Rc::new(Cell::new(0));
    let object = NativeObject::new(
        Rc::new(NativeType::<()>::new::<Probe>(Type::named("Probe"))),
        Probe {
            number: 42,
            drops: drops.clone(),
        },
    )
    .unwrap();
    let value = DynamicValue::from_rust(
        DynamicLayout::of::<Probe>(Type::named("Probe")),
        Probe {
            number: 42,
            drops: drops.clone(),
        },
    )
    .unwrap();
    for _ in 0..3 {
        assert_eq!(
            object.with_leaf(|leaf| REGISTRY.key_leaf(&leaf)),
            Ok(Ok(NativeKey::Signed(42)))
        );
        assert_eq!(REGISTRY.key(value.view()), Ok(NativeKey::Signed(42)));
        assert_eq!(
            REGISTRY.ordered_key(value.view()),
            Ok(NativeKey::Signed(42))
        );
    }
    assert_eq!(drops.get(), 0);
    drop((object, value));
    assert_eq!(drops.get(), 2);
}

#[test]
fn key_support_is_explicit_even_for_primitive_leaves() {
    let value = DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 42_i32).unwrap();
    assert!(NativeRegistry::new(&[], &[]).key(value.view()).is_err());
    let wrong =
        DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::named("Probe")), 42_i32)
            .unwrap();
    assert!(REGISTRY.key(wrong.view()).is_err());
}
