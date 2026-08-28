use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rils_frontend::{CompilationSession, ProjectId};

use crate::{Project, RilsError, SourceId, ast, lexer, macros, parser};

#[derive(Default)]
pub(crate) struct ProjectCompilation {
    by_path: HashMap<PathBuf, SourceId>,
    session: CompilationSession,
    project: Option<ProjectId>,
}

impl ProjectCompilation {
    pub(crate) fn register_project(&mut self, project: &Project) {
        let project_id = self.session.register_project(project.name());
        self.project = Some(project_id);
        for file in project.modules() {
            let source = self.register_path(&file.path);
            self.session
                .project_mut(project_id)
                .expect("registered project has semantic state")
                .register(&file.module_path, source);
        }
    }

    fn register_path(&mut self, path: &Path) -> SourceId {
        let key = source_path_key(path);
        if let Some(id) = self.by_path.get(&key) {
            return *id;
        }
        let name = path.to_string_lossy().into_owned();
        let id = self.session.sources_mut().reserve(name);
        self.by_path.insert(key, id);
        id
    }

    pub(crate) fn register_source(&mut self, path: &Path, source: &str) -> SourceId {
        let id = self.register_path(path);
        let name = self
            .session
            .sources()
            .source_file(id)
            .expect("registered source has metadata")
            .name
            .clone();
        self.session
            .sources_mut()
            .set_source_with_id(id, name, source);
        id
    }

    pub(crate) fn source_id(&self, path: &Path) -> Option<SourceId> {
        self.by_path.get(&source_path_key(path)).copied()
    }

    pub(crate) fn module_path(&self, source: SourceId) -> Option<&str> {
        self.session
            .project(self.project?)?
            .module(source)
            .map(|module| module.path.as_str())
    }

    pub(crate) fn session(&self) -> &CompilationSession {
        &self.session
    }

    pub(crate) fn project_id(&self) -> ProjectId {
        self.project
            .expect("project compilation must register a project before compiling")
    }

    pub(crate) fn set_entry_source(&mut self, source: SourceId) {
        let project = self.project_id();
        self.session
            .project_mut(project)
            .expect("registered project has semantic state")
            .set_entry_source(source);
    }

    pub(crate) fn push_root_program(&mut self, program: ast::Program) {
        let project = self.project_id();
        self.session
            .project_syntax_mut(project)
            .expect("registered project has syntax state")
            .push_root(program);
    }

    pub(crate) fn set_module_program(&mut self, source: SourceId, program: ast::Program) {
        let project = self.project_id();
        let module = self
            .session
            .project(project)
            .and_then(|semantics| semantics.module(source))
            .expect("registered project source has a module identity")
            .id;
        self.session
            .project_syntax_mut(project)
            .expect("registered project has syntax state")
            .insert_module(module, program);
    }

    pub(crate) fn interpreter_program(&self) -> ast::Program {
        let project = self.project_id();
        let semantics = self
            .session
            .project(project)
            .expect("registered project has semantic state");
        self.session
            .project_syntax(project)
            .expect("registered project has syntax state")
            .inline_module_compatibility_program(semantics.module_graph())
    }

    pub(crate) fn parse(
        &self,
        id: SourceId,
        native_macros: &[macros::NativeMacroDefinition],
    ) -> Result<ast::Program, RilsError> {
        if native_macros == macros::STANDARD_NATIVE_MACROS {
            return self.session.sources().parse(id).map_err(Into::into);
        }
        let source = self
            .session
            .sources()
            .source_text(id)
            .expect("source must be registered before parsing");
        let tokens = lexer::lex_with_source_id(source, id).map_err(RilsError::Lex)?;
        parser::parse_with_native_macros(tokens, native_macros).map_err(RilsError::Parse)
    }

    pub(crate) fn location(&self, id: SourceId) -> Option<(&str, &str)> {
        let file = self.session.sources().source_file(id)?;
        let source = self.session.sources().source_text(id)?;
        Some((&file.name, source))
    }
}

fn source_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
