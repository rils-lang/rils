mod environment;
mod error;
mod formatting;
mod hash_collections;
mod interpreter;
mod limits;
mod native_type;
mod numeric;
mod output;
mod runtime_builtins;
mod runtime_type;
mod standard_library;
mod value;

#[doc(hidden)]
pub mod support {
    pub mod environment {
        pub use crate::environment::*;
    }
    pub mod formatting {
        pub use crate::formatting::*;
    }
    pub mod hash_collections {
        pub use crate::hash_collections::*;
    }
    pub mod numeric {
        pub use crate::numeric::*;
    }
    pub mod output {
        pub use crate::output::*;
    }
    pub mod runtime_builtins {
        pub use crate::runtime_builtins::*;
    }
    pub mod standard_library {
        pub use crate::standard_library::*;
    }
    pub mod value {
        pub use crate::value::*;
    }
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

use std::{fs, path::Path, rc::Rc};

use rils_driver::{DriverError, ProjectSources};

pub use error::RilsError;
pub use limits::ExecutionLimits;
pub use native_type::{NativeFunctionHandler, NativeTypeHandle};
pub use opaque_host::{
    InlineHostValue, OpaqueHostHandle, host_enum_raw, host_enum_value, inline_host_value,
    inline_host_value_typed, opaque_host_handle, opaque_host_value, opaque_host_value_typed,
};
pub use output::{HostFormatKind, HostFormatSpec, HostValueFormatter, OutputHandler};
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
pub use value::Value;

use interpreter::{Interpreter, RuntimeError};

pub struct Engine {
    interpreter: Interpreter,
    native_macros: Vec<macros::NativeMacroDefinition>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            interpreter: Interpreter::new(),
            native_macros: macros::STANDARD_NATIVE_MACROS.to_vec(),
        }
    }

    pub fn set_max_steps(&mut self, max_steps: usize) {
        self.interpreter.set_max_steps(max_steps);
    }

    pub fn set_max_call_depth(&mut self, max_call_depth: usize) {
        self.interpreter.set_max_call_depth(max_call_depth);
    }

    pub fn set_execution_limits(&mut self, limits: ExecutionLimits) {
        self.interpreter.set_execution_limits(limits);
    }

    pub fn set_output_handler<F>(&mut self, handler: F)
    where
        F: Fn(&str, bool) -> Result<(), String> + 'static,
    {
        self.interpreter.set_output_handler(Rc::new(handler));
    }

    pub fn reset_output_handler(&mut self) {
        self.interpreter
            .set_output_handler(output::default_output_handler());
    }

    pub fn set_host_value_formatter<F>(&mut self, formatter: F)
    where
        F: Fn(&Value, HostFormatSpec) -> Result<Option<String>, String> + 'static,
    {
        self.interpreter
            .set_host_value_formatter(Some(Rc::new(formatter)));
    }

    pub fn reset_host_value_formatter(&mut self) {
        self.interpreter.set_host_value_formatter(None);
    }

    pub fn register_module(&mut self, path: &str) -> Result<(), String> {
        let path = parse_module_path(path)?;
        self.interpreter.register_host_module(&path)
    }

    pub fn register_module_function<F>(
        &mut self,
        module_path: &str,
        name: &str,
        min_arity: usize,
        max_arity: usize,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        let module_path = parse_module_path(module_path)?;
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid function name"));
        }
        self.interpreter.register_host_function(
            &module_path,
            name.into(),
            min_arity,
            max_arity,
            None,
            std::rc::Rc::new(function),
        )
    }

    pub fn register_module_typed_function<F>(
        &mut self,
        module_path: &str,
        name: &str,
        parameters: Vec<Type>,
        return_type: Type,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        let module_path = parse_module_path(module_path)?;
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid function name"));
        }
        let arity = parameters.len();
        self.interpreter.register_host_function(
            &module_path,
            name.into(),
            arity,
            arity,
            Some(FunctionSignature::fixed(parameters, return_type)),
            std::rc::Rc::new(function),
        )
    }

    pub fn register_native_type(
        &mut self,
        module_path: &str,
        name: &str,
    ) -> Result<NativeTypeHandle, String> {
        let module_path = parse_module_path(module_path)?;
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid type name"));
        }
        self.interpreter
            .register_host_type(&module_path, name.into())
            .map(|definition| NativeTypeHandle { definition })
    }

    pub fn register_native_macro(
        &mut self,
        macro_name: &'static str,
        native_name: &'static str,
        min_arity: usize,
        max_arity: usize,
        function: NativeFunctionHandler,
    ) -> Result<(), String> {
        if min_arity > max_arity {
            return Err("native macro minimum arity cannot exceed its maximum arity".into());
        }
        let name_tokens = lexer::lex(macro_name).map_err(|error| error.message)?;
        if !matches!(
            name_tokens.as_slice(),
            [token::Token {
                kind: token::TokenKind::Identifier(name),
                ..
            }, token::Token {
                kind: token::TokenKind::Eof,
                ..
            }] if name == macro_name
        ) {
            return Err(format!("`{macro_name}` is not a valid macro name"));
        }
        if self
            .native_macros
            .iter()
            .any(|definition| definition.name == macro_name)
        {
            return Err(format!("native macro `{macro_name}` is already registered"));
        }
        self.interpreter.register_native_function(
            native_name,
            macro_name,
            min_arity,
            max_arity,
            function,
        )?;
        self.native_macros.push(macros::NativeMacroDefinition {
            name: macro_name,
            target: native_name,
        });
        Ok(())
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, RilsError> {
        let tokens = lexer::lex(source).map_err(RilsError::Lex)?;
        let program = parser::parse_with_native_macros(tokens, &self.native_macros)
            .map_err(RilsError::Parse)?;
        let analysis = rils_frontend::analysis::analyze_program(&program);
        self.interpreter
            .execute_with_analysis(&program, &analysis)
            .map_err(RilsError::Runtime)
    }

    pub fn eval_file(&mut self, path: impl AsRef<Path>) -> Result<Value, RilsError> {
        let path = path.as_ref();
        let mut sources = ProjectSources::default();
        let result = (|| {
            let project = rils_driver::discover_entry_project(path)
                .map_err(|error| driver_message(error.to_string()))?;
            sources.register_project(&project);
            let source = fs::read_to_string(path).map_err(|error| {
                driver_message(format!(
                    "failed to load module `{}`: {error}",
                    path.display()
                ))
            })?;
            let source_id = sources.register_source(path, &source);
            if project.manifest_path().is_some() {
                sources.set_entry_source(source_id);
            }
            let mut program = sources.parse(source_id, &self.native_macros)?;
            rils_driver::load_file_modules(
                &mut program,
                path,
                &project,
                &self.native_macros,
                &mut sources,
                true,
            )
            .map_err(driver_error_to_rils)?;
            if project.manifest_path().is_some() {
                execute_project(&mut sources, &mut self.interpreter, &HostContract::new())
                    .map_err(RilsError::Runtime)
            } else {
                let analysis = rils_frontend::analysis::analyze_program(&program);
                self.interpreter
                    .execute_with_analysis(&program, &analysis)
                    .map_err(RilsError::Runtime)
            }
        })();
        result.map_err(|error| locate_rils_error(error, &sources))
    }
}

fn locate_rils_error(error: RilsError, sources: &ProjectSources) -> RilsError {
    if matches!(error, RilsError::Located { .. }) {
        return error;
    }
    let Some((source_name, source)) = sources.location(error.span().source) else {
        return error;
    };
    RilsError::Located {
        error: Box::new(error),
        source_name: source_name.into(),
        source: source.into(),
    }
}

fn parse_module_path(path: &str) -> Result<Vec<String>, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    path.split("::")
        .map(|segment| {
            is_identifier(segment)
                .then(|| segment.to_string())
                .ok_or_else(|| format!("`{path}` is not a valid module path"))
        })
        .collect()
}

pub(crate) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

fn driver_message(message: String) -> RilsError {
    RilsError::Runtime(RuntimeError {
        message,
        span: Span::default(),
        stack: Vec::new(),
    })
}

fn execute_project(
    sources: &mut ProjectSources,
    interpreter: &mut Interpreter,
    host: &HostContract,
) -> Result<Value, RuntimeError> {
    sources.analyze_project(host);
    let project = sources.project_id();
    let session = sources.session();
    let semantics = session
        .project(project)
        .expect("registered project has semantic state");
    let syntax = session
        .project_syntax(project)
        .expect("registered project has syntax state");
    let analysis = session
        .project_analysis(project, host)
        .expect("project analysis was stored");
    if let Some(diagnostic) = analysis.first_error() {
        return Err(RuntimeError {
            message: diagnostic.message.clone(),
            span: diagnostic.span,
            stack: Vec::new(),
        });
    }
    let source = semantics
        .entry_source()
        .expect("executable project has an entry source");
    let module = semantics
        .module(source)
        .expect("entry source has a module identity");
    let entry = analysis
        .def_map
        .definitions()
        .find(|definition| {
            definition.name == "main"
                && definition.span.source == source
                && definition.kind == rils_frontend::semantic::SymbolKind::Function
                && definition.container
                    == Some(rils_frontend::SymbolContainer::Module(module.path.clone()))
        })
        .map(|definition| definition.id)
        .expect("validated executable project preserves main definition");
    interpreter.execute_project_with_analysis(syntax, semantics.module_graph(), analysis, entry)
}

fn driver_error_to_rils(error: DriverError) -> RilsError {
    match error {
        DriverError::Frontend(error) => error.into(),
        DriverError::Message { message, span } => RilsError::Runtime(RuntimeError {
            message,
            span,
            stack: Vec::new(),
        }),
    }
}

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

pub fn eval(source: &str) -> Result<Value, RilsError> {
    Engine::new().eval(source)
}
