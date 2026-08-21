//! Project-level declarations shared by independent document analyses.

use std::{collections::HashMap, fs};

use rils_frontend::{
    SourceId,
    analysis::{DocumentAnalysis, ExternalModuleExport, ExternalTypeField, SymbolKind},
    ast::Stmt,
    lexer::lex_with_source_id,
    parser::parse,
};

use crate::{Server, path_to_file_uri};

pub(super) fn collect_external_exports(
    server: &Server,
) -> HashMap<String, Vec<ExternalModuleExport>> {
    let mut exports = HashMap::new();
    for project in &server.projects {
        for project_file in project.modules() {
            let uri = path_to_file_uri(&project_file.path);
            let Some((text, source_id)) = server
                .documents
                .get(&uri)
                .map(|document| (document.text.clone(), document.source_id))
                .or_else(|| {
                    fs::read_to_string(&project_file.path)
                        .ok()
                        .map(|text| (text, SourceId::UNKNOWN))
                })
            else {
                continue;
            };
            let Ok(tokens) = lex_with_source_id(&text, source_id) else {
                continue;
            };
            let Ok(program) = parse(tokens) else {
                continue;
            };
            let analysis = server
                .documents
                .get(&uri)
                .and_then(|document| document.analysis.as_ref().ok());
            collect_statements(
                &program.statements,
                &project_file.module_path,
                analysis,
                &mut exports,
            );
        }
    }
    exports
}

fn collect_statements(
    statements: &[Stmt],
    module_path: &str,
    analysis: Option<&DocumentAnalysis>,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let mut module_exports = Vec::new();
    for statement in statements {
        let (statement, is_public) = match statement {
            Stmt::Public { statement, .. } => (statement.as_ref(), true),
            statement => (statement, false),
        };
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
    if !module_path.is_empty() {
        output.insert(module_path.to_owned(), module_exports);
    }
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
