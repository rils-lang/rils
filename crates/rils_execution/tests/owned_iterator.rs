use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{FieldSlot, OwnedIteratorValue, SequenceValue},
};

#[test]
fn owned_sequence_iterator_moves_items_only_when_advanced() {
    let source = Rc::new(SequenceValue {
        elements: RefCell::new(
            [2, 3, 5]
                .into_iter()
                .map(|number| FieldSlot {
                    value: Some(Value::I32(number)),
                    type_annotation: Type::I32,
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(Some(Type::I32)),
        active_iterators: Default::default(),
    });
    let iterator = OwnedIteratorValue::from_sequence(source.clone(), Type::I32);

    assert!(
        source
            .elements
            .borrow()
            .iter()
            .all(|slot| slot.value.is_some())
    );
    assert_eq!(iterator.next().unwrap(), Some(Value::I32(2)));
    assert!(source.elements.borrow()[0].value.is_none());
    assert!(source.elements.borrow()[1].value.is_some());

    iterator.materialize().unwrap();
    assert!(
        source
            .elements
            .borrow()
            .iter()
            .all(|slot| slot.value.is_none())
    );
    assert_eq!(iterator.next().unwrap(), Some(Value::I32(3)));
    assert_eq!(iterator.next().unwrap(), Some(Value::I32(5)));
    assert_eq!(iterator.next().unwrap(), None);
}
