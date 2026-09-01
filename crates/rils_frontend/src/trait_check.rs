use std::collections::{HashMap, HashSet};

use crate::{
    DefId,
    analysis::AnalysisDiagnostic,
    ast::{AssociatedType, GenericParameter, ImplMethod, Program, Stmt, TraitMethod},
    types::Type,
};

mod coherence;

use coherence::{CoherenceKey, check_local_coherence, check_project_coherence};

#[derive(Clone)]
struct TraitRequirement {
    name: String,
    bounds: Vec<String>,
    associated_types: Vec<AssociatedType>,
    methods: Vec<TraitMethod>,
}

struct ProjectDeclarations<'a> {
    traits: &'a HashMap<String, TraitRequirement>,
    trait_ids: &'a HashMap<String, DefId>,
    types: &'a HashMap<String, DefId>,
    host_types: &'a HashSet<String>,
}

pub(crate) struct TraitCheckResult {
    pub(crate) diagnostics: Vec<AnalysisDiagnostic>,
    pub(crate) verified_impls: Vec<crate::Span>,
}

#[cfg(test)]
pub(crate) fn analyze(program: &Program) -> TraitCheckResult {
    analyze_with_host_types(program, &HashSet::new())
}

pub(crate) fn analyze_with_host_types(
    program: &Program,
    host_types: &HashSet<String>,
) -> TraitCheckResult {
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
    check_local_coherence(program, host_types, &mut result);
    result
}

pub(crate) fn analyze_project(
    programs: &[(&[String], &Program)],
    definitions: &crate::semantic::DefMap,
    host_types: &HashSet<String>,
) -> TraitCheckResult {
    let mut traits = HashMap::new();
    let mut trait_ids = HashMap::new();
    let mut types = HashMap::new();
    for (module_path, program) in programs {
        collect_project_declarations(
            &program.statements,
            &mut module_path.to_vec(),
            definitions,
            &mut traits,
            &mut trait_ids,
            &mut types,
        );
    }

    let mut result = TraitCheckResult {
        diagnostics: Vec::new(),
        verified_impls: Vec::new(),
    };
    let declarations = ProjectDeclarations {
        traits: &traits,
        trait_ids: &trait_ids,
        types: &types,
        host_types,
    };
    let mut implementations = HashMap::new();
    for (module_path, program) in programs {
        check_project_impls(
            &program.statements,
            module_path,
            &declarations,
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut implementations,
            &mut result,
        );
    }
    result
}

fn collect_project_declarations(
    statements: &[Stmt],
    module_path: &mut Vec<String>,
    definitions: &crate::semantic::DefMap,
    traits: &mut HashMap<String, TraitRequirement>,
    trait_ids: &mut HashMap<String, DefId>,
    types: &mut HashMap<String, DefId>,
) {
    for statement in statements {
        match statement {
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                module_path.push(name.clone());
                collect_project_declarations(
                    children,
                    module_path,
                    definitions,
                    traits,
                    trait_ids,
                    types,
                );
                module_path.pop();
            }
            Stmt::Trait {
                name,
                name_span,
                bounds,
                associated_types,
                methods,
                ..
            } => {
                let path = qualified(module_path, name);
                traits.insert(
                    path.clone(),
                    TraitRequirement {
                        name: name.clone(),
                        bounds: bounds.clone(),
                        associated_types: associated_types.clone(),
                        methods: methods.clone(),
                    },
                );
                if let Some(definition) = definitions.definition_at(*name_span) {
                    trait_ids.insert(path, definition.id);
                }
            }
            Stmt::Struct {
                name, name_span, ..
            }
            | Stmt::Enum {
                name, name_span, ..
            } => {
                if let Some(definition) = definitions.definition_at(*name_span) {
                    types.insert(qualified(module_path, name), definition.id);
                }
            }
            _ => {}
        }
    }
}

fn check_project_impls(
    statements: &[Stmt],
    module_path: &[String],
    declarations: &ProjectDeclarations<'_>,
    trait_aliases: &mut HashMap<String, String>,
    type_aliases: &mut HashMap<String, String>,
    implementations: &mut HashMap<CoherenceKey, crate::Span>,
    result: &mut TraitCheckResult,
) {
    for statement in statements {
        match statement {
            Stmt::Use { imports, .. } => collect_project_imports(
                imports,
                module_path,
                declarations.traits,
                declarations.types,
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
                check_project_impls(
                    children,
                    &child_path,
                    declarations,
                    &mut HashMap::new(),
                    &mut HashMap::new(),
                    implementations,
                    result,
                );
            }
            Stmt::Impl {
                generic_parameters,
                trait_name: Some(trait_name),
                target,
                associated_types,
                methods,
                span,
                ..
            } => {
                let supported =
                    check_impl_generic_bounds(generic_parameters, &mut result.diagnostics);
                let coherent = check_project_coherence(
                    trait_name,
                    target,
                    *span,
                    module_path,
                    declarations.trait_ids,
                    declarations.types,
                    declarations.host_types,
                    trait_aliases,
                    type_aliases,
                    implementations,
                    result,
                );
                let Some(trait_name) =
                    resolve_item_name(trait_name, module_path, trait_aliases, declarations.traits)
                else {
                    continue;
                };
                let Some(requirement) = declarations.traits.get(&trait_name) else {
                    continue;
                };
                let contract_valid = check_contract(
                    requirement,
                    associated_types,
                    methods,
                    *span,
                    &mut result.diagnostics,
                );
                if supported && contract_valid && coherent {
                    result.verified_impls.push(*span);
                }
            }
            _ => {}
        }
    }
}

fn collect_project_imports<T>(
    imports: &[crate::ast::UseImport],
    module_path: &[String],
    traits: &HashMap<String, TraitRequirement>,
    types: &HashMap<String, T>,
    trait_aliases: &mut HashMap<String, String>,
    type_aliases: &mut HashMap<String, String>,
) {
    for import in imports {
        match import.kind {
            crate::ast::UseImportKind::Single => {
                let path = import.path.join("::");
                if let Some(trait_name) =
                    resolve_item_name(&path, module_path, trait_aliases, traits)
                    && let Some(binding) = import.binding_name()
                {
                    trait_aliases.insert(binding.to_owned(), trait_name);
                }
                if let Some(type_name) = resolve_item_name(&path, module_path, type_aliases, types)
                    && let Some(binding) = import.binding_name()
                {
                    type_aliases.insert(binding.to_owned(), type_name);
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
                    trait_aliases.insert(binding.to_owned(), trait_name.clone());
                }
                for type_name in types.keys().filter(|name| {
                    name.strip_prefix(&prefix)
                        .is_some_and(|suffix| !suffix.contains("::"))
                }) {
                    let binding = type_name
                        .rsplit("::")
                        .next()
                        .expect("type name is non-empty");
                    type_aliases.insert(binding.to_owned(), type_name.clone());
                }
            }
        }
    }
}

fn resolve_item_name<T>(
    name: &str,
    module_path: &[String],
    aliases: &HashMap<String, String>,
    items: &HashMap<String, T>,
) -> Option<String> {
    if let Some(alias) = aliases.get(name) {
        return Some(alias.clone());
    }
    let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
    if path.len() == 1 {
        let local = qualified(module_path, name);
        if items.contains_key(&local) {
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
        .filter(|candidate| items.contains_key(candidate))
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
                associated_types,
                methods,
                ..
            } => {
                let requirement = TraitRequirement {
                    name: name.clone(),
                    bounds: bounds.clone(),
                    associated_types: associated_types.clone(),
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
        match statement {
            Stmt::Module {
                statements: Some(module_statements),
                ..
            } => check_impls(module_statements, traits, implementations, result),
            Stmt::Impl {
                generic_parameters,
                trait_name: Some(trait_name),
                target: Type::Named { name, .. },
                associated_types,
                methods,
                span,
                ..
            } => {
                let supported =
                    check_impl_generic_bounds(generic_parameters, &mut result.diagnostics);
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
                let contract_valid = check_contract(
                    requirement,
                    associated_types,
                    methods,
                    *span,
                    &mut result.diagnostics,
                );
                if supported && contract_valid {
                    result.verified_impls.push(*span);
                }
            }
            _ => {}
        }
    }
}

fn check_impl_generic_bounds(
    generic_parameters: &[GenericParameter],
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) -> bool {
    let diagnostics_start = diagnostics.len();
    for parameter in generic_parameters
        .iter()
        .filter(|parameter| !parameter.bounds.is_empty())
    {
        diagnostics.push(AnalysisDiagnostic::error(
            "conditional trait impl bounds are not supported yet",
            parameter.span,
        ));
    }
    diagnostics.len() == diagnostics_start
}

fn check_contract(
    requirement: &TraitRequirement,
    associated_types: &[AssociatedType],
    methods: &[ImplMethod],
    impl_span: crate::Span,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) -> bool {
    let diagnostics_start = diagnostics.len();
    check_associated_types(requirement, associated_types, impl_span, diagnostics);
    check_methods(requirement, methods, diagnostics);
    diagnostics.len() == diagnostics_start
}

fn check_associated_types(
    requirement: &TraitRequirement,
    implementations: &[AssociatedType],
    impl_span: crate::Span,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for required in &requirement.associated_types {
        let Some(implementation) = implementations
            .iter()
            .find(|implementation| implementation.name == required.name)
        else {
            if required.value.is_none() {
                diagnostics.push(AnalysisDiagnostic::error(
                    format!(
                        "impl of trait `{}` is missing associated type `{}`",
                        requirement.name, required.name
                    ),
                    impl_span,
                ));
            }
            continue;
        };
        if implementation.generic_parameters.len() != required.generic_parameters.len() {
            diagnostics.push(AnalysisDiagnostic::error(
                format!(
                    "associated type `{}` has the wrong number of generic parameters",
                    required.name
                ),
                implementation.span,
            ));
        }
    }
    if let Some(extra) = implementations.iter().find(|implementation| {
        !requirement
            .associated_types
            .iter()
            .any(|required| required.name == implementation.name)
    }) {
        diagnostics.push(AnalysisDiagnostic::error(
            format!(
                "associated type `{}` is not a member of trait `{}`",
                extra.name, requirement.name
            ),
            extra.span,
        ));
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

#[cfg(test)]
#[path = "../tests/unit/trait_check.rs"]
mod tests;
