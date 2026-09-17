//! Monotone re-export resolution. Aliases retain their declaration identity.
use super::{ExportTable, join_path};
use crate::{
    analysis::{AnalysisDiagnostic, ExternalModuleExport},
    ast::{Program, Stmt, UseImport, UseImportKind},
};

pub fn module_candidates(prefix: &[String], path: &[String]) -> Vec<String> {
    let Some(first) = path.first().map(String::as_str) else {
        return if prefix.is_empty() {
            vec![String::new()]
        } else {
            vec![prefix.join("::"), String::new()]
        };
    };
    if matches!(first, "crate" | "self" | "super") {
        let mut result = if first == "crate" {
            Vec::new()
        } else {
            prefix.to_vec()
        };
        for segment in path {
            match segment.as_str() {
                "crate" => result.clear(),
                "self" => {}
                "super" => {
                    if result.pop().is_none() {
                        return Vec::new();
                    }
                }
                _ => result.push(segment.clone()),
            }
        }
        return vec![result.join("::")];
    }
    let path = path.join("::");
    if prefix.is_empty() {
        vec![path]
    } else {
        vec![join_path(&prefix.join("::"), &path), path]
    }
}

pub fn resolve_module_path(
    exports: &ExportTable,
    prefix: &[String],
    path: &[String],
) -> Option<String> {
    for candidate in module_candidates(prefix, path) {
        if exports.contains_key(&candidate) {
            return Some(candidate);
        }
        let mut current = String::new();
        let mut valid = true;
        for segment in candidate.split("::").filter(|part| !part.is_empty()) {
            let direct = join_path(&current, segment);
            if exports.contains_key(&direct) {
                current = direct;
                continue;
            }
            let Some(module) = unique(
                exports
                    .get(&current)
                    .into_iter()
                    .flatten()
                    .filter(|export| export.name == segment && export.target_module.is_some()),
            ) else {
                valid = false;
                break;
            };
            current = module.target_module.clone().expect("module export");
        }
        if valid && exports.contains_key(&current) {
            return Some(current);
        }
    }
    None
}

fn candidates<'a>(
    exports: &'a ExportTable,
    prefix: &[String],
    path: &[String],
) -> Vec<&'a ExternalModuleExport> {
    let Some(name) = path.last() else {
        return Vec::new();
    };
    let Some(module) = resolve_module_path(exports, prefix, &path[..path.len() - 1]) else {
        return Vec::new();
    };
    exports
        .get(&module)
        .into_iter()
        .flatten()
        .filter(|export| export.name == *name)
        .collect()
}

fn unique<'a>(
    mut values: impl Iterator<Item = &'a ExternalModuleExport>,
) -> Option<&'a ExternalModuleExport> {
    let first = values.next()?;
    values
        .all(|value| same_declaration(first, value))
        .then_some(first)
}

fn same_declaration(left: &ExternalModuleExport, right: &ExternalModuleExport) -> bool {
    left.span == right.span && left.kind == right.kind && left.target_module == right.target_module
}

pub fn resolve_export<'a>(
    exports: &'a ExportTable,
    prefix: &[String],
    path: &[String],
) -> Option<&'a ExternalModuleExport> {
    unique(candidates(exports, prefix, path).into_iter())
}

struct Reexport<'a> {
    module: Vec<String>,
    import: &'a UseImport,
    trusted: bool,
}

fn collect<'a>(
    statements: &'a [Stmt],
    prefix: &[String],
    program: &Program,
    edges: &mut Vec<Reexport<'a>>,
) {
    for statement in statements {
        match statement {
            Stmt::Use {
                visibility,
                imports,
                ..
            } if visibility.is_public() => {
                for import in imports {
                    edges.push(Reexport {
                        module: prefix.to_vec(),
                        import,
                        trusted: program.language_declaration_spans.iter().any(|span| {
                            span.source == import.span.source
                                && span.start <= import.span.start
                                && import.span.end <= span.end
                        }),
                    });
                }
            }
            Stmt::Module {
                name,
                statements: Some(children),
                ..
            } => {
                let mut child = prefix.to_vec();
                child.push(name.clone());
                collect(children, &child, program, edges);
            }
            _ => {}
        }
    }
}

fn imported<'a>(exports: &'a ExportTable, edge: &Reexport<'_>) -> Vec<&'a ExternalModuleExport> {
    if edge.import.kind == UseImportKind::Glob {
        return resolve_module_path(exports, &edge.module, &edge.import.path)
            .and_then(|path| exports.get(&path))
            .map(|exports| exports.iter().collect())
            .unwrap_or_default();
    }
    // Builtin source files use catalog names independently of their physical
    // package layout. Only a trusted declaration may use this lookup.
    if edge.trusted && edge.import.path.len() == 1 {
        let name = &edge.import.path[0];
        if let Some(builtin) = rils_builtins::BUILTINS
            .iter()
            .find(|builtin| builtin.path.rsplit("::").next() == Some(name))
        {
            use crate::analysis::SymbolKind;
            use rils_builtins::BuiltinKind;
            let kind = match builtin.kind {
                BuiltinKind::Function => SymbolKind::Function,
                BuiltinKind::Module => SymbolKind::Module,
                BuiltinKind::Trait => SymbolKind::Trait,
                _ => SymbolKind::Type,
            };
            return exports
                .values()
                .flatten()
                .filter(|export| export.name == *name && export.kind == kind)
                .collect();
        }
    }
    candidates(exports, &edge.module, &edge.import.path)
}

pub fn resolve_reexports<'a>(
    exports: &mut ExportTable,
    units: impl IntoIterator<Item = (&'a [String], &'a Program)>,
) -> Vec<AnalysisDiagnostic> {
    let mut edges = Vec::new();
    for (path, program) in units {
        collect(&program.statements, path, program, &mut edges);
    }
    loop {
        let mut additions = Vec::new();
        for edge in &edges {
            for original in imported(exports, edge) {
                let mut export = original.clone();
                if let Some(name) = edge.import.binding_name() {
                    export.name = name.into();
                }
                additions.push((edge.module.join("::"), export));
            }
        }
        let mut changed = false;
        for (module, export) in additions {
            let values = exports.entry(module).or_default();
            if !values
                .iter()
                .any(|value| value.name == export.name && same_declaration(value, &export))
            {
                values.push(export);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut diagnostics = Vec::new();
    for edge in edges {
        let imports = imported(exports, &edge);
        if imports.is_empty() {
            let known_empty_module = edge.import.kind == UseImportKind::Glob
                && resolve_module_path(exports, &edge.module, &edge.import.path).is_some();
            let builtin = edge.trusted
                && edge.import.path.len() == 1
                && (rils_builtins::builtin(&edge.import.path[0]).is_some()
                    || rils_builtins::BUILTIN_MODULES
                        .iter()
                        .any(|module| module.members.contains(&edge.import.path[0].as_str())));
            if !known_empty_module && !builtin {
                diagnostics.push(AnalysisDiagnostic::error(
                    format!(
                        "cannot resolve public re-export `{}` (missing, private, or cyclic import)",
                        edge.import.path.join("::")
                    ),
                    edge.import.span,
                ));
            }
        } else {
            let destination = exports
                .get(&edge.module.join("::"))
                .expect("resolved destination");
            for original in imports {
                let name = edge.import.binding_name().unwrap_or(&original.name);
                if unique(destination.iter().filter(|export| export.name == name)).is_none() {
                    diagnostics.push(AnalysisDiagnostic::error(
                        format!("ambiguous public re-export `{name}`"),
                        edge.import.span,
                    ));
                }
            }
        }
    }
    diagnostics.sort_by_key(|error| (error.span.source, error.span.start, error.span.end));
    diagnostics.dedup_by(|left, right| left.span == right.span && left.message == right.message);
    diagnostics
}
