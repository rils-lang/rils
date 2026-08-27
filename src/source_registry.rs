use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rils_frontend::{ProjectSemanticIndex, SourceDatabase};

use crate::{Project, RilsError, SourceFile, SourceId, ast, lexer, macros, parser};

#[derive(Default)]
pub(crate) struct SourceRegistry {
    by_path: HashMap<PathBuf, SourceId>,
    database: SourceDatabase,
    modules: ProjectSemanticIndex,
}

impl SourceRegistry {
    pub(crate) fn register_project(&mut self, project: &Project) {
        for file in project.modules() {
            let source = self.register_path(&file.path);
            self.modules.register(&file.module_path, source);
        }
    }

    fn register_path(&mut self, path: &Path) -> SourceId {
        let key = source_path_key(path);
        if let Some(id) = self.by_path.get(&key) {
            return *id;
        }
        let name = path.to_string_lossy().into_owned();
        let id = self.database.reserve(name);
        self.by_path.insert(key, id);
        id
    }

    pub(crate) fn register_source(&mut self, path: &Path, source: &str) -> SourceId {
        let id = self.register_path(path);
        let name = self
            .database
            .source_file(id)
            .expect("registered source has metadata")
            .name
            .clone();
        self.database.set_source_with_id(id, name, source);
        id
    }

    pub(crate) fn source_id(&self, path: &Path) -> Option<SourceId> {
        self.by_path.get(&source_path_key(path)).copied()
    }

    pub(crate) fn module_path(&self, source: SourceId) -> Option<&str> {
        self.modules
            .module(source)
            .map(|module| module.path.as_str())
    }

    pub(crate) fn source_files(&self) -> Vec<SourceFile> {
        self.database.source_files()
    }

    pub(crate) fn parse(
        &self,
        id: SourceId,
        native_macros: &[macros::NativeMacroDefinition],
    ) -> Result<ast::Program, RilsError> {
        if native_macros == macros::STANDARD_NATIVE_MACROS {
            return self.database.parse(id).map_err(Into::into);
        }
        let source = self
            .database
            .source_text(id)
            .expect("source must be registered before parsing");
        let tokens = lexer::lex_with_source_id(source, id).map_err(RilsError::Lex)?;
        parser::parse_with_native_macros(tokens, native_macros).map_err(RilsError::Parse)
    }

    pub(crate) fn location(&self, id: SourceId) -> Option<(&str, &str)> {
        let file = self.database.source_file(id)?;
        let source = self.database.source_text(id)?;
        Some((&file.name, source))
    }
}

fn source_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
