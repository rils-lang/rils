//! Project-level declarations shared by independent document analyses.

use std::{collections::HashMap, fs};

use rils_frontend::{
    SourceId,
    analysis::{ExternalModuleExport, SymbolKind},
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
            collect_statements(&program.statements, &project_file.module_path, &mut exports);
        }
    }
    exports
}

fn collect_statements(
    statements: &[Stmt],
    module_path: &str,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let mut module_exports = Vec::new();
    for statement in statements {
        let (statement, is_public) = match statement {
            Stmt::Public { statement, .. } => (statement.as_ref(), true),
            statement => (statement, false),
        };
        if is_public {
            if let Some(export) = public_export(statement) {
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
                collect_statements(children, &child_path, output);
            }
        }
    }
    if !module_path.is_empty() {
        output.insert(module_path.to_owned(), module_exports);
    }
}

fn public_export(statement: &Stmt) -> Option<ExternalModuleExport> {
    let (name, span, kind) = match statement {
        Stmt::Function {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Function),
        Stmt::Struct {
            name, name_span, ..
        }
        | Stmt::Enum {
            name, name_span, ..
        }
        | Stmt::TypeAlias {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Type),
        Stmt::Trait {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Trait),
        Stmt::Module {
            name, name_span, ..
        } => (name, *name_span, SymbolKind::Module),
        _ => return None,
    };
    Some(ExternalModuleExport {
        name: name.clone(),
        span,
        kind,
    })
}
