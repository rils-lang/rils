use super::*;

#[test]
fn callback_imports_require_declared_shapes_and_complete_result_witnesses() {
    for (owner, method, result, valid) in [
        ("Option", "map", Type::Option(Box::new(Type::String)), true),
        ("Option", "map", Type::Bool, false),
        (
            "Option",
            "map",
            Type::Option(Box::new(Type::Unknown)),
            false,
        ),
        (
            "Option",
            "or_else",
            Type::Option(Box::new(Type::String)),
            true,
        ),
        ("Option", "or_else", Type::Bool, false),
        (
            "Result",
            "map_err",
            Type::Result(Box::new(Type::I32), Box::new(Type::String)),
            true,
        ),
        (
            "Result",
            "map_err",
            Type::Result(Box::new(Type::I32), Box::new(Type::Unknown)),
            false,
        ),
        (
            "Result",
            "and_then",
            Type::Option(Box::new(Type::I32)),
            false,
        ),
        (
            "Option",
            "map",
            Type::Option(Box::new(Type::Variable("T".into()))),
            true,
        ),
    ] {
        let member = rils_builtins::builtin_member(owner, method).unwrap();
        let erased =
            rils_frontend::standard_library::erased_builtin_member_signature(member).unwrap();
        let import = BytecodeNativeImport {
            symbol: member.native_symbol.unwrap().into(),
            signature: FunctionSignature {
                parameters: erased.parameters.clone(),
                return_type: result,
            },
        };
        assert_eq!(
            import.valid_callback_specialization(&erased),
            valid,
            "{owner}::{method}: {:?}",
            import.signature
        );
        let mut invalid = import;
        invalid.signature.parameters = Some(vec![]);
        assert!(!invalid.valid_callback_specialization(&erased));
    }
}
