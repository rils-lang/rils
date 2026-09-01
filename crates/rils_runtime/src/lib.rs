mod environment {
    pub(crate) use rils_execution::environment::*;
}
mod engine;
mod error;
mod formatting {
    pub(crate) use rils_execution::formatting::*;
}
mod hash_collections {
    pub(crate) use rils_execution::hash_collections::*;
}
mod interpreter;
mod native_type;
mod numeric {
    pub(crate) use rils_execution::numeric::*;
}
mod output {
    pub(crate) use rils_execution::output::default_output_handler;
}
mod runtime_builtins {
    pub(crate) use rils_execution::runtime_builtins::*;
}
mod standard_library {
    pub(crate) use rils_execution::standard_library::*;
}
mod value {
    pub(crate) use rils_execution::value::*;
}

pub mod analysis {
    pub use rils_frontend::analysis::{
        AnalysisDiagnostic, DiagnosticSeverity, DocumentAnalysis, InlayTypeHint, SymbolKind,
        SymbolOccurrence,
    };
    pub use rils_frontend::semantic::{
        BuiltinCallKind, DefMap, DefinitionData, ResolvedCall, TypeckResults,
    };

    pub fn analyze(source: &str) -> Result<DocumentAnalysis, crate::RilsError> {
        rils_frontend::analysis::analyze(source).map_err(Into::into)
    }

    pub fn analyze_with_host(
        source: &str,
        host: &crate::HostContract,
    ) -> Result<DocumentAnalysis, crate::RilsError> {
        rils_frontend::analyze_with_host(source, host).map_err(Into::into)
    }
}

mod ast {
    pub(crate) use rils_frontend::ast::*;
}
mod lexer {
    pub(crate) use rils_frontend::lexer::*;
}
mod macros {
    pub(crate) use rils_frontend::macros::*;
}
mod opaque_host;
mod parser {
    pub(crate) use rils_frontend::parser::*;
}
mod source {
    pub(crate) use rils_frontend::source::*;
}
mod token {
    pub(crate) use rils_frontend::token::*;
}
mod types {
    pub(crate) use rils_frontend::types::*;
}

pub(crate) use engine::is_identifier;
pub use engine::{Engine, eval};
pub use error::RilsError;
pub use native_type::{NativeFunctionHandler, NativeTypeHandle};
pub use opaque_host::{
    InlineHostValue, OpaqueHostHandle, host_enum_raw, host_enum_value, inline_host_value,
    inline_host_value_typed, opaque_host_handle, opaque_host_value, opaque_host_value_typed,
};
pub use rils_execution::ExecutionLimits;
pub use rils_execution::Value;
pub use rils_execution::{HostFormatKind, HostFormatSpec, HostValueFormatter, OutputHandler};
pub use rils_frontend::{
    FloatType, FrontendError, FunctionSignature, IntegerType, RuntimeValue, SourceFile, SourceId,
    Span, Type,
};
pub use rils_host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_MANIFEST_FORMAT_VERSION,
    HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES,
    HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HOST_MANIFEST_MAX_TYPES, HostCallKind,
    HostContract, HostEnumDefinition, HostFunctionDeclaration, HostModuleDeclaration, HostReceiver,
    HostThreadAffinity, HostTypeDeclaration, HostTypeTransport, HostValueLayout,
};
pub use rils_project::{Project, ProjectDependency, ProjectError, ProjectFile, ProjectKind};

#[macro_export]
macro_rules! rils_forward_macro {
    ($engine:expr, $name:ident, $min_arity:expr, $max_arity:expr, $function:expr $(,)?) => {{
        $engine.register_native_macro(
            stringify!($name),
            concat!("#rils_native_macro_", stringify!($name)),
            $min_arity,
            $max_arity,
            $function,
        )
    }};
}
