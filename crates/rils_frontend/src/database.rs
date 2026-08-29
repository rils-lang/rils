use std::collections::{BTreeMap, HashMap};

use crate::{
    DefId, DefMap, DefinitionData, FrontendError, ModuleId, SourceFile, SourceId,
    analysis::DocumentAnalysis, ast::Program,
};

#[derive(Clone, Debug)]
struct ProjectAnalysisState {
    host_contract_hash: String,
    analysis: DocumentAnalysis,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectId(u32);

impl ProjectId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CompilationSession {
    sources: SourceDatabase,
    next_project_id: u32,
    projects_by_name: HashMap<String, ProjectId>,
    projects: BTreeMap<ProjectId, ProjectSemanticIndex>,
    project_syntax: BTreeMap<ProjectId, ProjectSyntax>,
    project_analyses: BTreeMap<ProjectId, ProjectAnalysisState>,
}

impl CompilationSession {
    pub fn sources(&self) -> &SourceDatabase {
        &self.sources
    }

    pub fn sources_mut(&mut self) -> &mut SourceDatabase {
        self.project_analyses.clear();
        &mut self.sources
    }

    pub fn register_project(&mut self, name: impl Into<String>) -> ProjectId {
        let name = name.into();
        if let Some(id) = self.projects_by_name.get(&name) {
            return *id;
        }
        self.next_project_id = self
            .next_project_id
            .checked_add(1)
            .expect("project ID overflow");
        let id = ProjectId::new(self.next_project_id);
        self.projects_by_name.insert(name, id);
        self.projects.insert(id, ProjectSemanticIndex::default());
        self.project_syntax.insert(id, ProjectSyntax::default());
        id
    }

    pub fn project_id(&self, name: &str) -> Option<ProjectId> {
        self.projects_by_name.get(name).copied()
    }

    pub fn project(&self, id: ProjectId) -> Option<&ProjectSemanticIndex> {
        self.projects.get(&id)
    }

    pub fn project_mut(&mut self, id: ProjectId) -> Option<&mut ProjectSemanticIndex> {
        self.project_analyses.remove(&id);
        self.projects.get_mut(&id)
    }

    pub fn replace_project(&mut self, id: ProjectId, semantics: ProjectSemanticIndex) {
        let slot = self
            .projects
            .get_mut(&id)
            .expect("project must be registered before replacing its semantics");
        *slot = semantics;
        self.project_analyses.remove(&id);
    }

    pub fn project_syntax(&self, id: ProjectId) -> Option<&ProjectSyntax> {
        self.project_syntax.get(&id)
    }

    pub fn project_syntax_mut(&mut self, id: ProjectId) -> Option<&mut ProjectSyntax> {
        self.project_analyses.remove(&id);
        self.project_syntax.get_mut(&id)
    }

    pub fn set_project_analysis(
        &mut self,
        id: ProjectId,
        host: &rils_host::HostContract,
        analysis: DocumentAnalysis,
    ) {
        assert!(
            self.projects.contains_key(&id),
            "project must be registered before storing analysis"
        );
        self.project_analyses.insert(
            id,
            ProjectAnalysisState {
                host_contract_hash: host.contract_hash(),
                analysis,
            },
        );
    }

    pub fn project_analysis(
        &self,
        id: ProjectId,
        host: &rils_host::HostContract,
    ) -> Option<&DocumentAnalysis> {
        let state = self.project_analyses.get(&id)?;
        (state.host_contract_hash == host.contract_hash()).then_some(&state.analysis)
    }

    pub fn clear_projects(&mut self) {
        self.projects_by_name.clear();
        self.projects.clear();
        self.project_syntax.clear();
        self.project_analyses.clear();
    }

    pub fn projects(&self) -> impl Iterator<Item = &ProjectSemanticIndex> {
        self.projects.values()
    }
}

/// Parsed syntax units belonging to one project.
///
/// Module programs retain their file and module identities. The inline-module
/// compatibility view exists only for the reference AST interpreter while it
/// migrates to project semantic identities.
#[derive(Clone, Debug, Default)]
pub struct ProjectSyntax {
    roots: Vec<Program>,
    modules: BTreeMap<ModuleId, Program>,
}

impl ProjectSyntax {
    pub fn push_root(&mut self, program: Program) {
        self.roots.push(program);
    }

    pub fn insert_module(&mut self, module: ModuleId, program: Program) {
        self.modules.insert(module, program);
    }

    pub fn roots(&self) -> impl ExactSizeIterator<Item = &Program> {
        self.roots.iter()
    }

    pub fn roots_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Program> {
        self.roots.iter_mut()
    }

    pub fn module(&self, module: ModuleId) -> Option<&Program> {
        self.modules.get(&module)
    }

    pub fn modules(&self) -> impl ExactSizeIterator<Item = (ModuleId, &Program)> {
        self.modules.iter().map(|(id, program)| (*id, program))
    }

    pub fn modules_mut(&mut self) -> impl ExactSizeIterator<Item = (ModuleId, &mut Program)> {
        self.modules.iter_mut().map(|(id, program)| (*id, program))
    }

    pub fn root_program(&self) -> Program {
        let mut program = Program {
            statements: Vec::new(),
            type_references: Vec::new(),
            macros: Vec::new(),
        };
        for root in &self.roots {
            program.statements.extend(root.statements.clone());
            program.type_references.extend(root.type_references.clone());
            program.macros.extend(root.macros.clone());
        }
        program
    }
}

#[derive(Clone, Debug)]
struct SourceEntry {
    file: SourceFile,
    text: String,
    initialized: bool,
    revision: u64,
    parsed: Result<Program, FrontendError>,
}

/// In-memory source storage shared by compilers and editor tooling.
///
/// A source keeps its identity when its text changes. Parsing happens once per
/// revision and later consumers clone the cached syntax tree instead of
/// invoking the lexer and parser independently.
#[derive(Clone, Debug, Default)]
pub struct SourceDatabase {
    next_id: u32,
    by_name: HashMap<String, SourceId>,
    sources: BTreeMap<SourceId, SourceEntry>,
}

impl SourceDatabase {
    pub fn reserve(&mut self, name: impl Into<String>) -> SourceId {
        let name = name.into();
        if let Some(id) = self.by_name.get(&name) {
            return *id;
        }
        let id = self.allocate_id();
        self.insert(id, name, String::new(), false);
        id
    }

    pub fn set_source(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        let name = name.into();
        let text = text.into();
        if let Some(id) = self.by_name.get(&name).copied() {
            self.update(id, text);
            return id;
        }
        let id = self.allocate_id();
        self.insert(id, name, text, true);
        id
    }

    pub fn set_source_with_id(
        &mut self,
        id: SourceId,
        name: impl Into<String>,
        text: impl Into<String>,
    ) {
        assert!(id != SourceId::UNKNOWN, "source database IDs must be known");
        let name = name.into();
        let text = text.into();
        self.next_id = self.next_id.max(id.0);
        if let Some(existing) = self.sources.get(&id) {
            assert_eq!(
                existing.file.name, name,
                "source ID reused for another name"
            );
            self.update(id, text);
            return;
        }
        assert!(
            !self.by_name.contains_key(&name),
            "source name reused with another ID"
        );
        self.insert(id, name, text, true);
    }

    pub fn source_id(&self, name: &str) -> Option<SourceId> {
        self.by_name.get(name).copied()
    }

    pub fn source_file(&self, id: SourceId) -> Option<&SourceFile> {
        self.sources.get(&id).map(|entry| &entry.file)
    }

    pub fn source_text(&self, id: SourceId) -> Option<&str> {
        self.sources.get(&id).map(|entry| entry.text.as_str())
    }

    pub fn revision(&self, id: SourceId) -> Option<u64> {
        self.sources.get(&id).map(|entry| entry.revision)
    }

    pub fn parse(&self, id: SourceId) -> Result<Program, FrontendError> {
        self.try_parse(id)
            .expect("source must be registered before parsing")
    }

    pub fn try_parse(&self, id: SourceId) -> Option<Result<Program, FrontendError>> {
        self.sources.get(&id).map(|entry| entry.parsed.clone())
    }

    pub fn source_files(&self) -> Vec<SourceFile> {
        self.sources
            .values()
            .map(|entry| entry.file.clone())
            .collect()
    }

    fn allocate_id(&mut self) -> SourceId {
        self.next_id = self.next_id.checked_add(1).expect("source ID overflow");
        SourceId::new(self.next_id)
    }

    fn insert(&mut self, id: SourceId, name: String, text: String, initialized: bool) {
        let parsed = parse_source(&text, id);
        self.by_name.insert(name.clone(), id);
        self.sources.insert(
            id,
            SourceEntry {
                file: SourceFile { id, name },
                text,
                initialized,
                revision: 0,
                parsed,
            },
        );
    }

    fn update(&mut self, id: SourceId, text: String) {
        let entry = self.sources.get_mut(&id).expect("registered source");
        if entry.initialized && entry.text == text {
            return;
        }
        entry.text = text;
        if entry.initialized {
            entry.revision = entry
                .revision
                .checked_add(1)
                .expect("source revision overflow");
        }
        entry.initialized = true;
        entry.parsed = parse_source(&entry.text, id);
    }
}

fn parse_source(source: &str, source_id: SourceId) -> Result<Program, FrontendError> {
    let tokens = crate::lexer::lex_with_source_id(source, source_id).map_err(FrontendError::Lex)?;
    crate::parser::parse(tokens).map_err(FrontendError::Parse)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleData {
    pub id: ModuleId,
    pub path: String,
    pub parent: Option<ModuleId>,
    pub source: Option<SourceId>,
}

/// Stable module identities and parent relationships for one project session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<ModuleData>,
    by_path: HashMap<String, ModuleId>,
    by_source: HashMap<SourceId, ModuleId>,
}

/// Module identities shared by every consumer of one project's sources.
///
/// File discovery remains outside the frontend. Callers register the module
/// paths and stable source identities supplied by their project loader, then
/// use this index for path resolution instead of rebuilding string maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectSemanticIndex {
    modules: ModuleGraph,
    definitions: HashMap<DefId, DefinitionData>,
    entry_source: Option<SourceId>,
}

impl ProjectSemanticIndex {
    pub fn register(&mut self, path: &str, source: SourceId) -> ModuleId {
        self.modules.register(path, source)
    }

    pub fn module(&self, source: SourceId) -> Option<&ModuleData> {
        self.modules.module_for_source(source)
    }

    pub fn resolve(&self, source: SourceId, qualifier: &str) -> Option<&ModuleData> {
        let current = self.module(source)?;
        self.modules.resolve(current.id, qualifier)
    }

    pub fn children(&self, module: ModuleId) -> impl Iterator<Item = &ModuleData> {
        self.modules.children(module)
    }

    pub fn modules(&self) -> impl ExactSizeIterator<Item = &ModuleData> {
        self.modules.modules()
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.modules
    }

    pub fn index_def_map(&mut self, def_map: &DefMap) {
        self.definitions.extend(
            def_map
                .definitions()
                .map(|definition| (definition.id, definition.clone())),
        );
    }

    pub fn definition(&self, id: DefId) -> Option<&DefinitionData> {
        self.definitions.get(&id)
    }

    pub fn set_entry_source(&mut self, source: SourceId) {
        self.entry_source = Some(source);
    }

    pub fn entry_source(&self) -> Option<SourceId> {
        self.entry_source
    }
}

impl ModuleGraph {
    pub fn register(&mut self, path: &str, source: SourceId) -> ModuleId {
        let root = self.ensure_root();
        let mut parent = Some(root);
        let mut current = String::new();
        for segment in path.split("::").filter(|segment| !segment.is_empty()) {
            if !current.is_empty() {
                current.push_str("::");
            }
            current.push_str(segment);
            let id = if let Some(id) = self.by_path.get(&current).copied() {
                id
            } else {
                let id = self.next_id();
                self.by_path.insert(current.clone(), id);
                self.modules.push(ModuleData {
                    id,
                    path: current.clone(),
                    parent,
                    source: None,
                });
                id
            };
            parent = Some(id);
        }
        let id = if current.is_empty() {
            root
        } else {
            parent.expect("non-empty module has a final segment")
        };
        if let Some(previous) = self.modules[id.0 as usize].source.replace(source) {
            self.by_source.remove(&previous);
        }
        if let Some(previous) = self.by_source.insert(source, id) {
            assert_eq!(previous, id, "source registered for multiple modules");
        }
        id
    }

    pub fn module(&self, id: ModuleId) -> Option<&ModuleData> {
        self.modules.get(id.0 as usize)
    }

    pub fn module_by_path(&self, path: &str) -> Option<&ModuleData> {
        self.by_path.get(path).and_then(|id| self.module(*id))
    }

    pub fn module_for_source(&self, source: SourceId) -> Option<&ModuleData> {
        self.by_source.get(&source).and_then(|id| self.module(*id))
    }

    pub fn resolve(&self, current: ModuleId, qualifier: &str) -> Option<&ModuleData> {
        let mut segments = qualifier.split("::").filter(|segment| !segment.is_empty());
        let first = segments.next()?;
        let mut resolved = match first {
            "crate" => Vec::new(),
            "self" => module_path_segments(&self.module(current)?.path),
            "super" => {
                let mut path = module_path_segments(&self.module(current)?.path);
                path.pop()?;
                path
            }
            name => vec![name.to_owned()],
        };
        for segment in segments {
            match segment {
                "crate" | "self" => {}
                "super" => {
                    resolved.pop()?;
                }
                name => resolved.push(name.to_owned()),
            }
        }
        self.module_by_path(&resolved.join("::"))
    }

    pub fn children(&self, parent: ModuleId) -> impl Iterator<Item = &ModuleData> {
        self.modules
            .iter()
            .filter(move |module| module.parent == Some(parent))
    }

    pub fn modules(&self) -> impl ExactSizeIterator<Item = &ModuleData> {
        self.modules.iter()
    }

    fn next_id(&self) -> ModuleId {
        ModuleId::new(self.modules.len().try_into().expect("module ID overflow"))
    }

    fn ensure_root(&mut self) -> ModuleId {
        if let Some(id) = self.by_path.get("").copied() {
            return id;
        }
        let id = self.next_id();
        self.by_path.insert(String::new(), id);
        self.modules.push(ModuleData {
            id,
            path: String::new(),
            parent: None,
            source: None,
        });
        id
    }
}

fn module_path_segments(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/database.rs"]
mod tests;
