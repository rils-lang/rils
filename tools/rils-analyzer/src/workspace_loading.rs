//! workspace loading for the analyzer service.
use super::*;

impl Server {
    pub(super) fn load_projects(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut seen = HashSet::new();
        self.projects.clear();
        let stdlib_root = match language_package_root() {
            Ok(root) => Some(root),
            Err(error) => {
                self.show_workspace_error(error)?;
                None
            }
        };
        if let Some(root) = stdlib_root.as_deref() {
            match Project::from_language_package(
                root.join(rils_project::PROJECT_FILE_NAME),
                LanguagePackageKind::StandardLibrary,
            ) {
                Ok(project) => {
                    seen.insert(project.root().to_path_buf());
                    self.projects.push(project);
                }
                Err(error) => {
                    self.show_workspace_error(format!("failed to load standard library: {error}"))?
                }
            }
        }
        for root in workspace_roots(initialization) {
            match workspace::workspace_projects_with_language(&root, stdlib_root.as_deref()) {
                Ok(load) => {
                    for error in load.errors {
                        self.show_workspace_error(error)?;
                    }
                    self.projects
                        .extend(load.projects.into_iter().filter_map(|mut project| {
                            if !seen.insert(project.root().to_path_buf()) {
                                return None;
                            }
                            if stdlib_root.is_some() {
                                project
                                    .add_language_dependency(LanguagePackageKind::StandardLibrary);
                            }
                            Some(project)
                        }));
                }
                Err(error) => self.show_workspace_error(format!(
                    "failed to load workspace `{}`: {error}",
                    root.display()
                ))?,
            }
        }
        self.compilation.clear_projects();
        Ok(())
    }

    pub(super) fn show_workspace_error(&self, message: String) -> Result<(), AnyError> {
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                "window/showMessage".to_owned(),
                json!({"type": 1, "message": message}),
            )))?;
        Ok(())
    }

    pub(super) fn load_workspace(&mut self) -> Result<(), AnyError> {
        let files = self
            .projects
            .iter()
            .flat_map(|project| project.modules().map(|file| file.path.clone()))
            .chain(
                self.projects
                    .iter()
                    .filter_map(|project| project.prelude().map(Path::to_path_buf)),
            )
            .collect::<HashSet<_>>();
        for path in files {
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    self.show_workspace_error(format!(
                        "failed to read source `{}`: {error}",
                        path.display()
                    ))?;
                    continue;
                }
            };
            let uri = path_to_file_uri(&path);
            let source_id = match self.source_id_for_uri(&uri) {
                Ok(source_id) => source_id,
                Err(error) => {
                    self.show_workspace_error(error.to_string())?;
                    break;
                }
            };
            let capabilities = self.parse_capabilities_for_uri(&uri);
            self.compilation
                .sources_mut()
                .set_source_with_id_and_capabilities(
                    source_id,
                    uri.clone(),
                    text.clone(),
                    capabilities,
                );
            self.workspace_documents.insert(uri.clone());
            self.documents.insert(
                uri,
                Document {
                    source_id,
                    analysis: self.analyze_source(source_id, &HashMap::new()),
                    text,
                },
            );
        }
        self.reanalyze_documents();
        self.refresh_project_symbol_links();
        Ok(())
    }
}
