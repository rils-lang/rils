use std::rc::Rc;

use rils_native::{ElementRegistration, LayoutRegistration, NativeRegistry};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

fn matches_vec(ty: &Type) -> bool {
    matches!(ty, Type::Named { name, arguments } if name == "Vec" && arguments.len() == 1)
}

fn vec_layout(
    ty: &Type,
    resolve: &mut dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String>,
) -> Option<Result<Rc<DynamicLayout>, String>> {
    let Type::Named { arguments, .. } = ty else {
        return None;
    };
    Some(resolve(&arguments[0]).map(|item| DynamicLayout::sequence(ty.clone(), item)))
}

fn matches_string(ty: &Type) -> bool {
    ty == &Type::String
}

fn clone_string(item: rils_value::DynamicValueRef<'_>) -> Result<DynamicValue, String> {
    let text = item.with_rust::<String, _>(Clone::clone)?;
    DynamicValue::from_rust(item.layout()?, text)
}

static REGISTRY: NativeRegistry = NativeRegistry::new(
    &[LayoutRegistration {
        matches: matches_vec,
        layout: vec_layout,
    }],
    &[ElementRegistration {
        matches: matches_string,
        clone_borrowed: clone_string,
    }],
);

#[test]
fn recursive_layout_and_element_policy_are_registered_independently() {
    let ty = Type::Named {
        name: "Vec".into(),
        arguments: vec![Type::String],
    };
    let layout = REGISTRY
        .layout(&ty, &mut |child| {
            assert_eq!(child, &Type::String);
            Ok(DynamicLayout::of::<String>(Type::String))
        })
        .unwrap()
        .unwrap();
    assert_eq!(layout.rils_type(), &ty);

    let string_layout = DynamicLayout::of::<String>(Type::String);
    let item = DynamicValue::from_rust(string_layout, "hello".to_owned()).unwrap();
    let cloned = REGISTRY.clone_borrowed_element(&item).unwrap();
    assert!(cloned.with::<String, _>(|text| text == "hello").unwrap());
    assert!(
        REGISTRY
            .layout(&Type::Bool, &mut |_| unreachable!())
            .is_none()
    );
}
