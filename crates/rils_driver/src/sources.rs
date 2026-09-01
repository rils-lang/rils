use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rils_frontend::{
    CompilationSession, FrontendError, ProjectId, SourceId,
    ast::Program,
    lexer,
    macros::{self, NativeMacroDefinition},
    parser,
};
use rils_host::HostContract;
use rils_project::Project;

#[derive(Default)]
pub struct ProjectSources {
    by_path: HashMap<PathBuf, SourceId>,
    session: CompilationSession,
    project: Option<ProjectId>,
}

impl ProjectSources {
    pub fn register_project(&mut self, project: &Project) {
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

    pub fn register_source(&mut self, path: &Path, source: &str) -> SourceId {
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

    pub fn source_id(&self, path: &Path) -> Option<SourceId> {
        self.by_path.get(&source_path_key(path)).copied()
    }

    pub fn session(&self) -> &CompilationSession {
        &self.session
    }

    pub fn project_id(&self) -> ProjectId {
        self.project
            .expect("project sources must register a project before use")
    }

    pub fn set_entry_source(&mut self, source: SourceId) {
        let project = self.project_id();
        self.session
            .project_mut(project)
            .expect("registered project has semantic state")
            .set_entry_source(source);
    }

    pub fn push_root_program(&mut self, program: Program) {
        let project = self.project_id();
        self.session
            .project_syntax_mut(project)
            .expect("registered project has syntax state")
            .push_root(program);
    }

    pub fn set_module_program(&mut self, source: SourceId, program: Program) {
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

    pub fn analyze_project(&mut self, host: &HostContract) {
        let project = self.project_id();
        let analysis = {
            let semantics = self
                .session
                .project(project)
                .expect("registered project has semantic state");
            let syntax = self
                .session
                .project_syntax(project)
                .expect("registered project has syntax state");
            rils_frontend::analyze_project_with_host(syntax, semantics.module_graph(), host)
        };
        self.session.set_project_analysis(project, host, analysis);
    }

    pub fn parse(
        &self,
        id: SourceId,
        native_macros: &[NativeMacroDefinition],
    ) -> Result<Program, FrontendError> {
        if native_macros == macros::STANDARD_NATIVE_MACROS {
            return self.session.sources().parse(id);
        }
        let source = self
            .session
            .sources()
            .source_text(id)
            .expect("source must be registered before parsing");
        let tokens = lexer::lex_with_source_id(source, id).map_err(FrontendError::Lex)?;
        parser::parse_with_native_macros(tokens, native_macros).map_err(FrontendError::Parse)
    }

    pub fn location(&self, id: SourceId) -> Option<(&str, &str)> {
        let file = self.session.sources().source_file(id)?;
        let source = self.session.sources().source_text(id)?;
        Some((&file.name, source))
    }
}

fn source_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
