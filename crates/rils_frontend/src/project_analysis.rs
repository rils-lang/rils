use crate::exports::{collect_exports, resolve_reexports};
use std::collections::{HashMap, HashSet};

use crate::{
    ModuleGraph, ProjectSyntax, SourceId,
    analysis::{
        DocumentAnalysis, ExternalModuleExport,
        analyze_program_in_module_with_external_exports_and_host_types,
    },
    types::FunctionSignature,
};

pub fn analyze_project_with_host_declarations(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
) -> DocumentAnalysis {
    analyze_project_with_host_declarations_and_contract(
        syntax,
        modules,
        host_functions,
        host_types,
        None,
    )
}

pub(crate) fn analyze_project_with_host_declarations_and_contract(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
    host_contract: Option<&rils_host::HostContract>,
) -> DocumentAnalysis {
    analyze_project_with_host_declarations_and_contract_and_external_exports(
        syntax,
        modules,
        host_functions,
        host_types,
        host_contract,
        &HashMap::new(),
    )
}

fn analyze_project_with_host_declarations_and_contract_and_external_exports(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
    host_contract: Option<&rils_host::HostContract>,
    inherited_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> DocumentAnalysis {
    let root = syntax.root_program();
    let mut units =
        Vec::with_capacity(syntax.modules().len() + usize::from(!root.statements.is_empty()));
    if !root.statements.is_empty() {
        units.push((SourceId::UNKNOWN, Vec::new(), root));
    }
    units.extend(syntax.modules().filter_map(|(id, program)| {
        let module = modules.module(id)?;
        Some((
            module.source.unwrap_or(SourceId::UNKNOWN),
            module_path_segments(&module.path),
            program.clone(),
        ))
    }));

    let mut exports = inherited_exports.clone();
    for (_, path, program) in &units {
        collect_exports(program, path, None, path.is_empty(), &mut exports);
    }
    resolve_reexports(
        &mut exports,
        units
            .iter()
            .map(|(_, path, program)| (path.as_slice(), program)),
    );

    let first_pass = units
        .iter()
        .map(|(source, path, program)| {
            analyze_program_in_module_with_external_exports_and_host_types(
                program,
                *source,
                host_functions,
                host_types,
                &exports,
                path,
                host_contract,
            )
        })
        .collect::<Vec<_>>();
    let mut resolved_exports = inherited_exports.clone();
    for ((_, path, program), analysis) in units.iter().zip(&first_pass) {
        collect_exports(
            program,
            path,
            Some(analysis),
            path.is_empty(),
            &mut resolved_exports,
        );
    }

    let mut result = DocumentAnalysis::default();
    result.diagnostics.extend(resolve_reexports(
        &mut resolved_exports,
        units
            .iter()
            .map(|(_, path, program)| (path.as_slice(), program)),
    ));
    for (source, path, program) in &units {
        result.extend(
            analyze_program_in_module_with_external_exports_and_host_types(
                program,
                *source,
                host_functions,
                host_types,
                &resolved_exports,
                path,
                host_contract,
            ),
        );
    }
    let resolution_units = units
        .iter()
        .map(|(source, path, program)| (*source, path.as_slice(), program))
        .collect::<Vec<_>>();
    let definitions = result.def_map.clone();
    crate::semantic::resolve_project_calls(
        &resolution_units,
        &definitions,
        host_functions,
        &mut result.typeck_results,
        &result.host_type_resolutions,
    );
    let trait_units = units
        .iter()
        .map(|(_, path, program)| (path.as_slice(), program))
        .collect::<Vec<_>>();
    let trait_check =
        crate::trait_check::analyze_project(&trait_units, &result.def_map, host_types);
    result.diagnostics.extend(trait_check.diagnostics);
    result.verified_trait_impls.extend(
        trait_check
            .verified_impls
            .into_iter()
            .filter_map(|span| result.def_map.impl_at(span)),
    );
    result.diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.source,
            diagnostic.span.start,
            diagnostic.span.end,
        )
    });
    result
        .diagnostics
        .dedup_by(|left, right| left.span == right.span && left.message == right.message);
    result
}

pub fn analyze_project_with_host(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host: &rils_host::HostContract,
) -> DocumentAnalysis {
    let host_functions = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    analyze_project_with_host_declarations_and_contract(
        syntax,
        modules,
        &host_functions,
        &host_types,
        Some(host),
    )
}

pub fn analyze_project_with_host_and_external_exports(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host: &rils_host::HostContract,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> DocumentAnalysis {
    let host_functions = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    analyze_project_with_host_declarations_and_contract_and_external_exports(
        syntax,
        modules,
        &host_functions,
        &host_types,
        Some(host),
        external_exports,
    )
}

fn module_path_segments(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/project_analysis.rs"]
mod tests;
