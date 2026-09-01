use std::collections::{HashMap, HashSet};

use rils_frontend::{ModuleGraph, ModuleId, ProjectSyntax};

use super::RuntimeError;
use crate::{ast::Stmt, source::Span};

pub(super) fn module_initialization_order(
    syntax: &ProjectSyntax,
    graph: &ModuleGraph,
) -> Result<Vec<ModuleId>, RuntimeError> {
    let modules = syntax
        .modules()
        .map(|(module, _)| module)
        .collect::<HashSet<_>>();
    let mut dependencies = HashMap::new();
    for (module, program) in syntax.modules() {
        let mut module_dependencies = Vec::new();
        for statement in &program.statements {
            let Stmt::Use { imports, .. } = statement else {
                continue;
            };
            for import in imports {
                let dependency = (1..=import.path.len()).rev().find_map(|length| {
                    graph
                        .resolve(module, &import.path[..length].join("::"))
                        .map(|module| module.id)
                });
                if let Some(dependency) = dependency
                    && dependency != module
                    && modules.contains(&dependency)
                    && !module_dependencies.contains(&dependency)
                {
                    module_dependencies.push(dependency);
                }
            }
        }
        dependencies.insert(module, module_dependencies);
    }

    let mut order = Vec::with_capacity(modules.len());
    let mut visited = HashSet::new();
    for (module, _) in syntax.modules() {
        visit(
            module,
            graph,
            &dependencies,
            &mut Vec::new(),
            &mut visited,
            &mut order,
        )?;
    }
    Ok(order)
}

fn visit(
    module: ModuleId,
    graph: &ModuleGraph,
    dependencies: &HashMap<ModuleId, Vec<ModuleId>>,
    visiting: &mut Vec<ModuleId>,
    visited: &mut HashSet<ModuleId>,
    order: &mut Vec<ModuleId>,
) -> Result<(), RuntimeError> {
    if visited.contains(&module) {
        return Ok(());
    }
    if let Some(start) = visiting.iter().position(|candidate| *candidate == module) {
        let mut cycle = visiting[start..]
            .iter()
            .filter_map(|module| graph.module(*module))
            .map(|module| module.path.clone())
            .collect::<Vec<_>>();
        cycle.push(
            graph
                .module(module)
                .map(|module| module.path.clone())
                .unwrap_or_else(|| "<unknown>".into()),
        );
        return Err(RuntimeError::new(
            format!("project module import cycle: {}", cycle.join(" -> ")),
            Span::default(),
        ));
    }
    visiting.push(module);
    if let Some(module_dependencies) = dependencies.get(&module) {
        for dependency in module_dependencies {
            visit(*dependency, graph, dependencies, visiting, visited, order)?;
        }
    }
    visiting.pop();
    visited.insert(module);
    order.push(module);
    Ok(())
}
