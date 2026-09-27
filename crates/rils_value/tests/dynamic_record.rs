use std::{cell::Cell, mem, rc::Rc};

use rils_syntax::{IntegerType, Type};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn record_layout_calculates_aligned_offsets_and_field_lookup() {
    let byte = DynamicLayout::copy_of::<u8>(Type::Integer(IntegerType::U8));
    let wide = DynamicLayout::copy_of::<u64>(Type::Integer(IntegerType::U64));
    let record = DynamicLayout::record(
        Type::named("Pair"),
        vec![("low".into(), byte.clone()), ("high".into(), wide.clone())],
    )
    .unwrap();
    let fields = record.record_fields().unwrap();
    assert_eq!(record.record_field_index("low"), Some(0));
    assert_eq!(record.record_field_index("high"), Some(1));
    assert_eq!(record.record_field_index("missing"), None);
    assert_eq!(fields[0].name(), "low");
    assert_eq!(
        fields[0].layout().rils_type(),
        &Type::Integer(IntegerType::U8)
    );
    assert_eq!(fields[0].offset() % mem::align_of::<u8>(), 0);
    assert_eq!(fields[1].offset() % mem::align_of::<u64>(), 0);
    assert!(fields[1].offset() >= fields[0].offset() + mem::size_of::<u8>());
    assert!(record.layout().size() >= fields[1].offset() + mem::size_of::<u64>());
    assert_eq!(record.layout().align(), mem::align_of::<u64>());
    assert!(record.is_copy());

    let mut value = DynamicValue::record(
        record,
        vec![
            DynamicValue::from_rust(byte, 7_u8).unwrap(),
            DynamicValue::from_rust(wide, 10_u64).unwrap(),
        ],
    )
    .unwrap();
    assert!(value.is_inline());
    assert_eq!(value.with_field::<u64, _>(1, |number| *number), Ok(10));
    value
        .with_field_mut::<u64, _>(1, |number| *number = 12)
        .unwrap();
    assert_eq!(value.with_field::<u64, _>(1, |number| *number), Ok(12));
    assert!(value.with_field::<u8, _>(1, |_| ()).is_err());
    assert!(value.with_field::<u8, _>(2, |_| ()).is_err());
    assert_eq!(
        value.copy_owned().unwrap().with_field::<u8, _>(0, |n| *n),
        Ok(7)
    );
}

#[test]
fn nested_record_moves_each_noncopy_field_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let inner =
        DynamicLayout::record(Type::named("Inner"), vec![("value".into(), probe.clone())]).unwrap();
    let optional = DynamicLayout::option(inner.clone()).unwrap();
    let outer = DynamicLayout::record(
        Type::named("Outer"),
        vec![("inner".into(), optional.clone())],
    )
    .unwrap();
    let inner_value = DynamicValue::record(
        inner,
        vec![DynamicValue::from_rust(probe.clone(), Probe(drops.clone())).unwrap()],
    )
    .unwrap();
    let mut outer_value = DynamicValue::record(
        outer,
        vec![DynamicValue::some(optional, inner_value).unwrap()],
    )
    .unwrap();
    assert!(!outer_value.is_inline());
    assert!(outer_value.copy_owned().is_err());
    let option_value = outer_value.take_field(0).unwrap();
    assert_eq!(outer_value.field_is_live(0), Ok(false));
    assert!(outer_value.take_field(0).is_err());
    drop(outer_value);
    assert_eq!(drops.get(), 0);
    let mut inner_value = option_value.take_option().unwrap().unwrap();
    let probe_value = inner_value.take_field(0).unwrap();
    assert!(inner_value.with_field::<Probe, _>(0, |_| ()).is_err());
    drop(inner_value);
    assert_eq!(drops.get(), 0);
    drop(probe_value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn empty_option_and_replaced_field_preserve_drop_state() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let optional = DynamicLayout::option(probe.clone()).unwrap();
    let record = DynamicLayout::record(
        Type::named("Container"),
        vec![("item".into(), optional.clone())],
    )
    .unwrap();
    let mut value =
        DynamicValue::record(record, vec![DynamicValue::none(optional.clone()).unwrap()]).unwrap();
    let empty = value.take_field(0).unwrap();
    assert_eq!(empty.is_some(), Ok(false));
    assert!(empty.take_option().unwrap().is_none());
    value
        .put_field(
            0,
            DynamicValue::some(
                optional,
                DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(value.field_is_live(0), Ok(true));
    assert!(
        value
            .put_field(
                0,
                DynamicValue::record(
                    DynamicLayout::record(Type::named("Other"), vec![]).unwrap(),
                    vec![]
                )
                .unwrap()
            )
            .is_err()
    );
    drop(value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn record_rejects_invalid_field_layouts() {
    let first = DynamicLayout::copy_of::<i32>(Type::I32);
    let same_type_different_descriptor = DynamicLayout::copy_of::<i32>(Type::I32);
    let record =
        DynamicLayout::record(Type::named("One"), vec![("item".into(), first.clone())]).unwrap();
    assert!(
        DynamicLayout::record(
            Type::named("Duplicate"),
            vec![
                ("item".into(), first.clone()),
                ("item".into(), first.clone())
            ]
        )
        .is_err()
    );
    assert!(DynamicLayout::record(Type::I32, vec![]).is_err());
    assert!(DynamicValue::record(record.clone(), vec![]).is_err());
    assert!(
        DynamicValue::record(
            record.clone(),
            vec![DynamicValue::from_rust(same_type_different_descriptor.clone(), 1).unwrap()]
        )
        .is_err()
    );
    let mut value =
        DynamicValue::record(record, vec![DynamicValue::from_rust(first, 2).unwrap()]).unwrap();
    assert!(
        value
            .put_field(
                0,
                DynamicValue::from_rust(same_type_different_descriptor, 3).unwrap()
            )
            .is_err()
    );
    assert_eq!(value.with_field::<i32, _>(0, |n| *n), Ok(2));
}

#[test]
fn registered_operation_can_access_record_field_without_exposing_bytes() {
    let integer = DynamicLayout::copy_of::<i32>(Type::I32);
    let record = DynamicLayout::record(
        Type::named("Counter"),
        vec![("count".into(), integer.clone())],
    )
    .unwrap();
    let descriptor = Rc::new(DynamicType::<i32>::new(record.clone()).register_method(
        "increment",
        |context| {
            context.record_field_mut::<i32, _>(0, |count| {
                *count += 1;
                *count
            })
        },
    ));
    let object = DynamicObject::new(
        descriptor,
        DynamicValue::record(record, vec![DynamicValue::from_rust(integer, 41).unwrap()]).unwrap(),
    )
    .unwrap();
    assert_eq!(object.call("increment", &[]), Some(Ok(42)));
    assert_eq!(
        object.with(|value| value.with_field::<i32, _>(0, |count| *count)),
        Ok(Ok(42))
    );
}

#[test]
fn taken_field_can_transfer_a_rust_payload_without_clone() {
    let drops = Rc::new(Cell::new(0));
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let record =
        DynamicLayout::record(Type::named("Holder"), vec![("probe".into(), probe.clone())])
            .unwrap();
    let mut value = DynamicValue::record(
        record,
        vec![DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap()],
    )
    .unwrap();
    let field = value.take_field(0).unwrap();
    let (field, _) = field.into_rust::<String>().err().unwrap();
    let payload = field.into_rust::<Probe>().ok().unwrap();
    assert!(Rc::ptr_eq(&payload.0, &drops));
    drop(value);
    assert_eq!(drops.get(), 0);
    drop(payload);
    assert_eq!(drops.get(), 1);
}

#[test]
fn highly_aligned_field_uses_aligned_heap_storage() {
    #[repr(align(64))]
    #[derive(Clone, Copy)]
    struct Aligned(u8);

    let byte = DynamicLayout::copy_of::<u8>(Type::Integer(IntegerType::U8));
    let aligned = DynamicLayout::copy_of::<Aligned>(Type::named("Aligned"));
    let record = DynamicLayout::record(
        Type::named("Wide"),
        vec![
            ("byte".into(), byte.clone()),
            ("aligned".into(), aligned.clone()),
        ],
    )
    .unwrap();
    assert_eq!(record.layout().align(), 64);
    assert_eq!(record.record_fields().unwrap()[1].offset() % 64, 0);
    let value = DynamicValue::record(
        record,
        vec![
            DynamicValue::from_rust(byte, 1_u8).unwrap(),
            DynamicValue::from_rust(aligned, Aligned(9)).unwrap(),
        ],
    )
    .unwrap();
    assert!(!value.is_inline());
    assert_eq!(value.with_field::<Aligned, _>(1, |item| item.0), Ok(9));
}
