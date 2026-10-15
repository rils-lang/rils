use rils_stdlib::{Type, stdlib::collections::vector::Vec as RilsVec};
use rils_value::{DynamicLayout, DynamicValue, OwnedElements};
use std::{cell::Cell, rc::Rc};

#[repr(align(64))]
struct Payload {
    index: usize,
    drops: Rc<Cell<usize>>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn owning_method_retains_lazy_cursor_layout_and_exactly_once_destruction() {
    for array in [false, true] {
        for length in [0, 1, 4] {
            let drops = Rc::new(Cell::new(0));
            let layout = DynamicLayout::of::<Payload>(Type::named("Payload"));
            let values = (0..length)
                .map(|index| {
                    DynamicValue::from_rust(
                        layout.clone(),
                        Payload {
                            index,
                            drops: drops.clone(),
                        },
                    )
                    .unwrap()
                })
                .collect();
            let cursor = if array {
                OwnedElements::from_array(
                    DynamicValue::record(
                        DynamicLayout::array(layout.clone(), length).unwrap(),
                        values,
                    )
                    .unwrap(),
                    layout.clone(),
                )
                .unwrap()
            } else {
                OwnedElements::from_values(layout.clone(), values).unwrap()
            };
            let mut cursor = RilsVec::<Payload, _>::with_storage(cursor).into_iter();
            assert!(Rc::ptr_eq(cursor.item_layout(), &layout));
            assert_eq!(cursor.len(), length);
            assert_eq!(drops.get(), 0);
            let first = cursor.next().transpose().unwrap();
            if let Some(first) = &first {
                assert_eq!(
                    first
                        .view()
                        .with_rust::<Payload, _>(|value| value.index)
                        .unwrap(),
                    0
                );
            }
            drop(cursor);
            assert_eq!(drops.get(), length.saturating_sub(1));
            drop(first);
            assert_eq!(drops.get(), length);
        }
    }
}
