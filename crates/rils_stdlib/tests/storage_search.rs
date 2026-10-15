use rils_stdlib::{Type, stdlib::collections::vector::Vec as RilsVec, storage::ElementQuery};
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};
use std::cell::Cell;

struct Query {
    needle: String,
    visits: Cell<usize>,
    fail_at: Option<usize>,
}
impl ElementQuery for Query {
    fn equal(&self, element: DynamicValueRef<'_>) -> Result<bool, String> {
        let visit = self.visits.get();
        self.visits.set(visit + 1);
        if self.fail_at == Some(visit) {
            return Err("comparison failed".into());
        }
        element.with_rust::<String, _>(|value| value == &self.needle)
    }
}

#[test]
fn projected_search_preserves_short_circuit_errors_and_non_copy_elements() {
    for array in [false, true] {
        for length in [0, 1, 4] {
            let item = DynamicLayout::of::<String>(Type::String);
            let items = (0..length)
                .map(|i| DynamicValue::from_rust(item.clone(), i.to_string()).unwrap())
                .collect();
            let value = if array {
                DynamicValue::record(DynamicLayout::array(item, length).unwrap(), items).unwrap()
            } else {
                DynamicValue::sequence(DynamicLayout::sequence(Type::named("Items"), item), items)
                    .unwrap()
            };
            let view = value.view();
            let receiver = RilsVec::<Query, _>::with_storage(view.indexed().unwrap());
            for needle in ["0", "3", "absent"] {
                for fail_at in [None, Some(0), Some(1)] {
                    let query = Query {
                        needle: needle.into(),
                        visits: Cell::new(0),
                        fail_at,
                    };
                    let mut expected = Ok(false);
                    let mut visits = 0;
                    for i in 0..length {
                        visits += 1;
                        if fail_at == Some(i) {
                            expected = Err("comparison failed".into());
                            break;
                        }
                        if i.to_string() == needle {
                            expected = Ok(true);
                            break;
                        }
                    }
                    assert_eq!(receiver.contains(&query), expected);
                    assert_eq!(query.visits.get(), visits);
                }
            }
            for i in 0..length {
                assert_eq!(
                    value
                        .view()
                        .indexed()
                        .unwrap()
                        .item(i)
                        .unwrap()
                        .with_rust::<String, _>(String::clone)
                        .unwrap(),
                    i.to_string()
                );
            }
        }
    }
}
