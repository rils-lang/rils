//! Shared structural declarations consumed by both execution backends.
use std::{collections::HashMap, sync::OnceLock};

use crate::{
    ast::{GenericParameter, NamedField},
    source::Span,
};

pub struct BuiltinStructDeclaration {
    pub name: &'static str,
    pub opaque_native: bool,
    pub generic_parameters: Vec<GenericParameter>,
    pub fields: Vec<NamedField>,
    pub associated_types: HashMap<(String, String), crate::types::Type>,
}

pub fn builtin_runtime_struct_declarations() -> &'static [BuiltinStructDeclaration] {
    static DECLARATIONS: OnceLock<Vec<BuiltinStructDeclaration>> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        rils_builtins::BUILTINS
            .iter()
            // Host declarations are supplied by the host registry. Recreating them
            // here would give host results and bytecode separate nominal identities.
            .filter(|declaration| {
                declaration.kind == rils_builtins::BuiltinKind::Struct
                    && declaration.backend == rils_builtins::BuiltinBackend::Runtime
            })
            .map(|declaration| BuiltinStructDeclaration {
                associated_types: associated_types(declaration),
                name: declaration.path,
                opaque_native: declaration.opaque_native,
                generic_parameters: declaration
                    .type_parameters
                    .iter()
                    .map(|name| GenericParameter {
                        is_const: false,
                        name: (*name).into(),
                        bounds: Vec::new(),
                        span: Span::default(),
                    })
                    .collect(),
                fields: declaration
                    .members
                    .iter()
                    .filter(|member| member.kind == rils_builtins::BuiltinMemberKind::Field)
                    .map(|member| NamedField {
                        name: member.name.into(),
                        type_annotation: super::resolve_type_pattern(
                            member.value_type.expect("declared field type"),
                        ),
                        span: Span::default(),
                    })
                    .collect(),
            })
            .collect()
    })
}

fn associated_types(
    declaration: &rils_builtins::BuiltinDeclaration,
) -> HashMap<(String, String), crate::types::Type> {
    let mut result = HashMap::new();
    if let Some(source) = declaration.source {
        let tokens = rils_syntax::lex(source).expect("generated declaration must lex");
        let program = rils_syntax::parser::parse_builtin_declarations(tokens)
            .expect("generated declaration must parse");
        for statement in program.statements {
            if let crate::ast::Stmt::Impl {
                trait_name: Some(trait_name),
                target: crate::types::Type::Named { name, .. },
                associated_types,
                ..
            } = statement
                && name == declaration.path
            {
                for associated in associated_types {
                    if let Some(value) = associated.value {
                        result.insert((trait_name.clone(), associated.name), value);
                    }
                }
            }
        }
    }
    result
}
