use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{FieldSlot, IndexedStorage, OwnedIteratorValue},
};

#[test]
fn owned_indexed_iterator_moves_items_only_when_advanced() {
    let source = Rc::new(IndexedStorage {
        elements: RefCell::new(
            [2, 3, 5]
                .into_iter()
                .map(|number| FieldSlot::new(Type::I32, Value::from_i32(number)))
                .collect(),
        ),
        element_type: RefCell::new(Some(Type::I32)),
        active_iterators: Default::default(),
    });
    let iterator = OwnedIteratorValue::from_indexed(source.clone(), Type::I32);

    assert!(
        source
            .elements
            .borrow()
            .iter()
            .all(|slot| slot.value.is_some())
    );
    assert_eq!(iterator.next().unwrap(), Some(Value::from_i32(2)));
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
    assert_eq!(iterator.next().unwrap(), Some(Value::from_i32(3)));
    assert_eq!(iterator.next().unwrap(), Some(Value::from_i32(5)));
    assert_eq!(iterator.next().unwrap(), None);
}
