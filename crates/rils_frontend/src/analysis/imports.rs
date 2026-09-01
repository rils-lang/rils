//! Static symbol registration for flattened `use` trees.

use std::collections::HashMap;

use crate::{
    ast::{Stmt, UseImport, UseImportKind},
    source::Span,
};

use super::{
    AnalysisDiagnostic, Analyzer, Definition, SymbolContainer, SymbolKind, SymbolOccurrence,
};

#[derive(Clone)]
pub(super) struct ModuleExport {
    pub(super) name: String,
    pub(super) span: Span,
    pub(super) definition_id: Option<crate::DefId>,
    pub(super) kind: SymbolKind,
    pub(super) inferred_type: Option<crate::types::Type>,
    pub(super) detail: Option<String>,
    pub(super) module_path: String,
}

pub(super) fn analyze(analyzer: &mut Analyzer, imports: &[UseImport]) {
    for import in imports {
        let exported = imported_export(analyzer, import);
        for (index, (segment, segment_span)) in
            import.path.iter().zip(&import.path_spans).enumerate()
        {
            let is_imported_item =
                index + 1 == import.path.len() && import.kind == UseImportKind::Single;
            if index == 0 && !is_imported_item {
                analyzer.reference(segment, *segment_span, SymbolKind::Module);
            } else {
                analyzer.result.symbols.push(SymbolOccurrence {
                    name: segment.clone(),
                    span: *segment_span,
                    definition_span: is_imported_item
                        .then(|| exported.as_ref().map(|export| export.span))
                        .flatten(),
                    symbol_id: None,
                    definition_id: is_imported_item
                        .then(|| exported.as_ref().and_then(|export| export.definition_id))
                        .flatten(),
                    kind: if is_imported_item {
                        exported
                            .as_ref()
                            .map(|export| export.kind)
                            .unwrap_or(SymbolKind::Function)
                    } else {
                        SymbolKind::Module
                    },
                    is_definition: false,
                    inferred_type: is_imported_item
                        .then(|| {
                            exported
                                .as_ref()
                                .and_then(|export| export.inferred_type.clone())
                        })
                        .flatten(),
                    detail: is_imported_item
                        .then(|| exported.as_ref().and_then(|export| export.detail.clone()))
                        .flatten(),
                    container: is_imported_item
                        .then(|| {
                            exported
                                .as_ref()
                                .map(|export| SymbolContainer::Module(export.module_path.clone()))
                        })
                        .flatten(),
                });
            }
        }
        let Some(name) = import.binding_name() else {
            import_glob(analyzer, import);
            continue;
        };
        let name_span = import.alias_span.unwrap_or(import.name_span);
        let kind = exported.as_ref().map_or_else(
            || {
                if name.chars().next().is_some_and(char::is_uppercase) {
                    SymbolKind::Type
                } else {
                    SymbolKind::Function
                }
            },
            |export| export.kind,
        );
        analyzer.define(name, name_span, kind);
        if let Some(exported) = exported {
            if let Some(detail) = exported.detail {
                analyzer.set_last_detail(detail);
            }
            analyzer.set_last_container(SymbolContainer::Module(exported.module_path));
        }
    }
}

fn imported_export(analyzer: &Analyzer, import: &UseImport) -> Option<ModuleExport> {
    let name = import.path.last()?;
    module_candidates(
        &analyzer.module_path,
        &import.path[..import.path.len().saturating_sub(1)],
    )
    .iter()
    .find_map(|module| analyzer.module_exports.get(module))
    .and_then(|exports| exports.iter().find(|export| export.name == *name))
    .cloned()
}

pub(super) fn path_export(
    exports: &HashMap<String, Vec<ModuleExport>>,
    prefix: &[String],
    path: &[String],
) -> Option<ModuleExport> {
    let name = path.last()?;
    module_candidates(prefix, &path[..path.len().saturating_sub(1)])
        .iter()
        .find_map(|module| exports.get(module))
        .and_then(|exports| exports.iter().find(|export| export.name == *name))
        .cloned()
}

fn import_glob(analyzer: &mut Analyzer, import: &UseImport) {
    let exports = module_candidates(&analyzer.module_path, &import.path)
        .iter()
        .find_map(|module| analyzer.module_exports.get(module))
        .cloned();
    let Some(exports) = exports else {
        *analyzer.glob_imports.last_mut().expect("scope exists") = true;
        return;
    };
    for export in exports {
        if analyzer
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(&export.name))
        {
            analyzer.result.diagnostics.push(AnalysisDiagnostic::error(
                format!("`{}` is already defined in this scope", export.name),
                import.span,
            ));
            continue;
        }
        analyzer.scopes.last_mut().expect("scope exists").insert(
            export.name,
            Definition {
                span: Some(export.span),
                id: export.definition_id,
                kind: export.kind,
                container: Some(SymbolContainer::Module(export.module_path)),
            },
        );
    }
}

pub(super) fn collect_module_exports(
    statements: &[Stmt],
    module_path: &[String],
) -> HashMap<String, Vec<ModuleExport>> {
    fn visit(
        statements: &[Stmt],
        prefix: &mut Vec<String>,
        output: &mut HashMap<String, Vec<ModuleExport>>,
    ) {
        for statement in statements {
            let Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } = statement
            else {
                continue;
            };
            prefix.push(name.clone());
            output.insert(
                prefix.join("::"),
                module_statements
                    .iter()
                    .filter_map(|statement| public_export(statement, &prefix.join("::")))
                    .collect(),
            );
            visit(module_statements, prefix, output);
            prefix.pop();
        }
    }

    let mut output = HashMap::new();
    visit(statements, &mut module_path.to_vec(), &mut output);
    output
}

fn public_export(statement: &Stmt, module_path: &str) -> Option<ModuleExport> {
    if !statement
        .visibility()
        .is_some_and(|visibility| visibility.is_public())
    {
        return None;
    }
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
    Some(ModuleExport {
        name: name.clone(),
        span,
        definition_id: None,
        kind,
        inferred_type: None,
        detail: None,
        module_path: module_path.to_owned(),
    })
}

fn module_candidates(prefix: &[String], path: &[String]) -> Vec<String> {
    let Some(first) = path.first().map(String::as_str) else {
        return Vec::new();
    };
    if matches!(first, "crate" | "self" | "super") {
        let mut output = match first {
            "crate" => Vec::new(),
            "self" => prefix.to_vec(),
            "super" => {
                let mut output = prefix.to_vec();
                output.pop();
                output
            }
            _ => unreachable!(),
        };
        for segment in path.iter().skip(1) {
            match segment.as_str() {
                "crate" => output.clear(),
                "self" => {}
                "super" => {
                    output.pop();
                }
                _ => output.push(segment.clone()),
            }
        }
        return vec![output.join("::")];
    }
    let absolute = path.join("::");
    if prefix.is_empty() {
        vec![absolute]
    } else {
        vec![format!("{}::{absolute}", prefix.join("::")), absolute]
    }
}
