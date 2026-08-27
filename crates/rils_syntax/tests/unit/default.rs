use super::*;

#[test]
fn plans_recursive_defaults_and_rejects_unsupported_owned_types() {
    assert!(matches!(
        default_plan(&Type::Tuple(vec![Type::Bool, Type::I32])),
        Some(DefaultPlan::Tuple(elements)) if elements.len() == 2
    ));
    assert!(matches!(
        default_plan(&Type::Option(Box::new(Type::function(
            Vec::new(),
            Type::Unit
        )))),
        Some(DefaultPlan::Option(_))
    ));
    assert!(default_plan(&Type::function(Vec::new(), Type::Unit)).is_none());
    assert!(default_plan(&Type::Result(Box::new(Type::I32), Box::new(Type::String))).is_none());
}
