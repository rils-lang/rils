use std::{cell::Cell, rc::Rc};

use rils_execution::{
    FloatType, IntegerType, Type, Value,
    environment::{Environment, StorageSlot},
    value::{
        HashKey, ReferenceValue, record_codec::NativeRecordCodec,
        record_layout::RecordLayoutResolver,
    },
};
use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

fn value(payload: DynamicValue) -> Value {
    Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload).unwrap(),
    )
}

fn native(input: Value, ty: &Type) -> Value {
    let layout = RecordLayoutResolver::new(&[]).resolve(ty).unwrap();
    NativeRecordCodec::new()
        .from_native(NativeRecordCodec::new().into_native(input, layout).unwrap())
        .unwrap()
}

#[test]
fn every_registered_scalar_formats_through_nested_sum_views() {
    for (input, ty, display, debug) in [
        (Value::Unit, Type::Unit, "()", "()"),
        (Value::Bool(true), Type::Bool, "true", "true"),
        (
            Value::from_i8(-7),
            Type::Integer(IntegerType::I8),
            "-7",
            "-7",
        ),
        (
            Value::from_i16(-7),
            Type::Integer(IntegerType::I16),
            "-7",
            "-7",
        ),
        (Value::from_i32(-7), Type::I32, "-7", "-7"),
        (
            Value::from_i64(-7),
            Type::Integer(IntegerType::I64),
            "-7",
            "-7",
        ),
        (
            Value::from_i128(-7),
            Type::Integer(IntegerType::I128),
            "-7",
            "-7",
        ),
        (
            Value::from_isize(-7),
            Type::Integer(IntegerType::Isize),
            "-7",
            "-7",
        ),
        (Value::from_u8(7), Type::Integer(IntegerType::U8), "7", "7"),
        (
            Value::from_u16(7),
            Type::Integer(IntegerType::U16),
            "7",
            "7",
        ),
        (
            Value::from_u32(7),
            Type::Integer(IntegerType::U32),
            "7",
            "7",
        ),
        (
            Value::from_u64(7),
            Type::Integer(IntegerType::U64),
            "7",
            "7",
        ),
        (
            Value::from_u128(7),
            Type::Integer(IntegerType::U128),
            "7",
            "7",
        ),
        (
            Value::from_usize(7),
            Type::Integer(IntegerType::Usize),
            "7",
            "7",
        ),
        (
            Value::from_f32(1.5),
            Type::Float(FloatType::F32),
            "1.5",
            "1.5",
        ),
        (Value::from_f64(1.5), Type::F64, "1.5", "1.5"),
        (Value::from_f32(1.0), Type::Float(FloatType::F32), "1", "1"),
        (Value::from_f64(1.0), Type::F64, "1", "1"),
        (Value::from_f64(-0.0), Type::F64, "-0", "-0"),
        (Value::from_f64(f64::INFINITY), Type::F64, "inf", "inf"),
        (Value::from_f64(f64::NAN), Type::F64, "NaN", "NaN"),
        (Value::from_char('x'), Type::Char, "x", "'x'"),
        (Value::from_string("text"), Type::String, "text", "\"text\""),
    ] {
        let leaf = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
        let option = DynamicLayout::option(leaf.clone()).unwrap();
        let result = DynamicLayout::variant(
            Type::Result(Box::new(option.rils_type().clone()), Box::new(Type::Unit)),
            vec![option.clone(), DynamicLayout::copy_of::<()>(Type::Unit)],
        )
        .unwrap();
        let payload = NativeRecordCodec::new().into_native(input, leaf).unwrap();
        let wrapped = value(
            DynamicValue::variant(result, 0, DynamicValue::some(option, payload).unwrap()).unwrap(),
        );
        assert_eq!(format!("{wrapped}"), format!("Ok(Some({display}))"), "{ty}");
        assert_eq!(format!("{wrapped:?}"), format!("Ok(Some({debug}))"), "{ty}");
    }
}

#[test]
fn formatting_non_clone_records_and_vectors_reads_original_bytes() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let string = RecordLayoutResolver::new(&[])
        .resolve(&Type::String)
        .unwrap();
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![
            ("probe".into(), probe.clone()),
            ("text".into(), string.clone()),
        ],
    )
    .unwrap();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Record")],
        },
        record.clone(),
    );
    let option = DynamicLayout::option(vector.clone()).unwrap();
    let payload = DynamicValue::some(
        option,
        DynamicValue::sequence(
            vector,
            vec![
                DynamicValue::record(
                    record,
                    vec![
                        DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap(),
                        DynamicValue::from_rust(string, NativeString::from("owned".to_owned()))
                            .unwrap(),
                    ],
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let original = value(payload);
    let environment = Environment::global();
    environment
        .borrow_mut()
        .define("original", original, true, None);
    let slot = environment.borrow().slot("original").unwrap();
    let reference = Rc::new(ReferenceValue::new_storage(slot.clone(), false));
    assert!(
        reference
            .project_native_step(rils_value::DynamicPathStep::Some)
            .unwrap()
            .unwrap()
            .read()
            .is_err(),
        "Probe cannot be cloned through a reference"
    );
    let borrowed = Value::Reference(reference);
    assert_eq!(
        format!("{borrowed}"),
        "Some([Record { probe: <Probe>, text: owned }])"
    );
    assert_eq!(
        format!("{borrowed:?}"),
        "Some([Record { probe: <Probe>, text: \"owned\" }])"
    );
    assert_eq!(drops.get(), 0);
    drop(borrowed);
    drop(slot.borrow_mut().take().unwrap());
    assert_eq!(drops.get(), 1);
}

#[test]
fn sum_hash_keys_retain_native_storage_and_complete_type_witnesses() {
    let option_type = Type::Option(Box::new(Type::String));
    let result_type = Type::Result(Box::new(option_type.clone()), Box::new(Type::I32));
    let option = |item: Option<&str>| Value::Option {
        value: item.map(|text| Rc::new(Value::from_string(text))),
        element_type: Some(Type::String),
    };
    for (input, ty) in [
        (option(None), option_type.clone()),
        (option(Some("key")), option_type.clone()),
        (
            Value::Result {
                value: Ok(Rc::new(option(Some("key")))),
                ok_type: Some(option_type.clone()),
                error_type: Some(Type::I32),
            },
            result_type.clone(),
        ),
        (
            Value::Result {
                value: Err(Rc::new(Value::from_i32(42))),
                ok_type: Some(option_type.clone()),
                error_type: Some(Type::I32),
            },
            result_type,
        ),
    ] {
        let original = native(input, &ty);
        let key = HashKey::from_value(&original).unwrap();
        let stored = key.to_value().unwrap();
        assert!(
            matches!(stored, Value::Dynamic(_)),
            "key must not rebuild legacy sum storage"
        );
        assert_eq!(Type::of_value(&stored), Some(ty.clone()));
        assert_eq!(format!("{stored:?}"), format!("{original:?}"));
        let mut slot = StorageSlot::uninitialized(false);
        slot.initialize(original);
        let slot = Rc::new(std::cell::RefCell::new(slot));
        let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(slot, false)));
        assert_eq!(HashKey::from_value(&reference).unwrap(), key);
        assert_eq!(HashKey::from_value(&stored).unwrap(), key);
    }
}
