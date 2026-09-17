//! Static symbol registration for flattened `use` trees.

use std::collections::HashMap;

use crate::ast::{Stmt, UseImport, UseImportKind};

use super::{
    AnalysisDiagnostic, Analyzer, Definition, SymbolContainer, SymbolKind, SymbolOccurrence,
};

pub(super) type ModuleExport = super::ExternalModuleExport;

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
            if let Some(detail) = exported.detail.clone() {
                analyzer.set_last_detail(detail);
            }
            analyzer.set_last_container(SymbolContainer::Module(exported.module_path.clone()));
            let definition = analyzer
                .scopes
                .last_mut()
                .expect("scope")
                .get_mut(name)
                .expect("import binding");
            definition.span = Some(exported.span);
            definition.id = exported.definition_id;
            let symbol = analyzer
                .result
                .symbols
                .last_mut()
                .expect("import occurrence");
            symbol.definition_id = exported.definition_id;
            symbol.definition_span = Some(exported.span);
            symbol.inferred_type = exported.inferred_type;
        }
    }
}

fn imported_export(analyzer: &Analyzer, import: &UseImport) -> Option<ModuleExport> {
    path_export(
        &analyzer.module_exports,
        &analyzer.module_path,
        &import.path,
    )
}

pub(super) fn path_export(
    exports: &HashMap<String, Vec<ModuleExport>>,
    prefix: &[String],
    path: &[String],
) -> Option<ModuleExport> {
    crate::exports::resolve_export(exports, prefix, path).cloned()
}

fn import_glob(analyzer: &mut Analyzer, import: &UseImport) {
    let exports = crate::exports::resolve_module_path(
        &analyzer.module_exports,
        &analyzer.module_path,
        &import.path,
    )
    .and_then(|module| analyzer.module_exports.get(&module))
    .cloned();
    let Some(exports) = exports else {
        *analyzer.glob_imports.last_mut().expect("scope exists") = true;
        return;
    };
    for export in exports {
        if !analyzer.replaces_builtin(&export.name, import.span, export.kind)
            && analyzer
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
    let mut exports = HashMap::new();
    crate::exports::collect_statements(statements, module_path, None, false, &mut exports);
    exports
}
