use std::{cell::Cell, rc::Rc};

use rils_native::{EqualityRegistration, NativeRegistry};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, NativeLeafRef, NativeObject, NativeType};

struct Probe {
    number: i32,
    drops: Rc<Cell<usize>>,
}
impl PartialEq for Probe {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

static REGISTRY: NativeRegistry = NativeRegistry::new(&[], &[]).with_equalities(&[
    EqualityRegistration::of::<Probe>(),
    EqualityRegistration::of::<i32>(),
    EqualityRegistration::of::<f64>(),
]);

#[test]
fn registered_equality_reads_non_clone_standalone_and_composed_leaves() {
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
    assert_eq!(
        object.with_leaf(|left| REGISTRY.equal_leaf(&left, &value.view().leaf().unwrap())),
        Ok(Ok(true))
    );
    assert_eq!(drops.get(), 0);
    drop((object, value));
    assert_eq!(drops.get(), 2);
}

#[test]
fn equality_checks_both_rils_identity_and_physical_rust_identity() {
    let left = NativeLeafRef::from_rust(&42_i32, Type::I32);
    assert_eq!(
        REGISTRY.equal_leaf(
            &left,
            &NativeLeafRef::from_rust(&42_i32, Type::named("Other"))
        ),
        Ok(false)
    );
    assert!(
        REGISTRY
            .equal_leaf(&left, &NativeLeafRef::from_rust(&42_u32, Type::I32))
            .unwrap_err()
            .contains("no registered")
    );
    assert!(
        REGISTRY
            .equal_leaf(
                &NativeLeafRef::from_rust(&true, Type::Bool),
                &NativeLeafRef::from_rust(&true, Type::Bool)
            )
            .is_err()
    );
}

#[test]
fn leaf_registration_preserves_partial_equality_of_floats() {
    for (left, right, expected) in [
        (0.0, -0.0, true),
        (f64::NAN, f64::NAN, false),
        (f64::INFINITY, f64::INFINITY, true),
        (1.0, 2.0, false),
    ] {
        assert_eq!(
            REGISTRY.equal_leaf(
                &NativeLeafRef::from_rust(&left, Type::F64),
                &NativeLeafRef::from_rust(&right, Type::F64)
            ),
            Ok(expected)
        );
    }
}

#[test]
fn projected_equality_borrows_non_clone_wrappers_in_both_directions() {
    struct Wrapper(Probe);
    impl AsRef<Probe> for Wrapper {
        fn as_ref(&self) -> &Probe {
            &self.0
        }
    }
    static WRAPPERS: NativeRegistry = NativeRegistry::new(&[], &[])
        .with_equalities(&[EqualityRegistration::projected::<Wrapper, Probe>()]);
    let drops = Rc::new(Cell::new(0));
    let wrapped = Wrapper(Probe {
        number: 42,
        drops: drops.clone(),
    });
    let same_payload = Probe {
        number: 42,
        drops: drops.clone(),
    };
    let different = Probe {
        number: 41,
        drops: drops.clone(),
    };
    let wrapper = NativeLeafRef::from_rust(&wrapped, Type::named("Probe"));
    let same = NativeLeafRef::from_rust(&same_payload, Type::named("Probe"));
    let other = NativeLeafRef::from_rust(&different, Type::named("Probe"));
    for (left, right, expected) in [
        (&wrapper, &same, true),
        (&same, &wrapper, true),
        (&wrapper, &wrapper, true),
        (&same, &same, true),
        (&wrapper, &other, false),
        (&other, &wrapper, false),
    ] {
        assert_eq!(WRAPPERS.equal_leaf(left, right), Ok(expected));
    }
    assert_eq!(drops.get(), 0);
    drop((wrapper, same, other));
    drop((wrapped, same_payload, different));
    assert_eq!(drops.get(), 3);
}

#[test]
fn composed_equality_is_supplied_by_the_registering_type() {
    fn compare(
        left: &rils_value::DynamicValueRef<'_>,
        right: &rils_value::DynamicValueRef<'_>,
        children: rils_native::EqualChildren,
    ) -> Result<bool, String> {
        children(&left.field(0)?, &right.field(0)?)
    }
    fn child(
        left: &rils_value::DynamicValueRef<'_>,
        right: &rils_value::DynamicValueRef<'_>,
    ) -> Result<bool, String> {
        REGISTRY.equal_leaf(&left.leaf()?, &right.leaf()?)
    }
    static VIEWS: NativeRegistry =
        NativeRegistry::new(&[], &[]).with_equalities(&[EqualityRegistration::view(
            |layout| layout.rils_type() == &Type::named("Record"),
            compare,
        )]);
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let layout = DynamicLayout::record(
        Type::named("Record"),
        vec![("number".into(), number.clone())],
    )
    .unwrap();
    let left = DynamicValue::record(
        layout.clone(),
        vec![DynamicValue::from_rust(number.clone(), 1_i32).unwrap()],
    )
    .unwrap();
    let right = DynamicValue::record(
        layout,
        vec![DynamicValue::from_rust(number, 2_i32).unwrap()],
    )
    .unwrap();
    assert_eq!(
        VIEWS.equal_view(&left.view(), &right.view(), child),
        Some(Ok(false))
    );
    assert!(
        REGISTRY
            .equal_view(&left.view(), &right.view(), child)
            .is_none()
    );
}
