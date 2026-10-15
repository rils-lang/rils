use rils_stdlib::storage::{EntrySchema, EntryShape};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};
use std::rc::Rc;

const NESTED: EntrySchema = EntrySchema {
    owner: "Flipped",
    parameters: 2,
    entry: EntryShape::Tuple(&[
        EntryShape::Tuple(&[EntryShape::Parameter(1)]),
        EntryShape::Parameter(0),
    ]),
};

fn tuple(fields: Vec<Rc<DynamicLayout>>) -> Rc<DynamicLayout> {
    DynamicLayout::aggregate(
        Type::Tuple(
            fields
                .iter()
                .map(|field| field.rils_type().clone())
                .collect(),
        ),
        fields
            .into_iter()
            .enumerate()
            .map(|(index, field)| (index.to_string(), field))
            .collect(),
    )
    .unwrap()
}

#[test]
fn nested_entry_schema_projects_builds_and_moves_by_declared_parameter() {
    // Identical language and physical types must still follow the declaration's indices.
    let integer = DynamicLayout::copy_of::<i32>(Type::I32);
    let entry = tuple(vec![tuple(vec![integer.clone()]), integer.clone()]);
    let layout = DynamicLayout::sequence(
        Type::Named {
            name: "Flipped".into(),
            arguments: vec![Type::I32, Type::I32],
        },
        entry.clone(),
    );
    NESTED.validate(&layout).unwrap();
    assert_eq!(
        NESTED.parameter_path(0).unwrap(),
        vec![DynamicPathStep::Field(1)]
    );
    assert_eq!(
        NESTED.parameter_path(1).unwrap(),
        vec![DynamicPathStep::Field(0), DynamicPathStep::Field(0)]
    );
    for index in [0, 1] {
        assert!(Rc::ptr_eq(
            &NESTED.parameter_layout(&layout, index).unwrap(),
            &integer
        ));
    }
    let values = [11, 29].map(|value| DynamicValue::from_rust(integer.clone(), value).unwrap());
    let entry = NESTED.build_entry(entry, Vec::from(values)).unwrap();
    assert_eq!(
        NESTED
            .project(entry.view(), 0)
            .unwrap()
            .with_rust(|value: &i32| *value)
            .unwrap(),
        11
    );
    assert_eq!(
        NESTED
            .project(entry.view(), 1)
            .unwrap()
            .with_rust(|value: &i32| *value)
            .unwrap(),
        29
    );
    assert_eq!(
        NESTED
            .take(entry, 1)
            .unwrap()
            .with(|value: &i32| *value)
            .unwrap(),
        29
    );
}

#[test]
fn entry_schema_rejects_missing_repeated_or_wrong_parameter_layouts() {
    let integer = DynamicLayout::copy_of::<i32>(Type::I32);
    let layout = DynamicLayout::sequence(
        Type::Named {
            name: "Flipped".into(),
            arguments: vec![Type::I32, Type::Bool],
        },
        tuple(vec![integer.clone(), integer]),
    );
    for entry in [
        EntryShape::Tuple(&[EntryShape::Parameter(0), EntryShape::Parameter(0)]),
        EntryShape::Tuple(&[EntryShape::Parameter(0), EntryShape::Parameter(2)]),
        EntryShape::Tuple(&[EntryShape::Parameter(0), EntryShape::Parameter(1)]),
    ] {
        assert!(EntrySchema { entry, ..NESTED }.validate(&layout).is_err());
    }
    assert!(NESTED.parameter_path(2).is_err());
}
