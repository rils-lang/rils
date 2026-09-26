//! Checks the item relationship required by explicit IntoIterator implementations.

use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{AssociatedType, Program, Stmt},
    types::Type,
};

struct IteratorItem {
    owner: String,
    parameters: Vec<String>,
    item: Type,
}

pub(super) fn check(programs: &[(&[String], &Program)], diagnostics: &mut Vec<AnalysisDiagnostic>) {
    let mut items = Vec::new();
    let mut types = HashSet::new();
    for (module, program) in programs {
        collect(&program.statements, module, &mut items, &mut types);
    }
    for (module, program) in programs {
        check_statements(&program.statements, module, &items, &types, diagnostics);
    }
}

fn collect(
    statements: &[Stmt],
    module: &[String],
    items: &mut Vec<IteratorItem>,
    types: &mut HashSet<String>,
) {
    for statement in statements {
        match statement {
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                let mut nested = module.to_vec();
                nested.push(name.clone());
                collect(children, &nested, items, types);
            }
            Stmt::Struct { name, .. } | Stmt::Enum { name, .. } => {
                types.insert(qualify(module, name));
            }
            Stmt::Impl {
                target: Type::Named { name, .. },
                trait_name: Some(trait_name),
                generic_parameters,
                associated_types,
                ..
            } if trait_name.rsplit("::").next() == Some("Iterator") => {
                if let Some(item) = associated_value(associated_types, "Item") {
                    items.push(IteratorItem {
                        owner: qualify(module, name),
                        parameters: generic_parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        item: item.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn check_statements(
    statements: &[Stmt],
    module: &[String],
    items: &[IteratorItem],
    types: &HashSet<String>,
    diagnostics: &mut Vec<AnalysisDiagnostic>,
) {
    for statement in statements {
        match statement {
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                let mut nested = module.to_vec();
                nested.push(name.clone());
                check_statements(children, &nested, items, types, diagnostics);
            }
            Stmt::Impl {
                trait_name: Some(trait_name),
                associated_types,
                ..
            } if trait_name.rsplit("::").next() == Some("IntoIterator") => {
                let (Some(declared), Some(iterator)) = (
                    associated_value(associated_types, "Item"),
                    associated_value(associated_types, "IntoIter"),
                ) else {
                    continue;
                };
                let Some(expected) = iterator_item(iterator, module, items) else {
                    if let Type::Named { name, .. } = iterator
                        && (types.contains(&qualify(module, name))
                            || types.contains(name)
                            || rils_builtins::builtin(name.rsplit("::").next().unwrap_or(name))
                                .is_some())
                    {
                        let span = associated_types
                            .iter()
                            .find(|associated| associated.name == "IntoIter")
                            .expect("declared IntoIter")
                            .span;
                        diagnostics.push(AnalysisDiagnostic::error(
                            format!("IntoIterator::IntoIter `{iterator}` must implement Iterator"),
                            span,
                        ));
                    }
                    continue;
                };
                if declared != &expected && expected != Type::Unknown && *declared != Type::Unknown
                {
                    let span = associated_types
                        .iter()
                        .find(|associated| associated.name == "Item")
                        .expect("declared Item")
                        .span;
                    diagnostics.push(AnalysisDiagnostic::error(
                        format!(
                            "IntoIterator::Item must match IntoIter::Item: expected `{expected}`, found `{declared}`"
                        ),
                        span,
                    ));
                }
            }
            _ => {}
        }
    }
}

fn associated_value<'a>(types: &'a [AssociatedType], name: &str) -> Option<&'a Type> {
    types.iter().find(|item| item.name == name)?.value.as_ref()
}

fn iterator_item(iterator: &Type, module: &[String], items: &[IteratorItem]) -> Option<Type> {
    let Type::Named { name, arguments } = iterator else {
        return crate::standard_library::builtin_iterator_item_type(iterator);
    };
    let qualified = qualify(module, name);
    let definition = items
        .iter()
        .find(|item| item.owner == qualified)
        .or_else(|| items.iter().find(|item| item.owner == *name))
        .or_else(|| {
            let mut matches = items
                .iter()
                .filter(|item| item.owner.rsplit("::").next() == Some(name));
            let candidate = matches.next()?;
            matches.next().is_none().then_some(candidate)
        });
    if let Some(definition) = definition {
        let substitutions = definition
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        return Some(definition.item.substitute(&substitutions));
    }
    crate::standard_library::builtin_iterator_item_type(iterator)
}

fn qualify(module: &[String], name: &str) -> String {
    if module.is_empty() || name.contains("::") {
        name.to_owned()
    } else {
        format!("{}::{name}", module.join("::"))
    }
}
