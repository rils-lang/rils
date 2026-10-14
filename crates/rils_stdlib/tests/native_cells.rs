use rils_stdlib::stdlib::cell::{
    ErasedRefCell,
    native_storage::{cell, ref_cell},
};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};
use std::{cell::Cell, rc::Rc};

fn integer(value: i32) -> DynamicValue {
    DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), value).unwrap()
}
fn number(value: DynamicValue) -> i32 {
    value
        .into_rust::<i32>()
        .unwrap_or_else(|(_, error)| panic!("{error}"))
}

#[test]
fn cells_preserve_values_when_layout_or_borrow_checks_fail() {
    let value = cell::new(integer(7)).unwrap();
    assert_eq!(number(cell::get(value.view()).unwrap()), 7);
    let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    assert!(cell::set(value.view(), wrong).is_err());
    assert_eq!(number(cell::get(value.view()).unwrap()), 7);
    assert_eq!(number(cell::replace(value.view(), integer(8)).unwrap()), 7);
    assert_eq!(number(cell::get(value.view()).unwrap()), 8);

    let value = ref_cell::new(integer(7)).unwrap();
    value
        .view()
        .with_rust::<ErasedRefCell, _>(|cell| cell.references.set(2))
        .unwrap();
    assert!(ref_cell::replace(value.view(), integer(8)).is_err());
    value
        .view()
        .with_rust::<ErasedRefCell, _>(|cell| cell.references.set(0))
        .unwrap();
    let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    assert!(ref_cell::replace(value.view(), wrong).is_err());
    assert_eq!(
        number(ref_cell::replace(value.view(), integer(9)).unwrap()),
        7
    );
    assert!(cell::get(value.view()).is_err());
}

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn replacing_non_clone_payloads_moves_and_drops_exactly_once() {
    for borrowed in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let item = || {
            DynamicValue::from_rust(
                DynamicLayout::of::<Probe>(Type::named("Probe")),
                Probe(drops.clone()),
            )
            .unwrap()
        };
        let value = if borrowed {
            ref_cell::new(item())
        } else {
            cell::new(item())
        }
        .unwrap();
        if !borrowed {
            assert!(cell::get(value.view()).is_err());
        }
        let old = if borrowed {
            ref_cell::replace(value.view(), item())
        } else {
            cell::replace(value.view(), item())
        }
        .unwrap();
        assert_eq!(drops.get(), 0);
        drop(old);
        assert_eq!(drops.get(), 1);
        drop(value);
        assert_eq!(drops.get(), 2);
    }
}
