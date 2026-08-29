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

pub(crate) struct TraitCheckResult {
    pub(crate) diagnostics: Vec<AnalysisDiagnostic>,
    pub(crate) verified_impls: Vec<crate::Span>,
}

pub(crate) fn analyze(program: &Program) -> TraitCheckResult {
    let mut traits = HashMap::new();
    let mut implementations: HashMap<String, HashSet<String>> = HashMap::new();
    collect(
        &program.statements,
        &mut Vec::new(),
        &mut traits,
        &mut implementations,
    );

    let mut result = TraitCheckResult {
        diagnostics: Vec::new(),
        verified_impls: Vec::new(),
    };
    check_impls(&program.statements, &traits, &implementations, &mut result);
    result
}

pub(crate) fn analyze_project(programs: &[(&[String], &Program)]) -> TraitCheckResult {
    let mut traits = HashMap::new();
    for (module_path, program) in programs {
        collect_project_traits(&program.statements, &mut module_path.to_vec(), &mut traits);
    }

    let mut result = TraitCheckResult {
        diagnostics: Vec::new(),
        verified_impls: Vec::new(),
    };
    for (module_path, program) in programs {
        check_project_impls(
            &program.statements,
            module_path,
            &traits,
            &mut HashMap::new(),
            &mut result,
        );
    }
    result
}

fn collect_project_traits(
    statements: &[Stmt],
    module_path: &mut Vec<String>,
    traits: &mut HashMap<String, TraitRequirement>,
) {
    for statement in statements {
        match unwrap_public(statement) {
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                module_path.push(name.clone());
                collect_project_traits(children, module_path, traits);
                module_path.pop();
            }
            Stmt::Trait {
                name,
                bounds,
                methods,
                ..
            } => {
                traits.insert(
                    qualified(module_path, name),
                    TraitRequirement {
                        name: name.clone(),
                        bounds: bounds.clone(),
                        methods: methods.clone(),
                    },
                );
            }
            _ => {}
        }
    }
}

fn check_project_impls(
    statements: &[Stmt],
    module_path: &[String],
    traits: &HashMap<String, TraitRequirement>,
    aliases: &mut HashMap<String, String>,
    result: &mut TraitCheckResult,
) {
    for statement in statements {
        match unwrap_public(statement) {
            Stmt::Use { imports, .. } => {
                collect_trait_imports(imports, module_path, traits, aliases)
            }
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                let mut child_path = module_path.to_vec();
                child_path.push(name.clone());
                check_project_impls(children, &child_path, traits, &mut HashMap::new(), result);
            }
            Stmt::Impl {
                trait_name: Some(trait_name),
                target: Type::Named { .. },
                methods,
                span,
                ..
            } => {
                let Some(trait_name) = resolve_trait_name(trait_name, module_path, aliases, traits)
                else {
                    continue;
                };
                let Some(requirement) = traits.get(&trait_name) else {
                    continue;
                };
                if check_methods(requirement, methods, &mut result.diagnostics) {
                    result.verified_impls.push(*span);
                }
            }
            _ => {}
        }
    }
}

fn collect_trait_imports(
    imports: &[crate::ast::UseImport],
    module_path: &[String],
    traits: &HashMap<String, TraitRequirement>,
    aliases: &mut HashMap<String, String>,
) {
    for import in imports {
        match import.kind {
            crate::ast::UseImportKind::Single => {
                let path = import.path.join("::");
                if let Some(trait_name) = resolve_trait_name(&path, module_path, aliases, traits)
                    && let Some(binding) = import.binding_name()
                {
                    aliases.insert(binding.to_owned(), trait_name);
                }
            }
            crate::ast::UseImportKind::Glob => {
                let Some(module) = resolve_module_path(&import.path, module_path) else {
                    continue;
                };
                let prefix = if module.is_empty() {
                    String::new()
                } else {
                    format!("{module}::")
                };
                for trait_name in traits.keys().filter(|name| {
                    name.strip_prefix(&prefix)
                        .is_some_and(|suffix| !suffix.contains("::"))
                }) {
                    let binding = trait_name
                        .rsplit("::")
                        .next()
                        .expect("trait name is non-empty");
                    aliases.insert(binding.to_owned(), trait_name.clone());
                }
            }
        }
    }
}

fn resolve_trait_name(
    name: &str,
    module_path: &[String],
    aliases: &HashMap<String, String>,
    traits: &HashMap<String, TraitRequirement>,
) -> Option<String> {
    if let Some(alias) = aliases.get(name) {
        return Some(alias.clone());
    }
    let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
    if path.len() == 1 {
        let local = qualified(module_path, name);
        if traits.contains_key(&local) {
            return Some(local);
        }
    }
    resolve_module_path(&path, module_path)
        .and_then(|module| {
            let name = path.last()?;
            if module.is_empty() {
                Some(name.clone())
            } else {
                Some(format!("{module}::{name}"))
            }
        })
        .filter(|candidate| traits.contains_key(candidate))
}

fn resolve_module_path(path: &[String], module_path: &[String]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut resolved = if matches!(path[0].as_str(), "crate" | "self" | "super") {
        match path[0].as_str() {
            "crate" => Vec::new(),
            "self" => module_path.to_vec(),
            "super" => module_path[..module_path.len().saturating_sub(1)].to_vec(),
            _ => unreachable!(),
        }
    } else {
        module_path.to_vec()
    };
    let start = usize::from(matches!(path[0].as_str(), "crate" | "self" | "super"));
    for segment in path
        .iter()
        .skip(start)
        .take(path.len().saturating_sub(start + 1))
    {
        match segment.as_str() {
            "super" => {
                resolved.pop();
            }
            "self" => {}
            "crate" => resolved.clear(),
            _ => resolved.push(segment.clone()),
        }
    }
    Some(resolved.join("::"))
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
    result: &mut TraitCheckResult,
) {
    for statement in statements {
        let statement = unwrap_public(statement);
        match statement {
            Stmt::Module {
                statements: Some(module_statements),
                ..
            } => check_impls(module_statements, traits, implementations, result),
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
                        result.diagnostics.push(AnalysisDiagnostic::error(
                            format!(
                                "type `{name}` must implement supertrait `{bound}` before implementing `{}`",
                                requirement.name
                            ),
                            *span,
                        ));
                    }
                }
                if check_methods(requirement, methods, &mut result.diagnostics) {
                    result.verified_impls.push(*span);
                }
            }
            _ => {}
        }
    }
}

fn check_methods(
    requirement: &TraitRequirement,
    methods: &[ImplMethod],
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) -> bool {
    let diagnostics_start = diagnostics.len();
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
    diagnostics.len() == diagnostics_start
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
