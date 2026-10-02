use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn registered_owned_operation_moves_non_clone_child_and_drops_once() {
    struct Probe(i32, Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.1.set(self.1.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(DynamicType::new(layout.clone()).register_owned_operation(
        "consume",
        |value| {
            let child = value.take_option()?.ok_or("missing child")?;
            let probe = child.into_rust::<Probe>().map_err(|error| error.1)?;
            Ok(probe.0)
        },
    ));
    let make = || {
        DynamicObject::new(
            descriptor.clone(),
            DynamicValue::some(
                layout.clone(),
                DynamicValue::from_rust(item.clone(), Probe(42, drops.clone())).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(make().call_owned("consume").unwrap(), 42);
    assert_eq!(drops.get(), 1);
    let object = make();
    let alias = object.clone();
    assert!(object.call_owned("consume").is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(alias.call_owned("consume").unwrap(), 42);
    assert_eq!(drops.get(), 2);
}

#[test]
fn copy_option_uses_inline_handles_and_registered_operations() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(
        DynamicType::<bool>::new(layout.clone())
            .register_method("is_some", |context| context.option_is_some()),
    );
    let object = DynamicObject::new(
        descriptor,
        DynamicValue::some(layout, DynamicValue::from_rust(item, 42).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(object.is_inline());
    assert_eq!(object.call("is_some", &[]), Some(Ok(true)));
    assert_eq!(object.clone().call("is_some", &[]), Some(Ok(true)));
    assert_eq!(
        object.copy_owned().unwrap().call("is_some", &[]),
        Some(Ok(true))
    );
}

#[test]
fn noncopy_option_shares_payload_and_drops_it_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(DynamicType::<()>::new(layout.clone()));
    let object = DynamicObject::new(
        descriptor,
        DynamicValue::some(
            layout,
            DynamicValue::from_rust(item, Probe(drops.clone())).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!object.is_inline());
    assert!(object.copy_owned().is_err());
    let alias = object.clone();
    drop(object);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn dynamic_object_accepts_equivalent_layouts_and_rejects_incompatible_ones() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let first = DynamicLayout::option(item.clone()).unwrap();
    let second = DynamicLayout::option(item).unwrap();
    let value = DynamicValue::none(first).unwrap();
    assert!(DynamicObject::<()>::new(Rc::new(DynamicType::new(second)), value).is_ok());
    let wrong = DynamicLayout::option(DynamicLayout::copy_of::<u32>(Type::Integer(
        rils_syntax::IntegerType::U32,
    )))
    .unwrap();
    let value = DynamicValue::none(wrong).unwrap();
    let expected = DynamicLayout::option(DynamicLayout::copy_of::<i32>(Type::I32)).unwrap();
    assert!(DynamicObject::<()>::new(Rc::new(DynamicType::new(expected)), value).is_err());
}

#[test]
fn dynamic_object_moves_only_when_its_payload_is_unique() {
    let item = DynamicLayout::of::<String>(Type::String);
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(DynamicType::<()>::new(layout.clone()));
    let make = || {
        DynamicObject::new(
            descriptor.clone(),
            DynamicValue::some(
                layout.clone(),
                DynamicValue::from_rust(item.clone(), "owned".to_owned()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let object = make();
    let alias = object.clone();
    let (object, message) = match object.into_value() {
        Ok(_) => panic!("a shared payload cannot move"),
        Err(failure) => *failure,
    };
    assert!(message.contains("shared"));
    drop(alias);
    let value = object
        .into_value()
        .unwrap_or_else(|_| panic!("unique payload moves"));
    let child = value.take_option().unwrap().unwrap();
    assert_eq!(
        child
            .into_rust::<String>()
            .unwrap_or_else(|_| panic!("string moves")),
        "owned"
    );
}
