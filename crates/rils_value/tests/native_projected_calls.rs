use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, NativeType};
use std::rc::Rc;

#[test]
fn registered_calls_borrow_and_modify_the_same_leaf() {
    let descriptor = Rc::new(
        NativeType::<usize>::new::<String>(Type::String)
            .register_method("address", |call| {
                call.receiver::<String, _>(|text| text.as_ptr() as usize)
            })
            .register_method("append", |call| {
                call.receiver_mut::<String, _>(|text| {
                    text.push('!');
                    text.len()
                })
            })
            .register_method("reentrant", |call| {
                call.receiver::<String, _>(|_| {
                    assert!(call.receiver_mut::<String, _>(|_| ()).is_err());
                    0
                })
            })
            .register_method("copy", |call| {
                let text = call.receiver::<String, _>(Clone::clone)?;
                let copy = call.new_object(text)?;
                copy.with::<String, _>(String::len)
            }),
    );
    let text = String::from("native");
    let address = text.as_ptr() as usize;
    let mut payload = DynamicValue::from_rust(descriptor.layout_handle(), text).unwrap();
    assert_eq!(
        descriptor
            .call_view(payload.view(), "address", &[])
            .unwrap()
            .unwrap(),
        address
    );
    assert!(
        descriptor
            .call_view(payload.view(), "append", &[])
            .unwrap()
            .unwrap_err()
            .contains("mutable")
    );
    assert_eq!(
        descriptor
            .call_view_mut(payload.view_mut(), "append", &[])
            .unwrap()
            .unwrap(),
        7
    );
    assert_eq!(
        payload.view().with_rust::<String, _>(Clone::clone).unwrap(),
        "native!"
    );
    assert_eq!(
        descriptor
            .call_view_mut(payload.view_mut(), "reentrant", &[])
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(
        descriptor
            .call_view(payload.view(), "copy", &[])
            .unwrap()
            .unwrap(),
        7
    );
    assert!(
        descriptor
            .call_view(payload.view(), "missing", &[])
            .is_none()
    );
}

#[test]
fn projected_dispatch_rejects_a_mismatched_registration_before_calling() {
    let descriptor = Rc::new(
        NativeType::<usize>::new::<String>(Type::String).register_method("read", |_| {
            panic!("mismatched payload must not reach the method")
        }),
    );
    for layout in [
        DynamicLayout::of::<i32>(Type::String),
        DynamicLayout::of::<i32>(Type::I32),
    ] {
        let mut payload = DynamicValue::from_rust(layout, 42_i32).unwrap();
        assert!(
            descriptor
                .call_view(payload.view(), "read", &[])
                .unwrap()
                .is_err()
        );
        assert!(
            descriptor
                .call_view_mut(payload.view_mut(), "read", &[])
                .unwrap()
                .is_err()
        );
    }
}
