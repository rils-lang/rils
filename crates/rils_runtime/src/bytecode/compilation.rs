use super::encoder::encode;
use super::*;

pub fn compile(source: &str) -> Result<BytecodeModule, CompileError> {
    encode(rils_compiler::compile(source)?)
}

pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_with_host(source, host)?)
}

#[cfg(test)]
pub(crate) fn compile_program_with_host_and_sources(
    program: &crate::ast::Program,
    host: &HostContract,
    sources: Vec<SourceFile>,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_program_with_host_and_sources(
        program, host, sources,
    )?)
}

pub(crate) fn compile_program_with_host_and_session(
    host: &HostContract,
    session: &rils_frontend::CompilationSession,
    project: rils_frontend::ProjectId,
) -> Result<BytecodeModule, CompileError> {
    validate_contract_abi(host)?;
    encode(rils_compiler::compile_program_with_host_and_session(
        host, session, project,
    )?)
}

fn validate_contract_abi(host: &HostContract) -> Result<(), CompileError> {
    if host.host_abi_version() != BYTECODE_HOST_ABI_VERSION {
        return Err(CompileError::new(
            format!(
                "host contract ABI {} is incompatible with bytecode host ABI {}",
                host.host_abi_version(),
                BYTECODE_HOST_ABI_VERSION
            ),
            Span::default(),
        ));
    }
    Ok(())
}
