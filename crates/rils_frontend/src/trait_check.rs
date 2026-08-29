use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{ImplMethod, Program, Stmt, TraitMethod},
    types::Type,
};

#[derive(Clone)]
struct TraitRequirement {
    name: String,
    bounds: Vec<String>,
    methods: Vec<TraitMethod>,
}

pub(crate) fn analyze(program: &Program) -> Vec<AnalysisDiagnostic> {
    let mut traits = HashMap::new();
    let mut implementations: HashMap<String, HashSet<String>> = HashMap::new();
    collect(
        &program.statements,
        &mut Vec::new(),
        &mut traits,
        &mut implementations,
    );

    let mut diagnostics = Vec::new();
    check_impls(
        &program.statements,
        &traits,
        &implementations,
        &mut diagnostics,
    );
    diagnostics
}

fn collect(
    statements: &[Stmt],
    path: &mut Vec<String>,
    traits: &mut HashMap<String, TraitRequirement>,
    implementations: &mut HashMap<String, HashSet<String>>,
) {
    for statement in statements {
        let statement = unwrap_public(statement);
        match statement {
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                path.push(name.clone());
                collect(module_statements, path, traits, implementations);
                path.pop();
            }
            Stmt::Trait {
                name,
                bounds,
                methods,
                ..
            } => {
                let requirement = TraitRequirement {
                    name: name.clone(),
                    bounds: bounds.clone(),
                    methods: methods.clone(),
                };
                traits.insert(name.clone(), requirement.clone());
                traits.insert(qualified(path, name), requirement);
            }
            Stmt::Impl {
                trait_name: Some(trait_name),
                target: Type::Named { name, .. },
                ..
            } => {
                implementations
                    .entry(name.clone())
                    .or_default()
                    .insert(trait_name.clone());
            }
            _ => {}
        }
    }
}

fn check_impls(
    statements: &[Stmt],
    traits: &HashMap<String, TraitRequirement>,
    implementations: &HashMap<String, HashSet<String>>,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for statement in statements {
        let statement = unwrap_public(statement);
        match statement {
            Stmt::Module {
                statements: Some(module_statements),
                ..
            } => check_impls(module_statements, traits, implementations, diagnostics),
            Stmt::Impl {
                trait_name: Some(trait_name),
                target: Type::Named { name, .. },
                methods,
                span,
                ..
            } => {
                let Some(requirement) = traits.get(trait_name) else {
                    continue;
                };
                for bound in &requirement.bounds {
                    if !implementations
                        .get(name)
                        .is_some_and(|implemented| implemented.contains(bound))
                    {
                        diagnostics.push(AnalysisDiagnostic::error(
                            format!(
                                "type `{name}` must implement supertrait `{bound}` before implementing `{}`",
                                requirement.name
                            ),
                            *span,
                        ));
                    }
                }
                check_methods(requirement, methods, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_methods(
    requirement: &TraitRequirement,
    methods: &[ImplMethod],
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for required in &requirement.methods {
        let Some(implementation) = methods.iter().find(|method| method.name == required.name)
        else {
            diagnostics.push(AnalysisDiagnostic::error(
                format!(
                    "impl of trait `{}` is missing method `{}`",
                    requirement.name, required.name
                ),
                required.span,
            ));
            continue;
        };
        if !method_signature_matches(required, implementation) {
            diagnostics.push(AnalysisDiagnostic::error(
                format!(
                    "method `{}` does not match its trait signature",
                    required.name
                ),
                implementation.span,
            ));
        }
    }
    if let Some(extra) = methods.iter().find(|method| {
        !requirement
            .methods
            .iter()
            .any(|required| required.name == method.name)
    }) {
        diagnostics.push(AnalysisDiagnostic::error(
            format!(
                "method `{}` is not a member of trait `{}`",
                extra.name, requirement.name
            ),
            extra.span,
        ));
    }
}

fn method_signature_matches(required: &TraitMethod, implementation: &ImplMethod) -> bool {
    required.generic_parameters.len() == implementation.generic_parameters.len()
        && required.parameters.len() == implementation.parameters.len()
        && required
            .generic_parameters
            .iter()
            .zip(&implementation.generic_parameters)
            .all(|(required, actual)| {
                required.name == actual.name && required.bounds == actual.bounds
            })
        && required
            .parameters
            .iter()
            .zip(&implementation.parameters)
            .all(|(required, actual)| {
                required.name == actual.name
                    && required.type_annotation == actual.type_annotation
                    && required.mutable == actual.mutable
            })
        && required.return_type == implementation.return_type
}

fn qualified(path: &[String], name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", path.join("::"))
    }
}

fn unwrap_public(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}

#[cfg(test)]
#[path = "../tests/unit/trait_check.rs"]
mod tests;
