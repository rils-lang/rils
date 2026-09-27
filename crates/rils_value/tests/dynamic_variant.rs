use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

struct DropProbe(Rc<Cell<usize>>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn nested_variant_moves_its_payload_once() {
    let dropped = Rc::new(Cell::new(0));
    let unit = DynamicLayout::copy_of::<()>(Type::Unit);
    let probe_type = Type::named("Probe");
    let probe = DynamicLayout::of::<DropProbe>(probe_type.clone());
    let result_type = Type::Result(Box::new(Type::Unit), Box::new(probe_type));
    let result = DynamicLayout::variant(result_type.clone(), vec![unit, probe.clone()]).unwrap();
    let record =
        DynamicLayout::record(Type::named("Holder"), vec![("item".into(), result.clone())])
            .unwrap();
    let payload = DynamicValue::from_rust(probe, DropProbe(dropped.clone())).unwrap();
    let variant = DynamicValue::variant(result, 1, payload).unwrap();
    let mut value = DynamicValue::record(record, vec![variant]).unwrap();
    assert_eq!(dropped.get(), 0);
    let (index, payload) = value.take_field(0).unwrap().take_variant().unwrap();
    assert_eq!(index, 1);
    drop(value);
    assert_eq!(dropped.get(), 0);
    drop(payload);
    assert_eq!(dropped.get(), 1);
}

#[test]
fn variant_rejects_wrong_payload_and_preserves_it() {
    let unit = DynamicLayout::copy_of::<()>(Type::Unit);
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let result = DynamicLayout::variant(
        Type::Result(Box::new(Type::Unit), Box::new(Type::I32)),
        vec![unit.clone(), number.clone()],
    )
    .unwrap();
    assert!(
        DynamicValue::variant(
            result.clone(),
            2,
            DynamicValue::from_rust(number.clone(), 1).unwrap()
        )
        .is_err()
    );
    assert!(
        DynamicValue::variant(
            result.clone(),
            0,
            DynamicValue::from_rust(number, 1).unwrap()
        )
        .is_err()
    );
    let value =
        DynamicValue::variant(result, 0, DynamicValue::from_rust(unit, ()).unwrap()).unwrap();
    assert_eq!(value.variant_index(), Ok(0));
}
