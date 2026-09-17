//! Project-level declarations shared by independent document analyses.

use std::{collections::HashMap, path::Path};

use rils_frontend::{
    analysis::ExternalModuleExport,
    ast::Program,
    exports::{collect_exports, resolve_reexports},
};

use crate::{Server, path_to_file_uri};
use rils_project::{Project, ProjectOrigin};

pub(super) struct ProjectExportIndex {
    by_project: HashMap<std::path::PathBuf, HashMap<String, Vec<ExternalModuleExport>>>,
}

impl ProjectExportIndex {
    pub(super) fn for_project(
        &self,
        server: &Server,
        project: &Project,
        include_dependencies: bool,
    ) -> HashMap<String, Vec<ExternalModuleExport>> {
        let mut result = self
            .by_project
            .get(project.root())
            .cloned()
            .unwrap_or_default();
        if include_dependencies {
            merge_exports(
                &mut result,
                self.dependencies_for_project(server, project).iter(),
            );
        }
        result
    }

    pub(super) fn dependencies_for_project(
        &self,
        server: &Server,
        project: &Project,
    ) -> HashMap<String, Vec<ExternalModuleExport>> {
        let mut result = HashMap::new();
        for dependency in project.language_dependencies() {
            if let Some(language_project) = server
                .projects
                .iter()
                .find(|candidate| candidate.origin() == ProjectOrigin::Language(dependency))
            {
                merge_exports(
                    &mut result,
                    self.by_project
                        .get(language_project.root())
                        .into_iter()
                        .flatten(),
                );
            }
        }
        result
    }
}

pub(super) fn collect_external_exports(server: &Server) -> ProjectExportIndex {
    let mut by_project = HashMap::new();
    for project in &server.projects {
        let mut exports = HashMap::new();
        let mut units = Vec::new();
        for project_file in project.modules() {
            let uri = path_to_file_uri(&project_file.path);
            let Some(program) = parse_project_file(server, &project_file.path) else {
                continue;
            };
            let analysis = server.project_analysis(project).or_else(|| {
                server
                    .documents
                    .get(&uri)
                    .and_then(|document| document.analysis.as_ref().ok())
            });
            let path = project_file
                .module_path
                .split("::")
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            collect_exports(&program, &path, analysis, false, &mut exports);
            units.push((path, program));
        }
        if let Some(path) = project.prelude()
            && let Some(program) = parse_project_file(server, path)
        {
            let uri = path_to_file_uri(path);
            let analysis = server.project_analysis(project).or_else(|| {
                server
                    .documents
                    .get(&uri)
                    .and_then(|document| document.analysis.as_ref().ok())
            });
            collect_exports(&program, &[], analysis, false, &mut exports);
            units.push((Vec::new(), program));
        }
        resolve_reexports(
            &mut exports,
            units
                .iter()
                .map(|(path, program)| (path.as_slice(), program)),
        );
        by_project.insert(project.root().to_path_buf(), exports);
    }
    ProjectExportIndex { by_project }
}

fn merge_exports<'a>(
    target: &mut HashMap<String, Vec<ExternalModuleExport>>,
    source: impl IntoIterator<Item = (&'a String, &'a Vec<ExternalModuleExport>)>,
) {
    for (path, exports) in source {
        target
            .entry(path.clone())
            .or_default()
            .extend(exports.iter().cloned());
    }
}

fn parse_project_file(server: &Server, path: &Path) -> Option<Program> {
    let uri = path_to_file_uri(path);
    let source = server.compilation.sources().source_id(&uri)?;
    server
        .parse_source(source)
        .ok()
        .or_else(|| server.compilation.sources().last_valid_parse(source))
}
