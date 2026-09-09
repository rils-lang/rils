//! Project-level declarations shared by independent document analyses.

use std::{collections::HashMap, fs, path::Path};

use rils_frontend::{
    SourceId,
    analysis::{DocumentAnalysis, ExternalModuleExport, ExternalTypeField, SymbolKind},
    ast::{Program, Stmt},
    lexer::lex_with_source_id,
    macros::STANDARD_NATIVE_MACROS,
    parse_with_capabilities,
    parser::ParseCapabilities,
};

use crate::{Server, path_to_file_uri};
use rils_project::{Project, ProjectOrigin};

pub(super) struct ProjectExportIndex {
    by_project: HashMap<std::path::PathBuf, HashMap<String, Vec<ExternalModuleExport>>>,
}

impl ProjectExportIndex {
    pub(super) fn for_project(
        &self,
        server: &Server,
        project: &Project,
        include_dependencies: bool,
    ) -> HashMap<String, Vec<ExternalModuleExport>> {
        let mut result = self
            .by_project
            .get(project.root())
            .cloned()
            .unwrap_or_default();
        if include_dependencies {
            merge_exports(
                &mut result,
                self.dependencies_for_project(server, project).iter(),
            );
        }
        result
    }

    pub(super) fn dependencies_for_project(
        &self,
        server: &Server,
        project: &Project,
    ) -> HashMap<String, Vec<ExternalModuleExport>> {
        let mut result = HashMap::new();
        for dependency in project.language_dependencies() {
            if let Some(language_project) = server
                .projects
                .iter()
                .find(|candidate| candidate.origin() == ProjectOrigin::Language(dependency))
            {
                merge_exports(
                    &mut result,
                    self.by_project
                        .get(language_project.root())
                        .into_iter()
                        .flatten(),
                );
            }
        }
        result
    }
}

pub(super) fn collect_external_exports(server: &Server) -> ProjectExportIndex {
    let mut by_project = HashMap::new();
    for project in &server.projects {
        let mut exports = HashMap::new();
        for project_file in project.modules() {
            let uri = path_to_file_uri(&project_file.path);
            let Some(program) = parse_project_file(server, project, &project_file.path) else {
                continue;
            };
            let analysis = server.project_analysis(project).or_else(|| {
                server
                    .documents
                    .get(&uri)
                    .and_then(|document| document.analysis.as_ref().ok())
            });
            collect_statements(
                &program.statements,
                &project_file.module_path,
                analysis,
                &mut exports,
            );
        }
        if let Some(path) = project.prelude()
            && let Some(program) = parse_project_file(server, project, path)
        {
            let uri = path_to_file_uri(path);
            let analysis = server.project_analysis(project).or_else(|| {
                server
                    .documents
                    .get(&uri)
                    .and_then(|document| document.analysis.as_ref().ok())
            });
            collect_statements(&program.statements, "", analysis, &mut exports);
        }
        by_project.insert(project.root().to_path_buf(), exports);
    }
    ProjectExportIndex { by_project }
}

fn merge_exports<'a>(
    target: &mut HashMap<String, Vec<ExternalModuleExport>>,
    source: impl IntoIterator<Item = (&'a String, &'a Vec<ExternalModuleExport>)>,
) {
    for (path, exports) in source {
        target
            .entry(path.clone())
            .or_default()
            .extend(exports.iter().cloned());
    }
}

fn parse_project_file(server: &Server, project: &Project, path: &Path) -> Option<Program> {
    let uri = path_to_file_uri(path);
    let (text, source_id) = server
        .documents
        .get(&uri)
        .map(|document| (document.text.clone(), document.source_id))
        .or_else(|| {
            fs::read_to_string(path)
                .ok()
                .map(|text| (text, SourceId::UNKNOWN))
        })?;
    if source_id != SourceId::UNKNOWN {
        return server
            .parse_source(source_id)
            .ok()
            .or_else(|| server.compilation.sources().last_valid_parse(source_id));
    }
    let tokens = lex_with_source_id(&text, source_id).ok()?;
    let capabilities = if matches!(project.origin(), ProjectOrigin::Language(_)) {
        ParseCapabilities::STANDARD_LIBRARY
    } else {
        ParseCapabilities::USER
    };
    parse_with_capabilities(tokens, STANDARD_NATIVE_MACROS, capabilities).ok()
}

fn collect_statements(
    statements: &[Stmt],
    module_path: &str,
    analysis: Option<&DocumentAnalysis>,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let mut module_exports = Vec::new();
    for statement in statements {
        let is_public = statement
            .visibility()
            .is_some_and(|visibility| visibility.is_public());
        if is_public {
            if let Some(export) = public_export(statement, module_path, analysis) {
                module_exports.push(export);
            }
        }
        if is_public {
            if let Stmt::Module {
                name,
                statements: Some(children),
                ..
            } = statement
            {
                let child_path = if module_path.is_empty() {
                    name.clone()
                } else {
                    format!("{module_path}::{name}")
                };
                collect_statements(children, &child_path, analysis, output);
            }
        }
    }
    output.insert(module_path.to_owned(), module_exports);
}

fn public_export(
    statement: &Stmt,
    module_path: &str,
    analysis: Option<&DocumentAnalysis>,
) -> Option<ExternalModuleExport> {
    let (name, span, kind, fields) = match statement {
        Stmt::Function {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Function, Vec::new()),
        Stmt::Struct {
            name,
            name_span,
            fields,
            ..
        } => (
            name,
            *name_span,
            SymbolKind::Type,
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
        } => (name, *name_span, SymbolKind::Type, Vec::new()),
        Stmt::Trait {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Trait, Vec::new()),
        Stmt::Module {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Module, Vec::new()),
        _ => return None,
    };
    Some(ExternalModuleExport {
        name: name.clone(),
        span,
        definition_id: analysis.and_then(|analysis| analysis.def_map.resolution(span)),
        kind,
        inferred_type: analysis.and_then(|analysis| {
            analysis
                .symbols
                .iter()
                .find(|symbol| symbol.is_definition && symbol.span == span)
                .and_then(|symbol| symbol.inferred_type.clone())
        }),
        detail: analysis.and_then(|analysis| {
            analysis
                .symbols
                .iter()
                .find(|symbol| symbol.is_definition && symbol.span == span)
                .and_then(|symbol| symbol.detail.clone())
        }),
        module_path: module_path.to_owned(),
        fields,
    })
}
