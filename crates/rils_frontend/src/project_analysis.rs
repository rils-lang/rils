use std::collections::{HashMap, HashSet};

use crate::{
    ModuleGraph, ProjectSyntax, SourceId, Type,
    analysis::{
        DocumentAnalysis, ExternalModuleExport, ExternalTypeField, SymbolKind,
        analyze_program_in_module_with_external_exports_and_host_types,
    },
    ast::{Program, Stmt},
    types::FunctionSignature,
};

pub fn analyze_project_with_host_declarations(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
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

    let mut exports = HashMap::new();
    for (_, path, program) in &units {
        collect_exports(program, path, None, path.is_empty(), &mut exports);
    }

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
            )
        })
        .collect::<Vec<_>>();
    exports.clear();
    for ((_, path, program), analysis) in units.iter().zip(&first_pass) {
        collect_exports(program, path, Some(analysis), path.is_empty(), &mut exports);
    }

    let mut result = DocumentAnalysis::default();
    for (source, path, program) in &units {
        result.extend(
            analyze_program_in_module_with_external_exports_and_host_types(
                program,
                *source,
                host_functions,
                host_types,
                &exports,
                path,
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

fn collect_exports(
    program: &Program,
    module_path: &[String],
    analysis: Option<&DocumentAnalysis>,
    include_private: bool,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    collect_statements(
        &program.statements,
        module_path,
        analysis,
        include_private,
        output,
    );
}

fn collect_statements(
    statements: &[Stmt],
    module_path: &[String],
    analysis: Option<&DocumentAnalysis>,
    include_private: bool,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let path = module_path.join("::");
    let exports = output.entry(path.clone()).or_default();
    for statement in statements {
        let (statement, public) = match statement {
            Stmt::Public { statement, .. } => (statement.as_ref(), true),
            statement => (statement, false),
        };
        if (public || include_private)
            && let Some(export) = declaration_export(statement, &path, analysis)
        {
            exports.push(export);
        }
    }
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        if let Stmt::Module {
            name,
            statements: Some(children),
            ..
        } = statement
        {
            let mut child_path = module_path.to_vec();
            child_path.push(name.clone());
            collect_statements(children, &child_path, analysis, false, output);
        }
    }
}

fn declaration_export(
    statement: &Stmt,
    module_path: &str,
    analysis: Option<&DocumentAnalysis>,
) -> Option<ExternalModuleExport> {
    let (name, span, kind, inferred_type, fields) = match statement {
        Stmt::Function {
            name,
            name_span,
            parameters,
            return_type,
            ..
        } => (
            name,
            *name_span,
            SymbolKind::Function,
            Some(Type::function(
                parameters
                    .iter()
                    .map(|parameter| parameter.type_annotation.clone().unwrap_or(Type::Unknown))
                    .collect(),
                return_type.clone().unwrap_or(Type::Unknown),
            )),
            Vec::new(),
        ),
        Stmt::Struct {
            name,
            name_span,
            fields,
            ..
        } => (
            name,
            *name_span,
            SymbolKind::Type,
            None,
            fields
                .iter()
                .map(|field| ExternalTypeField {
                    name: field.name.clone(),
                    span: field.span,
                    ty: field.type_annotation.clone(),
                })
                .collect(),
        ),
        Stmt::Enum {
            name, name_span, ..
        }
        | Stmt::TypeAlias {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Type, None, Vec::new()),
        Stmt::Trait {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Trait, None, Vec::new()),
        Stmt::Module {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Module, None, Vec::new()),
        _ => return None,
    };
    let symbol = analysis.and_then(|analysis| {
        analysis
            .symbols
            .iter()
            .find(|symbol| symbol.is_definition && symbol.span == span)
    });
    Some(ExternalModuleExport {
        name: name.clone(),
        span,
        definition_id: analysis.and_then(|analysis| analysis.def_map.resolution(span)),
        kind,
        inferred_type: symbol
            .and_then(|symbol| symbol.inferred_type.clone())
            .or(inferred_type),
        detail: symbol.and_then(|symbol| symbol.detail.clone()),
        module_path: module_path.to_owned(),
        fields,
    })
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
