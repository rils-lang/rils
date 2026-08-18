use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{Program, Stmt},
    types::Type,
};

#[derive(Clone)]
struct TraitRequirement {
    name: String,
    bounds: Vec<String>,
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
            Stmt::Trait { name, bounds, .. } => {
                let requirement = TraitRequirement {
                    name: name.clone(),
                    bounds: bounds.clone(),
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
            }
            _ => {}
        }
    }
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
mod tests {
    use crate::{analysis::analyze_program, lexer::lex, parser::parse};

    #[test]
    fn requires_supertraits_for_trait_implementations() {
        let missing = parse(
            lex("trait Behaviour: Default {} struct State; impl Behaviour for State {}").unwrap(),
        )
        .unwrap();
        let diagnostics = analyze_program(&missing).diagnostics;
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("must implement supertrait `Default`")
        }));

        let valid = parse(
            lex("trait Behaviour: Default {} #[derive(Default)] struct State; impl Behaviour for State {}")
                .unwrap(),
        )
        .unwrap();
        assert!(analyze_program(&valid).diagnostics.is_empty());
    }
}
