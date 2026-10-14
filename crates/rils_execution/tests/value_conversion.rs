use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{DynamicObject, HostObject, HostType, NativeObject, NativeType, native_char},
};
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

struct DropProbe(Rc<Cell<usize>>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn storage_conversions_move_non_copy_payloads_and_drop_them_once() {
    let drops = Rc::new(Cell::new(0));
    let native = NativeObject::new(
        Rc::new(NativeType::new::<DropProbe>(Type::named("Probe"))),
        DropProbe(drops.clone()),
    )
    .unwrap();
    let value: Value = native.into();
    assert_eq!(drops.get(), 0);
    drop(value);
    assert_eq!(drops.get(), 1);

    let layout = DynamicLayout::of::<DropProbe>(Type::named("Probe"));
    let payload = DynamicValue::from_rust(layout.clone(), DropProbe(drops.clone())).unwrap();
    let dynamic = DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap();
    let value = Value::from(dynamic);
    assert_eq!(drops.get(), 1);
    drop(value);
    assert_eq!(drops.get(), 2);

    let host = Rc::new(HostObject {
        type_definition: Rc::new(HostType {
            name: "Probe".into(),
            base_types: Default::default(),
            copy: false,
            methods: Default::default(),
        }),
        payload: Rc::new(DropProbe(drops.clone())),
    });
    let identity = Rc::downgrade(&host);
    let value = Value::from(host);
    let Value::HostObject(handle) = &value else {
        panic!("host storage lost")
    };
    assert!(Rc::ptr_eq(handle, &identity.upgrade().unwrap()));
    drop(value);
    assert!(identity.upgrade().is_none());
    assert_eq!(drops.get(), 3);
}

#[test]
fn character_factory_is_usable_before_entering_runtime_storage() {
    for character in ['\0', 'a', '界', '🦀', '\u{10ffff}'] {
        let native: NativeObject = native_char(character);
        assert_eq!(native.with::<char, _>(|value| *value).unwrap(), character);
        let value: Value = native.into();
        assert_eq!(value.get_cloned::<char>().ok(), Some(character));
        assert_eq!(Type::of_value(&value), Some(Type::Char));
    }
}
