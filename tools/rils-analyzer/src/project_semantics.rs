//! project semantics for the analyzer service.
use super::*;

impl Server {
    pub(super) fn reanalyze_documents(&mut self) {
        for (uri, document) in &self.documents {
            let capabilities = self
                .compilation
                .sources()
                .parse_capabilities(document.source_id)
                .unwrap_or_else(|| self.parse_capabilities_for_uri(uri));
            self.compilation
                .sources_mut()
                .set_source_with_id_and_capabilities(
                    document.source_id,
                    uri.clone(),
                    document.text.clone(),
                    capabilities,
                );
        }
        self.rebuild_project_semantics(None);
        let exports = project_index::collect_external_exports(self);
        let analyses = self
            .documents
            .values()
            .map(|document| {
                let external_exports = self
                    .project_for_source(document.source_id)
                    .map(|project| exports.for_project(self, project, true))
                    .unwrap_or_default();
                (
                    document.source_id,
                    self.analyze_source(document.source_id, &external_exports),
                )
            })
            .collect::<HashMap<_, _>>();
        for document in self.documents.values_mut() {
            document.analysis = analyses
                .get(&document.source_id)
                .cloned()
                .expect("analysis collected for every document");
        }
        self.rebuild_project_semantics(Some(&exports));
    }

    pub(super) fn rebuild_project_semantics(
        &mut self,
        inherited_exports: Option<&project_index::ProjectExportIndex>,
    ) {
        self.compilation.clear_projects();
        for project in &self.projects {
            self.compilation
                .register_project(project_session_name(project));
        }
        let standard_library = self
            .projects
            .iter()
            .find(|project| {
                project.origin() == ProjectOrigin::Language(LanguagePackageKind::StandardLibrary)
            })
            .and_then(|project| self.compilation.project_id(&project_session_name(project)));
        for project in &self.projects {
            let project_id = self
                .compilation
                .register_project(project_session_name(project));
            let mut index = ProjectSemanticIndex::default();
            for dependency in project.language_dependencies() {
                match dependency {
                    LanguagePackageKind::StandardLibrary => {
                        if let Some(dependency) = standard_library {
                            index.add_dependency(dependency);
                        }
                    }
                }
            }
            let mut programs = Vec::new();
            for file in project.modules() {
                if let Some(document) = self.documents.get(&path_to_file_uri(&file.path)) {
                    let module = index.register(&file.module_path, document.source_id);
                    if let Some(program) =
                        self.parse_source(document.source_id).ok().or_else(|| {
                            self.compilation
                                .sources()
                                .last_valid_parse(document.source_id)
                        })
                    {
                        programs.push((module, program));
                    }
                }
            }
            let prelude_program = project.prelude().and_then(|path| {
                self.documents
                    .get(&path_to_file_uri(path))
                    .and_then(|document| {
                        self.parse_source(document.source_id).ok().or_else(|| {
                            self.compilation
                                .sources()
                                .last_valid_parse(document.source_id)
                        })
                    })
            });
            self.compilation.replace_project(project_id, index);
            let syntax = self
                .compilation
                .project_syntax_mut(project_id)
                .expect("registered project must have syntax storage");
            if let Some(program) = prelude_program {
                syntax.push_root(program);
            }
            for (module, program) in programs {
                syntax.insert_module(module, program);
            }

            let analysis = {
                let semantics = self
                    .compilation
                    .project(project_id)
                    .expect("registered project must have semantic storage");
                let syntax = self
                    .compilation
                    .project_syntax(project_id)
                    .expect("registered project must have syntax storage");
                let language_dependency = project
                    .language_dependencies()
                    .any(|dependency| dependency == LanguagePackageKind::StandardLibrary);
                if language_dependency {
                    if let Some(exports) = inherited_exports {
                        let language_exports = exports.dependencies_for_project(self, project);
                        rils_frontend::analyze_project_with_host_and_external_exports(
                            syntax,
                            semantics.module_graph(),
                            &self.host_contract,
                            &language_exports,
                        )
                    } else {
                        rils_frontend::analyze_project_with_host(
                            syntax,
                            semantics.module_graph(),
                            &self.host_contract,
                        )
                    }
                } else {
                    rils_frontend::analyze_project_with_host(
                        syntax,
                        semantics.module_graph(),
                        &self.host_contract,
                    )
                }
            };
            self.compilation
                .project_mut(project_id)
                .expect("registered project must have semantic storage")
                .index_def_map(&analysis.def_map);
            self.compilation
                .set_project_analysis(project_id, &self.host_contract, analysis);
        }
    }

    pub(super) fn project_semantics(&self, project: &Project) -> Option<&ProjectSemanticIndex> {
        self.compilation
            .project_id(&project_session_name(project))
            .and_then(|id| self.compilation.project(id))
    }

    pub(super) fn project_analysis(&self, project: &Project) -> Option<&DocumentAnalysis> {
        self.compilation
            .project_id(&project_session_name(project))
            .and_then(|id| self.compilation.project_analysis(id, &self.host_contract))
    }

    pub(super) fn document_uri_for_source(&self, source: SourceId) -> Option<&str> {
        self.documents
            .iter()
            .find(|(_, document)| document.source_id == source)
            .map(|(uri, _)| uri.as_str())
    }

    pub(super) fn project_definition_by_id(
        &self,
        id: rils_frontend::DefId,
    ) -> Option<&DefinitionData> {
        if let Some(definition) = self.projects.iter().find_map(|project| {
            self.project_analysis(project)
                .and_then(|analysis| analysis.def_map.definition(id))
        }) {
            return Some(definition);
        }
        self.compilation
            .projects()
            .find_map(|index| index.definition(id))
    }

    pub(super) fn refresh_project_symbol_links(&mut self) {
        let mut links = Vec::new();
        for (uri, document) in &self.documents {
            let Some(document_analysis) = analysis(document) else {
                continue;
            };
            for (index, symbol) in document_analysis.symbols.iter().enumerate() {
                if symbol.is_definition || symbol.definition_id.is_some() {
                    continue;
                }
                if let Some(target) =
                    self.project_symbol_id(uri, document, symbol.span.start, symbol.kind)
                {
                    links.push((uri.clone(), index, target));
                }
            }
        }
        for (uri, index, target) in links {
            let Some(Ok(analysis)) = self
                .documents
                .get_mut(&uri)
                .map(|document| &mut document.analysis)
            else {
                continue;
            };
            if let Some(symbol) = analysis.symbols.get_mut(index) {
                symbol.definition_id = Some(target);
            }
        }
    }
}
