use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

struct Probe {
    drops: Rc<Cell<usize>>,
    text: String,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn native_sequence_storage_is_borrowed_in_place_through_nested_paths() {
    for length in [0, 1, 3] {
        let drops = Rc::new(Cell::new(0));
        let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let layout = DynamicLayout::sequence(Type::named("Items"), item.clone());
        let fields = (0..length)
            .map(|index| {
                DynamicValue::from_rust(
                    item.clone(),
                    Probe {
                        drops: drops.clone(),
                        text: index.to_string(),
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let pointer = fields.as_ptr();
        let sequence = DynamicValue::sequence(layout.clone(), fields).unwrap();
        let record =
            DynamicLayout::record(Type::named("Holder"), vec![("items".into(), layout)]).unwrap();
        let record = DynamicValue::record(record, vec![sequence]).unwrap();
        record
            .view()
            .field(0)
            .unwrap()
            .with_sequence_storage(|items| {
                assert_eq!(items.len(), length);
                assert_eq!(items.as_ptr(), pointer);
                for (index, item) in items.iter().enumerate() {
                    item.with::<Probe, _>(|value| assert_eq!(value.text, index.to_string()))
                        .unwrap();
                }
            })
            .unwrap();
        assert!(record.view().with_sequence_storage(|_| ()).is_err());
        assert_eq!(drops.get(), 0);
        drop(record);
        assert_eq!(drops.get(), length);
    }
}

#[test]
fn native_view_projects_noncopy_generic_compositions_without_moving_them() {
    let drops = Rc::new(Cell::new(0));
    let probe_type = Type::named("Probe");
    let probe = DynamicLayout::of::<Probe>(probe_type.clone());
    let option = DynamicLayout::option(probe.clone()).unwrap();
    let result_type = Type::Result(
        Box::new(Type::Unit),
        Box::new(Type::Option(Box::new(probe_type.clone()))),
    );
    let result = DynamicLayout::variant(
        result_type.clone(),
        vec![DynamicLayout::copy_of::<()>(Type::Unit), option.clone()],
    )
    .unwrap();
    let sequence = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![result_type],
        },
        result.clone(),
    );
    let record = DynamicLayout::record(
        Type::named("Holder"),
        vec![("items".into(), sequence.clone())],
    )
    .unwrap();
    let payload = DynamicValue::from_rust(
        probe,
        Probe {
            drops: drops.clone(),
            text: "native".into(),
        },
    )
    .unwrap();
    let option = DynamicValue::some(option, payload).unwrap();
    let result = DynamicValue::variant(result, 1, option).unwrap();
    let sequence = DynamicValue::sequence(sequence, vec![result]).unwrap();
    let holder = DynamicValue::record(record, vec![sequence]).unwrap();

    let items = holder.view().project(DynamicPathStep::Field(0)).unwrap();
    assert_eq!(items.sequence_len(), Ok(1));
    let result = items.project(DynamicPathStep::Index(0)).unwrap();
    assert_eq!(result.variant_index(), Ok(1));
    let option = result.project(DynamicPathStep::Variant(1)).unwrap();
    assert_eq!(option.option_is_some(), Ok(true));
    let item = option.project(DynamicPathStep::Some).unwrap();
    assert_eq!(item.layout().unwrap().rils_type(), &probe_type);
    assert_eq!(
        item.with_rust::<Probe, _>(|probe| probe.text.clone()),
        Ok("native".into())
    );
    assert!(item.copy_owned().is_err());
    assert_eq!(drops.get(), 0);
    drop(holder);
    assert_eq!(drops.get(), 1);
}

#[test]
fn native_view_rejects_absent_and_moved_children() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let option = DynamicLayout::option(number.clone()).unwrap();
    let none = DynamicValue::none(option).unwrap();
    let view = none.view();
    assert_eq!(view.option_is_some(), Ok(false));
    assert!(view.project(DynamicPathStep::Some).is_err());

    let record = DynamicLayout::record(
        Type::named("Holder"),
        vec![("value".into(), number.clone())],
    )
    .unwrap();
    let mut holder =
        DynamicValue::record(record, vec![DynamicValue::from_rust(number, 3).unwrap()]).unwrap();
    holder.take_field(0).unwrap();
    assert!(holder.view().project(DynamicPathStep::Field(0)).is_err());
}
