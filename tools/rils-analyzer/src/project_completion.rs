//! Module completion consumes the same export graph as name analysis.
use super::*;

impl Server {
    pub(crate) fn add_project_completions(
        &self,
        uri: &str,
        qualifier: &str,
        member_prefix: &str,
        module_names: &mut HashSet<String>,
        items: &mut Vec<Value>,
    ) {
        let Some(document) = self.documents.get(uri) else {
            return;
        };
        let Some(project) = self.project_for_source(document.source_id) else {
            return;
        };
        let exports =
            project_index::collect_external_exports(self).for_project(self, project, true);
        let current = file_uri_to_path(uri)
            .and_then(|path| project.module_for_file(&path))
            .map(|file| {
                file.module_path
                    .split("::")
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let path = qualifier
            .split("::")
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let Some(module) = rils_frontend::exports::resolve_module_path(&exports, &current, &path)
        else {
            return;
        };
        for export in exports.get(&module).into_iter().flatten() {
            if !export.name.starts_with(member_prefix) {
                continue;
            }
            let mut item_path = module
                .split("::")
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            item_path.insert(0, "crate".into());
            item_path.push(export.name.clone());
            if rils_frontend::exports::resolve_export(&exports, &[], &item_path).is_none() {
                continue;
            }
            let kind = match export.kind {
                SymbolKind::Module => {
                    if !module_names.insert(export.name.clone()) {
                        continue;
                    }
                    9
                }
                SymbolKind::Function => 3,
                SymbolKind::Trait => 8,
                SymbolKind::Type => 22,
                _ => 6,
            };
            items.push(json!({
                "label": export.name, "filterText": export.name, "insertText": export.name,
                "kind": kind, "detail": export.detail,
                "sortText": format!("1_{}", export.name),
            }));
        }
    }
}
