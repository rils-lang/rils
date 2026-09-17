//! Document analysis with project namespace and last-valid-syntax recovery.
use super::*;

impl Server {
    /// Analyze the current revision, falling back to the last valid syntax
    /// tree while an editor is in the middle of an invalid edit. The fallback
    /// is deliberately kept in `SourceDatabase` and never used by compiler
    /// entry points, so diagnostics still report the current syntax error.
    pub(crate) fn analyze_source(
        &self,
        source_id: SourceId,
        external_exports: &HashMap<String, Vec<rils_frontend::analysis::ExternalModuleExport>>,
    ) -> Result<DocumentAnalysis, FrontendError> {
        let module_path = self
            .compilation
            .sources()
            .source_file(source_id)
            .and_then(|file| file_uri_to_path(&file.name))
            .and_then(|path| self.project_for_source(source_id)?.module_for_file(&path))
            .map(|file| {
                file.module_path
                    .split("::")
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        match self.parse_source(source_id) {
            Ok(program) => Ok(rils_frontend::analyze_module_with_host(
                &program,
                source_id,
                &module_path,
                &self.host_contract,
                external_exports,
            )),
            Err(error) => {
                let Some(program) = self.compilation.sources().last_valid_parse(source_id) else {
                    return Err(error);
                };
                let mut analysis = rils_frontend::analyze_module_with_host(
                    &program,
                    source_id,
                    &module_path,
                    &self.host_contract,
                    external_exports,
                );
                analysis
                    .diagnostics
                    .push(AnalysisDiagnostic::error(error.to_string(), error.span()));
                Ok(analysis)
            }
        }
    }
}
