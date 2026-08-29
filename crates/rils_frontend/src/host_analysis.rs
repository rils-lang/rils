use std::collections::HashMap;

use rils_host::HostContract;

use crate::{
    FrontendError, SourceId,
    analysis::{DocumentAnalysis, ExternalModuleExport},
    ast::Program,
};

pub fn analyze_with_host(
    source: &str,
    host: &HostContract,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex(source).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    Ok(analyze_program_with_host_contract(
        &program,
        SourceId::UNKNOWN,
        &signatures,
        &host_types,
        &HashMap::new(),
        &[],
        host,
    ))
}

pub fn analyze_with_host_and_source_id_and_external_exports(
    source: &str,
    source_id: SourceId,
    host: &HostContract,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex_with_source_id(source, source_id).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    Ok(
        analyze_program_with_host_and_source_id_and_external_exports(
            &program,
            source_id,
            host,
            external_exports,
        ),
    )
}

pub fn analyze_program_with_host_and_source_id_and_external_exports(
    program: &Program,
    source_id: SourceId,
    host: &HostContract,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> DocumentAnalysis {
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    analyze_program_with_host_contract(
        program,
        source_id,
        &signatures,
        &host_types,
        external_exports,
        &[],
        host,
    )
}

fn analyze_program_with_host_contract(
    program: &Program,
    source_id: SourceId,
    host_functions: &HashMap<String, crate::FunctionSignature>,
    host_types: &std::collections::HashSet<String>,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
    module_path: &[String],
    host: &HostContract,
) -> DocumentAnalysis {
    crate::analysis::analyze_program_in_module_with_external_exports_and_host_types(
        program,
        source_id,
        host_functions,
        host_types,
        external_exports,
        module_path,
        Some(host),
    )
}
