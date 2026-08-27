use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rils_frontend::{ModuleData, ModuleGraph, ModuleId, SourceId};
use rils_project::Project;

use crate::{Document, path_to_file_uri};

/// Cross-file identities and module relationships for one configured project.
pub(super) struct ProjectSemanticIndex {
    graph: ModuleGraph,
    files: HashMap<ModuleId, PathBuf>,
}

impl ProjectSemanticIndex {
    pub(super) fn build(project: &Project, documents: &HashMap<String, Document>) -> Self {
        let mut graph = ModuleGraph::default();
        let mut files = HashMap::new();
        for file in project.modules() {
            let Some(document) = documents.get(&path_to_file_uri(&file.path)) else {
                continue;
            };
            let module = graph.register(&file.module_path, document.source_id);
            files.insert(module, file.path.clone());
        }
        Self { graph, files }
    }

    pub(super) fn resolve(&self, source: SourceId, qualifier: &str) -> Option<&ModuleData> {
        let current = self.graph.module_for_source(source)?;
        self.graph.resolve(current.id, qualifier)
    }

    pub(super) fn module(&self, source: SourceId) -> Option<&ModuleData> {
        self.graph.module_for_source(source)
    }

    pub(super) fn children(&self, module: ModuleId) -> impl Iterator<Item = &ModuleData> {
        self.graph.children(module)
    }

    pub(super) fn file(&self, module: ModuleId) -> Option<&Path> {
        self.files.get(&module).map(PathBuf::as_path)
    }
}
