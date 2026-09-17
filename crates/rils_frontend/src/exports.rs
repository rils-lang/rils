//! Shared project export collection and re-export queries.
use crate::{
    Type,
    analysis::{DocumentAnalysis, ExternalModuleExport, ExternalTypeField, SymbolKind},
    ast::{Program, Stmt},
};
use std::collections::HashMap;
mod resolution;
pub use resolution::{module_candidates, resolve_export, resolve_module_path, resolve_reexports};
pub type ExportTable = HashMap<String, Vec<ExternalModuleExport>>;

pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.into()
    } else {
        format!("{parent}::{name}")
    }
}

pub fn collect_exports(
    program: &Program,
    module_path: &[String],
    analysis: Option<&DocumentAnalysis>,
    include_private: bool,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let source = program
        .statements
        .iter()
        .find_map(|statement| declaration_export(statement, "", None))
        .map(|export| export.span.source)
        .unwrap_or(crate::SourceId::UNKNOWN);
    let mut parent = String::new();
    for (index, name) in module_path.iter().enumerate() {
        let target = join_path(&parent, name);
        let siblings = output.entry(parent.clone()).or_default();
        if !siblings.iter().any(|export| export.name == *name) {
            siblings.push(ExternalModuleExport {
                name: name.clone(),
                target_module: Some(target.clone()),
                span: crate::Span::in_source(
                    if index + 1 == module_path.len() {
                        source
                    } else {
                        crate::SourceId::UNKNOWN
                    },
                    0,
                    0,
                ),
                definition_id: None,
                kind: SymbolKind::Module,
                inferred_type: None,
                detail: Some(format!("module {target}")),
                module_path: parent.clone(),
                fields: Vec::new(),
            });
        }
        parent = target;
    }
    collect_statements(
        &program.statements,
        module_path,
        analysis,
        include_private,
        output,
    );
}

pub fn collect_statements(
    statements: &[Stmt],
    module_path: &[String],
    analysis: Option<&DocumentAnalysis>,
    include_private: bool,
    output: &mut HashMap<String, Vec<ExternalModuleExport>>,
) {
    let path = module_path.join("::");
    let exports = output.entry(path.clone()).or_default();
    for statement in statements {
        let public = statement
            .visibility()
            .is_some_and(|visibility| visibility.is_public());
        if (public || include_private)
            && let Some(export) = declaration_export(statement, &path, analysis)
        {
            exports.push(export);
        }
    }
    for statement in statements {
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
        target_module: (kind == SymbolKind::Module).then(|| join_path(module_path, name)),
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
