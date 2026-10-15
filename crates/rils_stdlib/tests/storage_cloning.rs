use rils_stdlib::{Type, stdlib::collections::VecDeque, storage::CloningView};
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

fn clone_string(value: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
    let text = value.with_rust::<String, _>(Clone::clone)?;
    DynamicValue::from_rust(DynamicLayout::of::<String>(Type::String), text)
}

#[test]
fn endpoint_clones_preserve_sources_and_propagate_failure() {
    for array in [false, true] {
        for length in [0, 1, 4] {
            let item = DynamicLayout::of::<String>(Type::String);
            let values = (0..length)
                .map(|i| DynamicValue::from_rust(item.clone(), i.to_string()).unwrap())
                .collect();
            let value = if array {
                DynamicValue::record(DynamicLayout::array(item, length).unwrap(), values).unwrap()
            } else {
                DynamicValue::sequence(DynamicLayout::sequence(Type::named("Items"), item), values)
                    .unwrap()
            };
            let view = value.view();
            let receiver = VecDeque::<DynamicValue, _>::with_storage(CloningView::new(
                view.indexed().unwrap(),
                clone_string,
            ));
            for back in [false, true] {
                let result = if back {
                    receiver.back_cloned()
                } else {
                    receiver.front_cloned()
                };
                let expected = (length > 0).then(|| if back { length - 1 } else { 0 }.to_string());
                assert_eq!(
                    result
                        .unwrap()
                        .map(|v| v.view().with_rust::<String, _>(Clone::clone).unwrap()),
                    expected
                );
            }
            for i in 0..length {
                assert_eq!(
                    view.indexed()
                        .unwrap()
                        .item(i)
                        .unwrap()
                        .with_rust::<String, _>(Clone::clone)
                        .unwrap(),
                    i.to_string()
                );
            }
            let failing = VecDeque::<DynamicValue, _>::with_storage(CloningView::new(
                view.indexed().unwrap(),
                |_| Err("clone failed".into()),
            ));
            for result in [failing.front_cloned(), failing.back_cloned()] {
                if length == 0 {
                    assert!(result.unwrap().is_none());
                } else {
                    assert_eq!(result.err().unwrap(), "clone failed");
                }
            }
        }
    }
}
