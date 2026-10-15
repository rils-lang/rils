use rils_compiler::{compile, hir::HirTypeDefinition};

#[test]
fn compiler_preserves_runtime_catalog_structs_without_redeclaring_host_types() {
    let program = compile("").unwrap();
    for declaration in rils_builtins::BUILTINS
        .iter()
        .filter(|item| matches!(item.backend, rils_builtins::BuiltinBackend::Host(_)))
    {
        assert!(!program.types.iter().any(|item| matches!(item, HirTypeDefinition::Struct { name, .. } if name == declaration.path)));
    }
    for declaration in rils_builtins::BUILTINS.iter().filter(|item| {
        item.kind == rils_builtins::BuiltinKind::Struct
            && item.backend == rils_builtins::BuiltinBackend::Runtime
    }) {
        let definitions = program
            .types
            .iter()
            .filter_map(|item| match item {
                HirTypeDefinition::Struct {
                    name,
                    opaque_native,
                    generic_parameters,
                    fields,
                } if name == declaration.path => Some((opaque_native, generic_parameters, fields)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 1, "{}", declaration.path);
        let (opaque, parameters, fields) = definitions[0];
        assert_eq!(*opaque, declaration.opaque_native);
        assert_eq!(
            parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            declaration.type_parameters
        );
        assert_eq!(
            fields.len(),
            declaration
                .members
                .iter()
                .filter(|m| m.kind == rils_builtins::BuiltinMemberKind::Field)
                .count()
        );
    }
}
