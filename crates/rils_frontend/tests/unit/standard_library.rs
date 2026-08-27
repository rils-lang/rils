use super::*;

#[test]
fn integer_intrinsic_types_replace_nested_self_patterns() {
    let intrinsic = rils_builtins::integer_method("checked_add").unwrap();
    assert_eq!(
        integer_intrinsic_type(intrinsic, crate::types::IntegerType::I32),
        Type::function(vec![Type::I32], Type::Option(Box::new(Type::I32)))
    );
}

#[test]
fn float_intrinsic_types_preserve_concrete_float_type() {
    let intrinsic = rils_builtins::float_method("clamp").unwrap();
    let float = Type::Float(crate::types::FloatType::F32);
    assert_eq!(
        float_intrinsic_type(intrinsic, crate::types::FloatType::F32),
        Type::function(vec![float.clone(), float.clone()], float)
    );
}

#[test]
fn runtime_signatures_are_resolved_by_stable_id() {
    let option = erased_runtime_signature(rils_builtins::BuiltinId::OptionReplace).unwrap();
    let string = erased_runtime_signature(rils_builtins::BuiltinId::StringReplace).unwrap();

    assert_ne!(option, string);
    assert_eq!(option.return_type, Type::Unknown);
    assert_eq!(string.return_type, Type::String);
}

#[test]
fn derived_debug_runtime_call_has_one_reference_layer_per_argument() {
    assert_eq!(
        erased_runtime_signature(rils_builtins::BuiltinId::FormatterWriteDerivedDebug),
        Some(FunctionSignature::fixed(
            vec![
                Type::Reference {
                    mutable: true,
                    inner: Box::new(Type::Unknown),
                },
                Type::Reference {
                    mutable: false,
                    inner: Box::new(Type::Unknown),
                },
            ],
            Type::Result(Box::new(Type::Unit), Box::new(Type::named("FormatError")),),
        ))
    );
}

#[test]
fn builtin_method_generics_preserve_callback_result_types() {
    assert_eq!(
        builtin_member_type(&Type::Option(Box::new(Type::I32)), "map"),
        Some(Type::function(
            vec![Type::function(vec![Type::I32], Type::Variable("U".into()))],
            Type::Option(Box::new(Type::Variable("U".into()))),
        ))
    );
    assert_eq!(
        builtin_member_type(
            &Type::Result(Box::new(Type::I32), Box::new(Type::String)),
            "map_err",
        ),
        Some(Type::function(
            vec![Type::function(
                vec![Type::String],
                Type::Variable("F".into()),
            )],
            Type::Result(Box::new(Type::I32), Box::new(Type::Variable("F".into()))),
        ))
    );
}

#[test]
fn builtin_members_replace_generics_nested_in_return_types() {
    let tasks = Type::Named {
        name: "Vec".into(),
        arguments: vec![Type::named("Task")],
    };
    assert_eq!(
        builtin_member_type(&tasks, "into_iter"),
        Some(Type::function(
            Vec::new(),
            Type::Named {
                name: "SequenceIterator".into(),
                arguments: vec![Type::named("Task")],
            }
        ))
    );
}
