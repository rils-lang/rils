use std::collections::HashSet;

use super::CopyTypes;
use crate::{
    Type,
    analysis::AnalysisDiagnostic,
    ast::{EnumVariant, Program, Stmt},
    semantic::DeclarationTypeResolver,
};

pub(crate) fn collect(
    programs: &[(&[String], &Program)],
    resolver: &DeclarationTypeResolver,
) -> CopyTypes {
    let mut result = CopyTypes::default();
    for (module, program) in programs {
        visit(&program.statements, module, &mut |statement, module| {
            let (name, parameters, fields) = match statement {
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => (
                    name,
                    generic_parameters,
                    fields
                        .iter()
                        .map(|field| field.type_annotation.clone())
                        .collect::<Vec<_>>(),
                ),
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => (
                    name,
                    generic_parameters,
                    variants
                        .iter()
                        .flat_map(|variant| match variant {
                            EnumVariant::Unit { .. } => vec![],
                            EnumVariant::Tuple { fields, .. } => fields.clone(),
                            EnumVariant::Record { fields, .. } => fields
                                .iter()
                                .map(|field| field.type_annotation.clone())
                                .collect(),
                        })
                        .collect(),
                ),
                _ => return,
            };
            let qualified = module
                .iter()
                .map(String::as_str)
                .chain([name.as_str()])
                .collect::<Vec<_>>()
                .join("::");
            result.define(
                qualified,
                parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect(),
                fields
                    .iter()
                    .map(|field| resolver.resolve(field, module))
                    .collect(),
            );
        });
    }
    for (module, program) in programs {
        visit(&program.statements, module, &mut |statement, module| {
            if let Stmt::Impl {
                trait_name: Some(trait_name),
                target,
                ..
            } = statement
                && is_builtin_trait(trait_name, "Clone", module, resolver)
                && let Type::Named { name, .. } = resolver.resolve(target, module)
            {
                result.implement_clone(&name);
            }
            if let Stmt::Impl {
                trait_name: Some(trait_name),
                target,
                ..
            } = statement
                && is_copy_trait(trait_name, module, resolver)
                && result.is_unconditional_target(&resolver.resolve(target, module))
                && let Type::Named { name, .. } = resolver.resolve(target, module)
            {
                result.implement(&name);
            }
        });
    }
    result
}

pub(crate) fn validate(
    program: &Program,
    module: &[String],
    resolver: &DeclarationTypeResolver,
    hosts: &HashSet<String>,
) -> Vec<AnalysisDiagnostic> {
    let mut diagnostics = vec![];
    visit(&program.statements, module, &mut |statement, module| {
        if let Stmt::Impl {
            trait_name: Some(trait_name),
            target,
            span,
            ..
        } = statement
            && is_copy_trait(trait_name, module, resolver)
        {
            let target = resolver.resolve(target, module);
            if !resolver.copy_types().is_unconditional_target(&target) {
                diagnostics.push(AnalysisDiagnostic::error(
                    "conditional Copy implementations are not supported yet",
                    *span,
                ));
                return;
            }
            if !resolver.copy_types().fields_are_copy(&target, hosts) {
                diagnostics.push(AnalysisDiagnostic::error(
                    format!("`{target}` cannot implement Copy because it contains non-Copy fields"),
                    *span,
                ));
            }
            if let Type::Named { name, .. } = &target
                && !resolver.copy_types().has_clone(name)
            {
                diagnostics.push(AnalysisDiagnostic::error(format!("type `{target}` must implement supertrait `Clone` before implementing `Copy`"), *span));
            }
        }
    });
    diagnostics
}

fn is_copy_trait(name: &str, module: &[String], resolver: &DeclarationTypeResolver) -> bool {
    is_builtin_trait(name, "Copy", module, resolver)
}

fn is_builtin_trait(
    name: &str,
    expected: &str,
    module: &[String],
    resolver: &DeclarationTypeResolver,
) -> bool {
    let Type::Named { name, .. } = resolver.resolve(
        &Type::Named {
            name: name.into(),
            arguments: vec![],
        },
        module,
    ) else {
        return false;
    };
    (name == expected || name == format!("core::clone::{expected}"))
        && !resolver.is_declared_type(&name)
}

fn visit(statements: &[Stmt], module: &[String], callback: &mut impl FnMut(&Stmt, &[String])) {
    for statement in statements {
        callback(statement, module);
        if let Stmt::Module {
            name,
            statements: Some(children),
            ..
        } = statement
        {
            let mut path = module.to_vec();
            path.push(name.clone());
            visit(children, &path, callback);
        }
    }
}
