use std::collections::{HashMap, HashSet};

use crate::{
    DefId, Type,
    analysis::AnalysisDiagnostic,
    ast::{Program, Stmt},
};

use super::{
    TraitCheckResult, TraitRequirement, collect, collect_project_imports, qualified,
    resolve_item_name,
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum CoherenceIdentity {
    Definition(DefId),
    LocalPath(String),
    Foreign(String),
}

impl CoherenceIdentity {
    fn is_local(&self) -> bool {
        !matches!(self, Self::Foreign(_))
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct CoherenceKey {
    trait_id: CoherenceIdentity,
    target_id: CoherenceIdentity,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check_project_coherence(
    trait_name: &str,
    target: &Type,
    span: crate::Span,
    module_path: &[String],
    trait_ids: &HashMap<String, DefId>,
    types: &HashMap<String, DefId>,
    host_types: &HashSet<String>,
    trait_aliases: &HashMap<String, String>,
    type_aliases: &HashMap<String, String>,
    implementations: &mut HashMap<CoherenceKey, crate::Span>,
    result: &mut TraitCheckResult,
) -> bool {
    let trait_id = resolve_item_name(trait_name, module_path, trait_aliases, trait_ids)
        .and_then(|name| trait_ids.get(&name).copied())
        .map(CoherenceIdentity::Definition)
        .or_else(|| builtin_identity(trait_name, rils_builtins::BuiltinKind::Trait));
    let target_id = match target {
        Type::Named { name, .. } => resolve_item_name(name, module_path, type_aliases, types)
            .and_then(|name| types.get(&name).copied())
            .map(CoherenceIdentity::Definition)
            .or_else(|| builtin_type_identity(name))
            .or_else(|| host_type_identity(name, host_types)),
        target => foreign_type_identity(target),
    };
    check_coherence_pair(
        trait_name,
        target,
        span,
        trait_id,
        target_id,
        implementations,
        result,
    )
}

fn check_coherence_pair(
    trait_name: &str,
    target: &Type,
    span: crate::Span,
    trait_id: Option<CoherenceIdentity>,
    target_id: Option<CoherenceIdentity>,
    implementations: &mut HashMap<CoherenceKey, crate::Span>,
    result: &mut TraitCheckResult,
) -> bool {
    let (Some(trait_id), Some(target_id)) = (trait_id, target_id) else {
        return true;
    };
    if !trait_id.is_local() && !target_id.is_local() {
        result.diagnostics.push(AnalysisDiagnostic::error(
            "trait impl violates the orphan rule: either the trait or target type must be declared in the current project",
            span,
        ));
        return false;
    }
    let key = CoherenceKey {
        trait_id,
        target_id,
    };
    if implementations.insert(key, span).is_some() {
        result.diagnostics.push(AnalysisDiagnostic::error(
            format!("trait `{trait_name}` is already implemented for `{target}`"),
            span,
        ));
        return false;
    }
    true
}

fn builtin_identity(name: &str, kind: rils_builtins::BuiltinKind) -> Option<CoherenceIdentity> {
    rils_builtins::BUILTINS
        .iter()
        .find(|declaration| {
            declaration.kind == kind
                && (declaration.path == name || declaration.path.rsplit("::").next() == Some(name))
        })
        .map(|declaration| CoherenceIdentity::Foreign(declaration.path.to_owned()))
}

fn builtin_type_identity(name: &str) -> Option<CoherenceIdentity> {
    rils_builtins::BUILTINS
        .iter()
        .find(|declaration| {
            matches!(
                declaration.kind,
                rils_builtins::BuiltinKind::Primitive
                    | rils_builtins::BuiltinKind::Struct
                    | rils_builtins::BuiltinKind::Enum
            ) && (declaration.path == name || declaration.path.rsplit("::").next() == Some(name))
        })
        .map(|declaration| CoherenceIdentity::Foreign(declaration.path.to_owned()))
}

fn host_type_identity(name: &str, host_types: &HashSet<String>) -> Option<CoherenceIdentity> {
    host_types
        .iter()
        .find(|candidate| candidate.as_str() == name || candidate.rsplit("::").next() == Some(name))
        .map(|name| CoherenceIdentity::Foreign(name.clone()))
}

fn foreign_type_identity(target: &Type) -> Option<CoherenceIdentity> {
    matches!(
        target,
        Type::String | Type::Integer(_) | Type::Float(_) | Type::Option(_) | Type::Result(_, _)
    )
    .then(|| CoherenceIdentity::Foreign(target_constructor_name(target)))
}

fn target_constructor_name(target: &Type) -> String {
    match target {
        Type::Named { name, .. } => name.clone(),
        Type::String => "string".into(),
        Type::Integer(integer) => integer.name().into(),
        Type::Float(float) => float.name().into(),
        Type::Option(_) => "Option".into(),
        Type::Result(_, _) => "Result".into(),
        other => other.to_string(),
    }
}

pub(super) fn check_local_coherence(
    program: &Program,
    host_types: &HashSet<String>,
    result: &mut TraitCheckResult,
) {
    fn collect_types(
        statements: &[Stmt],
        module_path: &mut Vec<String>,
        types: &mut HashMap<String, ()>,
    ) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    name,
                    statements: Some(children),
                    ..
                } => {
                    module_path.push(name.clone());
                    collect_types(children, module_path, types);
                    module_path.pop();
                }
                Stmt::Struct { name, .. } | Stmt::Enum { name, .. } => {
                    types.insert(qualified(module_path, name), ());
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        statements: &[Stmt],
        module_path: &[String],
        traits: &HashMap<String, TraitRequirement>,
        types: &HashMap<String, ()>,
        host_types: &HashSet<String>,
        trait_aliases: &mut HashMap<String, String>,
        type_aliases: &mut HashMap<String, String>,
        implementations: &mut HashMap<CoherenceKey, crate::Span>,
        invalid_impls: &mut HashSet<crate::Span>,
        result: &mut TraitCheckResult,
    ) {
        for statement in statements {
            match statement {
                Stmt::Use { imports, .. } => collect_project_imports(
                    imports,
                    module_path,
                    traits,
                    types,
                    trait_aliases,
                    type_aliases,
                ),
                Stmt::Module {
                    name,
                    statements: Some(children),
                    ..
                } => {
                    let mut child_path = module_path.to_vec();
                    child_path.push(name.clone());
                    visit(
                        children,
                        &child_path,
                        traits,
                        types,
                        host_types,
                        &mut HashMap::new(),
                        &mut HashMap::new(),
                        implementations,
                        invalid_impls,
                        result,
                    );
                }
                Stmt::Impl {
                    trait_name: Some(trait_name),
                    target,
                    span,
                    ..
                } => {
                    let trait_id =
                        resolve_item_name(trait_name, module_path, trait_aliases, traits)
                            .map(CoherenceIdentity::LocalPath)
                            .or_else(|| {
                                builtin_identity(trait_name, rils_builtins::BuiltinKind::Trait)
                            });
                    let target_id = match target {
                        Type::Named { name, .. } => {
                            resolve_item_name(name, module_path, type_aliases, types)
                                .map(CoherenceIdentity::LocalPath)
                                .or_else(|| builtin_type_identity(name))
                                .or_else(|| host_type_identity(name, host_types))
                        }
                        target => foreign_type_identity(target),
                    };
                    if !check_coherence_pair(
                        trait_name,
                        target,
                        *span,
                        trait_id,
                        target_id,
                        implementations,
                        result,
                    ) {
                        invalid_impls.insert(*span);
                    }
                }
                _ => {}
            }
        }
    }

    let mut traits = HashMap::new();
    let mut ignored_implementations = HashMap::new();
    collect(
        &program.statements,
        &mut Vec::new(),
        &mut traits,
        &mut ignored_implementations,
    );
    let mut types = HashMap::new();
    collect_types(&program.statements, &mut Vec::new(), &mut types);
    let mut invalid_impls = HashSet::new();
    visit(
        &program.statements,
        &[],
        &traits,
        &types,
        host_types,
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut invalid_impls,
        result,
    );
    result
        .verified_impls
        .retain(|span| !invalid_impls.contains(span));
}
