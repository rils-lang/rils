use std::{cell::Cell, rc::Rc};

use rils_execution::{
    FloatType, IntegerType, Type, Value,
    environment::Environment,
    value::{ReferenceValue, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver},
};
use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

fn value(payload: DynamicValue) -> Value {
    Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload).unwrap(),
    )
}

#[test]
fn every_registered_scalar_formats_through_nested_sum_views() {
    for (input, ty, display, debug) in [
        (
            Value::from(rils_execution::value::native_unit()),
            Type::Unit,
            "()",
            "()",
        ),
        (
            Value::from(rils_execution::value::native_bool(true)),
            Type::Bool,
            "true",
            "true",
        ),
        (
            Value::from(rils_execution::value::native_i8(-7)),
            Type::Integer(IntegerType::I8),
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_i16(-7)),
            Type::Integer(IntegerType::I16),
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_i32(-7)),
            Type::I32,
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_i64(-7)),
            Type::Integer(IntegerType::I64),
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_i128(-7)),
            Type::Integer(IntegerType::I128),
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_isize(-7)),
            Type::Integer(IntegerType::Isize),
            "-7",
            "-7",
        ),
        (
            Value::from(rils_execution::value::native_u8(7)),
            Type::Integer(IntegerType::U8),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_u16(7)),
            Type::Integer(IntegerType::U16),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_u32(7)),
            Type::Integer(IntegerType::U32),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_u64(7)),
            Type::Integer(IntegerType::U64),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_u128(7)),
            Type::Integer(IntegerType::U128),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_usize(7)),
            Type::Integer(IntegerType::Usize),
            "7",
            "7",
        ),
        (
            Value::from(rils_execution::value::native_f32(1.5)),
            Type::Float(FloatType::F32),
            "1.5",
            "1.5",
        ),
        (
            Value::from(rils_execution::value::native_f64(1.5)),
            Type::F64,
            "1.5",
            "1.5",
        ),
        (
            Value::from(rils_execution::value::native_f32(1.0)),
            Type::Float(FloatType::F32),
            "1",
            "1",
        ),
        (
            Value::from(rils_execution::value::native_f64(1.0)),
            Type::F64,
            "1",
            "1",
        ),
        (
            Value::from(rils_execution::value::native_f64(-0.0)),
            Type::F64,
            "-0",
            "-0",
        ),
        (
            Value::from(rils_execution::value::native_f64(f64::INFINITY)),
            Type::F64,
            "inf",
            "inf",
        ),
        (
            Value::from(rils_execution::value::native_f64(f64::NAN)),
            Type::F64,
            "NaN",
            "NaN",
        ),
        (
            Value::from(rils_execution::value::native_char('x')),
            Type::Char,
            "x",
            "'x'",
        ),
        (
            Value::from(rils_execution::value::native_string("text")),
            Type::String,
            "text",
            "\"text\"",
        ),
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
    let borrowed = Value::from_reference(reference);
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
