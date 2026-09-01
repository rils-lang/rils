//! Bytecode compiler, verified format, virtual machine, and bytecode host for Rils.

use rils_execution::{
    ExecutionLimits, FloatType, HostFormatKind, HostFormatSpec, HostValueFormatter, IntegerType,
    OutputHandler,
};

#[cfg(test)]
use rils_runtime::{Value, eval};

mod ast {
    pub(crate) use rils_frontend::ast::*;
}
mod environment {
    pub(crate) use rils_execution::environment::*;
}
mod formatting {
    pub(crate) use rils_execution::formatting::*;
}
mod hash_collections {
    pub(crate) use rils_execution::hash_collections::*;
}
mod hir {
    pub(crate) use rils_compiler::hir::*;
}
mod lexer {
    #[allow(unused_imports)]
    pub(crate) use rils_frontend::lexer::*;
}
mod macros {
    pub(crate) use rils_frontend::macros::*;
}
mod mir {
    pub(crate) use rils_compiler::mir::*;
}
mod numeric {
    pub(crate) use rils_execution::numeric::*;
}
mod output {
    pub(crate) use rils_execution::output::*;
}
mod parser {
    #[allow(unused_imports)]
    pub(crate) use rils_frontend::parser::*;
}
mod runtime_builtins {
    pub(crate) use rils_execution::runtime_builtins::*;
}
mod source {
    pub(crate) use rils_frontend::source::*;
}
mod standard_library {
    pub(crate) use rils_execution::standard_library::*;
}
mod types {
    pub(crate) use rils_frontend::types::*;
}
mod value {
    pub(crate) use rils_execution::value::*;
}

mod compile;
mod image;
mod library;

/// Compatibility namespace for the pre-refactor bytecode module path.
pub mod bytecode {
    pub use crate::image::*;
}

pub use compile::{
    compile, compile_file, compile_file_with_host, compile_library, compile_with_host,
};
pub use image::{
    BYTECODE_FORMAT_VERSION, BYTECODE_HOST_ABI_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeError,
    BytecodeFormatError, BytecodeHost, BytecodeImport, BytecodeModule, CompileError,
};
pub use library::{LibraryFormatError, RilsLibrary};
pub use rils_host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_MANIFEST_FORMAT_VERSION,
    HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES,
    HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HOST_MANIFEST_MAX_TYPES, HostCallKind,
    HostContract, HostEnumDefinition, HostFunctionDeclaration, HostModuleDeclaration, HostReceiver,
    HostThreadAffinity, HostTypeDeclaration, HostTypeTransport, HostValueLayout,
};
