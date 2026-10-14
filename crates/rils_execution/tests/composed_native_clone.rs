use rils_execution::{
    Type, Value,
    value::{
        NativeObject, NativeType, native_instance, native_ops, record_codec::NativeRecordCodec,
    },
};
use rils_value::{DynamicLayout, DynamicValue};
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

fn descriptor(calls: Rc<Cell<usize>>, failure: bool) -> Rc<NativeType> {
    Rc::new(
        NativeType::new::<Probe>(Type::named("Probe")).register_method(
            native_ops::CLONE,
            move |context| {
                calls.set(calls.get() + 1);
                let cloned = context.receiver::<Probe, _>(|probe| {
                    if failure && probe.number == 2 {
                        return Err("clone failed".to_owned());
                    }
                    Ok(Probe {
                        number: probe.number + 10,
                        drops: probe.drops.clone(),
                    })
                })??;
                Ok(Value::Native(context.new_object(cloned)?))
            },
        ),
    )
}

fn nested(
    descriptor: Rc<NativeType>,
    drops: Rc<Cell<usize>>,
    branch: usize,
    length: usize,
) -> Value {
    let leaf = descriptor.layout_handle();
    let sequence = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Probe")],
        },
        leaf,
    );
    let unit = DynamicLayout::copy_of::<()>(Type::Unit);
    let result = DynamicLayout::variant(
        Type::Result(Box::new(sequence.rils_type().clone()), Box::new(Type::Unit)),
        vec![sequence.clone(), unit.clone()],
    )
    .unwrap();
    let option = DynamicLayout::option(result.clone()).unwrap();
    let payload = if branch == 0 {
        DynamicValue::none(option.clone()).unwrap()
    } else {
        let (index, item) = if branch == 1 {
            let items = (1..=length)
                .map(|number| {
                    NativeObject::new(
                        descriptor.clone(),
                        Probe {
                            number: number as i32,
                            drops: drops.clone(),
                        },
                    )
                    .unwrap()
                    .into_dynamic()
                    .ok()
                    .unwrap()
                })
                .collect();
            (0, DynamicValue::sequence(sequence, items).unwrap())
        } else {
            (1, DynamicValue::from_rust(unit, ()).unwrap())
        };
        DynamicValue::some(
            option.clone(),
            DynamicValue::variant(result, index, item).unwrap(),
        )
        .unwrap()
    };
    let tuple = DynamicLayout::aggregate(
        Type::Tuple(vec![option.rils_type().clone()]),
        vec![("0".into(), option)],
    )
    .unwrap();
    let mut codec = NativeRecordCodec::new();
    codec.register_native_type(descriptor).unwrap();
    native_instance::from_native(
        DynamicValue::record(tuple, vec![payload]).unwrap(),
        Rc::new(codec),
    )
    .unwrap()
}

fn numbers(value: &Value) -> Vec<i32> {
    let mut result = Vec::new();
    value
        .with_native_view(|view| {
            view.any_leaf(&mut |leaf| {
                if leaf.layout()?.is_rust_type::<Probe>() {
                    result.push(leaf.with_rust::<Probe, _>(|probe| probe.number)?);
                }
                Ok(false)
            })
        })
        .unwrap()
        .unwrap();
    result
}

#[test]
fn nested_registered_clones_keep_context_and_only_clone_the_active_payload() {
    for (branch, length) in [(0, 0), (1, 0), (1, 2), (2, 0)] {
        let calls = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let source = nested(
            descriptor(calls.clone(), false),
            drops.clone(),
            branch,
            length,
        );
        let cloned = source.clone_owned().unwrap();
        let again = cloned.clone_owned().unwrap();
        assert_eq!(numbers(&source), (1..=length as i32).collect::<Vec<_>>());
        assert_eq!(
            numbers(&cloned),
            (1..=length as i32).map(|n| n + 10).collect::<Vec<_>>()
        );
        assert_eq!(
            numbers(&again),
            (1..=length as i32).map(|n| n + 20).collect::<Vec<_>>()
        );
        assert_eq!(calls.get(), 2 * length);
        assert_eq!(Type::of_value(&source), Type::of_value(&again));
        drop((source, cloned, again));
        assert_eq!(drops.get(), 3 * length);
    }
}

#[test]
fn failed_nested_clone_releases_completed_outputs_and_keeps_the_source() {
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let source = nested(descriptor(calls.clone(), true), drops.clone(), 1, 2);
    assert_eq!(source.clone_owned().unwrap_err(), "clone failed");
    assert_eq!(calls.get(), 2);
    assert_eq!(drops.get(), 1);
    assert_eq!(numbers(&source), vec![1, 2]);
    drop(source);
    assert_eq!(drops.get(), 3);
}

#[test]
fn nested_clone_rejects_wrong_return_types_and_registration_identity() {
    for wrong_registration in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(
            NativeType::new::<Probe>(Type::named("Probe")).register_method(
                native_ops::CLONE,
                move |context| {
                    if wrong_registration {
                        let drops = context.receiver::<Probe, _>(|probe| probe.drops.clone())?;
                        Ok(Value::Native(NativeObject::new(
                            Rc::new(NativeType::new::<Probe>(Type::named("Probe"))),
                            Probe { number: 9, drops },
                        )?))
                    } else {
                        Ok(Value::from(rils_execution::value::native_bool(true)))
                    }
                },
            ),
        );
        let source = nested(descriptor, drops.clone(), 1, 1);
        assert!(
            source
                .clone_owned()
                .unwrap_err()
                .contains("different registered type")
        );
        assert_eq!(numbers(&source), vec![1]);
        drop(source);
        assert_eq!(drops.get(), if wrong_registration { 2 } else { 1 });
    }
}
