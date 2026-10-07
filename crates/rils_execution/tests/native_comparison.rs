use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use rils_execution::{
    FloatType, IntegerType, Type, Value,
    environment::StorageSlot,
    runtime_builtins::call_native_symbol,
    value::{
        FieldSlot, IndexedStorage, ReferenceValue, equality::borrowed_equal,
        record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    },
};
use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicObject, DynamicPathStep, DynamicType, DynamicValue};

fn value(payload: DynamicValue) -> Value {
    Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload).unwrap(),
    )
}

fn native(input: Value, ty: &Type) -> Value {
    let layout = RecordLayoutResolver::new(&[]).resolve(ty).unwrap();
    value(NativeRecordCodec::new().into_native(input, layout).unwrap())
}

fn some(input: Value, ty: &Type) -> Value {
    native(
        Value::Option {
            value: Some(Rc::new(input)),
            element_type: Some(ty.clone()),
        },
        &Type::Option(Box::new(ty.clone())),
    )
}

fn reference(input: Value, mutable: bool) -> (Value, Rc<RefCell<StorageSlot>>) {
    let mut slot = StorageSlot::uninitialized(mutable);
    slot.initialize(input);
    let slot = Rc::new(RefCell::new(slot));
    (
        Value::Reference(Rc::new(ReferenceValue::new_storage(slot.clone(), mutable))),
        slot,
    )
}

#[test]
fn every_scalar_compares_directly_and_inside_native_sums() {
    for (left, equal, different, ty) in [
        (Value::Unit, Value::Unit, None, Type::Unit),
        (
            Value::Bool(true),
            Value::Bool(true),
            Some(Value::Bool(false)),
            Type::Bool,
        ),
        (
            Value::from_i8(-7),
            Value::from_i8(-7),
            Some(Value::from_i8(8)),
            Type::Integer(IntegerType::I8),
        ),
        (
            Value::from_i16(-7),
            Value::I16(-7),
            Some(Value::from_i16(8)),
            Type::Integer(IntegerType::I16),
        ),
        (
            Value::from_i32(-7),
            Value::from_i32(-7),
            Some(Value::from_i32(8)),
            Type::I32,
        ),
        (
            Value::from_i64(-7),
            Value::I64(-7),
            Some(Value::from_i64(8)),
            Type::Integer(IntegerType::I64),
        ),
        (
            Value::from_i128(-7),
            Value::I128(-7),
            Some(Value::from_i128(8)),
            Type::Integer(IntegerType::I128),
        ),
        (
            Value::from_isize(-7),
            Value::Isize(-7),
            Some(Value::from_isize(8)),
            Type::Integer(IntegerType::Isize),
        ),
        (
            Value::from_u8(7),
            Value::U8(7),
            Some(Value::from_u8(8)),
            Type::Integer(IntegerType::U8),
        ),
        (
            Value::from_u16(7),
            Value::U16(7),
            Some(Value::from_u16(8)),
            Type::Integer(IntegerType::U16),
        ),
        (
            Value::from_u32(7),
            Value::U32(7),
            Some(Value::from_u32(8)),
            Type::Integer(IntegerType::U32),
        ),
        (
            Value::from_u64(7),
            Value::U64(7),
            Some(Value::from_u64(8)),
            Type::Integer(IntegerType::U64),
        ),
        (
            Value::from_u128(7),
            Value::U128(7),
            Some(Value::from_u128(8)),
            Type::Integer(IntegerType::U128),
        ),
        (
            Value::from_usize(7),
            Value::Usize(7),
            Some(Value::from_usize(8)),
            Type::USIZE,
        ),
        (
            Value::from_f32(1.5),
            Value::F32(1.5),
            Some(Value::from_f32(2.0)),
            Type::Float(FloatType::F32),
        ),
        (
            Value::from_f64(1.5),
            Value::F64(1.5),
            Some(Value::from_f64(2.0)),
            Type::F64,
        ),
        (
            Value::from_char('x'),
            Value::Char('x'),
            Some(Value::from_char('y')),
            Type::Char,
        ),
        (
            Value::from_string("same"),
            Value::from_string("same"),
            Some(Value::from_string("different")),
            Type::String,
        ),
    ] {
        assert_eq!(
            native(left.clone(), &ty).try_equal(&equal),
            Ok(true),
            "{ty}"
        );
        assert_eq!(
            some(left.clone(), &ty).try_equal(&some(equal, &ty)),
            Ok(true),
            "Option<{ty}>"
        );
        if let Some(different) = different {
            assert_eq!(
                some(left, &ty).try_equal(&some(different, &ty)),
                Ok(false),
                "Option<{ty}>"
            );
        }
    }
}

#[test]
fn option_and_result_comparison_checks_tags_and_full_type_witnesses() {
    let none = |ty: &Type| {
        native(
            Value::Option {
                value: None,
                element_type: Some(ty.clone()),
            },
            &Type::Option(Box::new(ty.clone())),
        )
    };
    assert_eq!(
        none(&Type::String).try_equal(&none(&Type::String)),
        Ok(true)
    );
    assert_eq!(none(&Type::String).try_equal(&none(&Type::I32)), Ok(false));
    assert_eq!(
        none(&Type::String).try_equal(&some(Value::from_string("text"), &Type::String)),
        Ok(false)
    );
    let result = |ok: bool, text: &str| {
        native(
            Value::Result {
                value: if ok {
                    Ok(Rc::new(Value::from_string(text)))
                } else {
                    Err(Rc::new(Value::from_string(text)))
                },
                ok_type: Some(Type::String),
                error_type: Some(Type::String),
            },
            &Type::Result(Box::new(Type::String), Box::new(Type::String)),
        )
    };
    for (left, right, expected) in [
        (result(true, "x"), result(true, "x"), true),
        (result(false, "x"), result(false, "x"), true),
        (result(true, "x"), result(false, "x"), false),
        (result(true, "x"), result(true, "y"), false),
    ] {
        assert_eq!(left.try_equal(&right), Ok(expected));
    }
}

#[test]
fn sum_comparison_preserves_float_partial_equality() {
    for (left, right, expected) in [
        (0.0, -0.0, true),
        (f64::NAN, f64::NAN, false),
        (f64::INFINITY, f64::INFINITY, true),
    ] {
        assert_eq!(
            some(Value::from_f64(left), &Type::F64)
                .try_equal(&some(Value::from_f64(right), &Type::F64)),
            Ok(expected)
        );
    }
}

#[test]
fn generated_scalar_equality_accepts_registered_borrowed_representations() {
    let wrapped = value(
        DynamicValue::from_rust(
            DynamicLayout::copy_of::<rils_stdlib::stdlib::integer::Number<i32>>(Type::I32),
            rils_stdlib::stdlib::integer::Number(42_i32),
        )
        .unwrap(),
    );
    let raw = native(Value::from_i32(42), &Type::I32);
    assert_eq!(wrapped.try_equal(&raw), Ok(true));
    assert_eq!(raw.try_equal(&wrapped), Ok(true));
}

fn nested(text: &str, count: i32, length: usize) -> Value {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let string = RecordLayoutResolver::new(&[])
        .resolve(&Type::String)
        .unwrap();
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![
            ("text".into(), string.clone()),
            ("count".into(), number.clone()),
        ],
    )
    .unwrap();
    let enumeration = DynamicLayout::variant(
        Type::named("Enum"),
        vec![DynamicLayout::copy_of::<()>(Type::Unit), record.clone()],
    )
    .unwrap();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Enum")],
        },
        enumeration.clone(),
    );
    let option = DynamicLayout::option(vector.clone()).unwrap();
    value(
        DynamicValue::some(
            option,
            DynamicValue::sequence(
                vector,
                (0..length)
                    .map(|_| {
                        DynamicValue::variant(
                            enumeration.clone(),
                            1,
                            DynamicValue::record(
                                record.clone(),
                                vec![
                                    DynamicValue::from_rust(
                                        string.clone(),
                                        NativeString::from(text.to_owned()),
                                    )
                                    .unwrap(),
                                    DynamicValue::from_rust(number.clone(), count).unwrap(),
                                ],
                            )
                            .unwrap(),
                        )
                        .unwrap()
                    })
                    .collect(),
            )
            .unwrap(),
        )
        .unwrap(),
    )
}

#[test]
fn nested_records_enums_and_vectors_compare_without_moving_or_leasing_fields() {
    let left = nested("same", 42, 2);
    assert!(!left.is_copy());
    for (right, expected) in [
        (nested("same", 42, 2), true),
        (nested("other", 42, 2), false),
        (nested("same", 41, 2), false),
        (nested("same", 42, 1), false),
    ] {
        assert_eq!(left.try_equal(&right), Ok(expected));
    }
    assert!(!left.has_active_references());
    assert!(!left.is_partially_moved());
    let (borrowed, slot) = reference(left, false);
    let right = nested("same", 42, 2);
    assert_eq!(borrowed_equal(&borrowed, &right), Ok(true));
    drop(borrowed);
    assert!(slot.borrow_mut().take().is_ok());
}

#[test]
fn missing_leaf_equality_reports_an_error_without_cloning_or_dropping() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let leaf = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let option = DynamicLayout::option(leaf.clone()).unwrap();
    let some = || {
        value(
            DynamicValue::some(
                option.clone(),
                DynamicValue::from_rust(leaf.clone(), Probe(drops.clone())).unwrap(),
            )
            .unwrap(),
        )
    };
    let left = some();
    let right = some();
    assert!(
        left.try_equal(&right)
            .unwrap_err()
            .contains("no registered native equality")
    );
    assert_eq!(
        left.try_equal(&value(DynamicValue::none(option).unwrap())),
        Ok(false)
    );
    assert_eq!(drops.get(), 0);
    drop((left, right));
    assert_eq!(drops.get(), 2);
}

#[test]
fn vec_contains_reads_borrowed_string_and_record_needles() {
    let symbol = rils_builtins::builtin("Vec")
        .unwrap()
        .member("contains")
        .unwrap()
        .native_symbol
        .unwrap();
    let string = RecordLayoutResolver::new(&[])
        .resolve(&Type::String)
        .unwrap();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::String],
        },
        string.clone(),
    );
    let values = value(
        DynamicValue::sequence(
            vector,
            vec![DynamicValue::from_rust(string, NativeString::from("needle".to_owned())).unwrap()],
        )
        .unwrap(),
    );
    let (receiver, slot) = reference(values, true);
    for (text, expected) in [("needle", true), ("absent", false)] {
        let (needle, _) = reference(Value::from_string(text), false);
        assert_eq!(
            call_native_symbol(symbol, &[receiver.clone(), needle]).unwrap(),
            Ok(Value::Bool(expected))
        );
    }
    drop(receiver);
    assert!(slot.borrow_mut().take().is_ok());

    let record = nested("needle", 42, 1);
    let (needle, _) = reference(nested("needle", 42, 1), false);
    let Value::Dynamic(object) = record else {
        unreachable!()
    };
    let item_layout = object.descriptor().layout_handle();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![item_layout.rils_type().clone()],
        },
        item_layout,
    );
    let item = object.into_value().ok().expect("unique nested item");
    let (receiver, _) = reference(
        value(DynamicValue::sequence(vector, vec![item]).unwrap()),
        true,
    );
    assert_eq!(
        call_native_symbol(symbol, &[receiver, needle]).unwrap(),
        Ok(Value::Bool(true))
    );
}

#[test]
fn native_and_compatibility_aggregates_compare_through_original_elements() {
    let original = Value::Vec(Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(
            Type::String,
            Value::from_string("same"),
        )]),
        element_type: RefCell::new(Some(Type::String)),
        active_iterators: Cell::new(0),
    }));
    let ty = Type::Named {
        name: "Vec".into(),
        arguments: vec![Type::String],
    };
    let converted = native(original.clone_owned().unwrap(), &ty);
    assert_eq!(converted.try_equal(&original), Ok(true));
    assert_eq!(original.try_equal(&converted), Ok(true));
    let Value::Vec(elements) = original else {
        unreachable!()
    };
    elements.elements.borrow_mut()[0].value = None;
    assert!(
        converted
            .try_equal(&Value::Vec(elements))
            .unwrap_err()
            .contains("moved")
    );
}

#[test]
fn native_aggregate_reference_leaves_preserve_reference_identity() {
    let (reference, _) = reference(Value::from_i32(42), false);
    let separate = reference.clone();
    let ty = Type::of_value(&reference).unwrap();
    let option = some(reference.clone(), &ty);
    assert_eq!(option.try_equal(&some(separate, &ty)), Ok(true));
    // A second reference to an equal value has a different reference identity.
    let (other, _) = self::reference(Value::from_i32(42), false);
    assert_eq!(option.try_equal(&some(other, &ty)), Ok(false));
    assert_eq!(reference.try_equal(&reference), Ok(true));
}

#[test]
fn comparison_rejects_native_fields_that_have_been_moved() {
    let string = RecordLayoutResolver::new(&[])
        .resolve(&Type::String)
        .unwrap();
    let layout =
        DynamicLayout::record(Type::named("Record"), vec![("text".into(), string.clone())])
            .unwrap();
    let make = || {
        DynamicValue::record(
            layout.clone(),
            vec![
                DynamicValue::from_rust(string.clone(), NativeString::from("owned".to_owned()))
                    .unwrap(),
            ],
        )
        .unwrap()
    };
    let mut moved = make();
    let item = moved.take_path_field(&[DynamicPathStep::Field(0)]).unwrap();
    assert!(
        value(moved)
            .try_equal(&value(make()))
            .unwrap_err()
            .contains("moved")
    );
    drop(item);
}

#[test]
fn comparison_reports_conflicting_access_and_releases_read_guards() {
    let (borrowed, slot) = reference(Value::from_string("original"), true);
    let other = Value::from_string("original");
    {
        let _writing = slot.borrow_mut();
        assert!(
            borrowed_equal(&borrowed, &other)
                .unwrap_err()
                .contains("mutably accessed")
        );
    }
    assert_eq!(borrowed_equal(&borrowed, &other), Ok(true));
    let Value::Reference(reference) = &borrowed else {
        unreachable!()
    };
    reference.write(Value::from_string("changed")).unwrap();
    assert_eq!(borrowed_equal(&borrowed, &other), Ok(false));
}

#[test]
fn mixed_sum_comparison_keeps_reference_payloads_as_values() {
    let (item, _) = reference(Value::from_i32(42), false);
    let ty = Type::of_value(&item).unwrap();
    let converted = some(item.clone(), &ty);
    let original = Value::Option {
        value: Some(Rc::new(item)),
        element_type: Some(ty),
    };
    assert_eq!(converted.try_equal(&original), Ok(true));
    assert_eq!(original.try_equal(&converted), Ok(true));
}

#[test]
fn vec_contains_dereferences_the_outer_argument_but_preserves_reference_elements() {
    let symbol = rils_builtins::builtin("Vec")
        .unwrap()
        .member("contains")
        .unwrap()
        .native_symbol
        .unwrap();
    let (item, _) = reference(Value::from_i32(42), false);
    let ty = Type::of_value(&item).unwrap();
    let original = Value::Vec(Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(ty.clone(), item.clone())]),
        element_type: RefCell::new(Some(ty.clone())),
        active_iterators: Cell::new(0),
    }));
    let converted = native(
        original.clone_owned().unwrap(),
        &Type::Named {
            name: "Vec".into(),
            arguments: vec![ty],
        },
    );
    for collection in [original, converted] {
        let (receiver, _) = reference(collection, false);
        let (other, _) = reference(Value::from_i32(42), false);
        for (needle, expected) in [(item.clone(), true), (other, false)] {
            let (argument, _) = reference(needle, false);
            assert_eq!(
                call_native_symbol(symbol, &[receiver.clone(), argument]).unwrap(),
                Ok(Value::Bool(expected))
            );
        }
    }
}
