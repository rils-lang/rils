mod environment;
mod error;
mod formatting;
mod hash_collections;
mod interpreter;
mod limits;
mod native_type;
mod numeric;
mod output;
mod project_compilation;
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

    pub use crate::load_file_modules;
    pub use crate::project_compilation::ProjectCompilation;
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

use std::{
    collections::HashSet,
    fmt, fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use project_compilation::ProjectCompilation;

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
        let mut sources = ProjectCompilation::default();
        let result = (|| {
            let project =
                discover_entry_project(path).map_err(|error| module_message(error.to_string()))?;
            sources.register_project(&project);
            let source =
                fs::read_to_string(path).map_err(|error| module_load_error(path, error))?;
            let source_id = sources.register_source(path, &source);
            if project.manifest_path().is_some() {
                sources.set_entry_source(source_id);
            }
            let mut program = sources.parse(source_id, &self.native_macros)?;
            load_file_modules(
                &mut program,
                path,
                &project,
                &self.native_macros,
                &mut sources,
                true,
            )?;
            if project.manifest_path().is_some() {
                sources
                    .execute_project(&mut self.interpreter, &HostContract::new())
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

fn locate_rils_error(error: RilsError, sources: &ProjectCompilation) -> RilsError {
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

fn load_external_modules(
    statements: &mut [ast::Stmt],
    base: &Path,
    native_macros: &[macros::NativeMacroDefinition],
    loading: &mut HashSet<PathBuf>,
    sources: &mut ProjectCompilation,
) -> Result<(), RilsError> {
    for statement in statements {
        let statement = match statement {
            ast::Stmt::Public { statement, .. } => statement.as_mut(),
            statement => statement,
        };
        let ast::Stmt::Module {
            name, statements, ..
        } = statement
        else {
            continue;
        };
        if let Some(statements) = statements {
            load_external_modules(statements, base, native_macros, loading, sources)?;
            continue;
        }

        let flat = base.join(format!("{name}.rils"));
        let nested = base.join(name.as_str()).join("mod.rils");
        let path = if flat.is_file() {
            flat
        } else if nested.is_file() {
            nested
        } else {
            return Err(module_message(format!(
                "cannot find module `{name}`; expected `{}` or `{}`",
                flat.display(),
                nested.display()
            )));
        };
        let canonical = path
            .canonicalize()
            .map_err(|error| module_load_error(&path, error))?;
        if !loading.insert(canonical.clone()) {
            return Err(module_message(format!(
                "cyclic module load detected at `{}`",
                path.display()
            )));
        }
        let source = fs::read_to_string(&path).map_err(|error| module_load_error(&path, error))?;
        let source_id = sources.register_source(&path, &source);
        let mut module = sources.parse(source_id, native_macros)?;
        load_external_modules(
            &mut module.statements,
            path.parent().unwrap_or(base),
            native_macros,
            loading,
            sources,
        )?;
        loading.remove(&canonical);
        *statements = Some(module.statements);
    }
    Ok(())
}

pub fn load_file_modules(
    program: &mut ast::Program,
    entry_path: &Path,
    project: &Project,
    native_macros: &[macros::NativeMacroDefinition],
    sources: &mut ProjectCompilation,
    require_entry: bool,
) -> Result<(), RilsError> {
    if project.manifest_path().is_none() {
        let base = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let mut loading = HashSet::new();
        if let Ok(canonical) = entry_path.canonicalize() {
            loading.insert(canonical);
        }
        load_external_modules(
            &mut program.statements,
            base,
            native_macros,
            &mut loading,
            sources,
        )?;
        sources.push_root_program(program.clone());
        return Ok(());
    }
    let entry = project.module_for_file(entry_path);
    let entry_source = sources.source_id(entry_path);
    let entry_is_prelude = project.prelude().is_some_and(|prelude_path| {
        prelude_path == entry_path
            || entry_path.canonicalize().is_ok_and(|entry_path| {
                prelude_path
                    .canonicalize()
                    .is_ok_and(|path| path == entry_path)
            })
    });
    if entry.is_none() && !entry_is_prelude {
        return Err(module_message(format!(
            "entry script `{}` is outside the src roots configured by `{}`",
            entry_path.display(),
            project.manifest_path().unwrap().display()
        )));
    }
    let entry_statements = if require_entry {
        prepare_project_entry(std::mem::take(&mut program.statements))?
    } else {
        reject_external_module_declarations(&program.statements)?;
        std::mem::take(&mut program.statements)
    };
    let mut entry_program = program.clone();
    entry_program.statements = entry_statements;
    if entry_is_prelude {
        sources.push_root_program(entry_program.clone());
    } else if let Some(prelude_path) = project.prelude() {
        let source = fs::read_to_string(prelude_path)
            .map_err(|error| module_load_error(prelude_path, error))?;
        let source_id = sources.register_source(prelude_path, &source);
        let prelude = sources.parse(source_id, native_macros)?;
        reject_external_module_declarations(&prelude.statements)?;
        sources.push_root_program(prelude);
    }
    for dependency in project.dependencies() {
        let Some(prelude_path) = dependency.prelude.as_deref() else {
            continue;
        };
        let source = fs::read_to_string(prelude_path)
            .map_err(|error| module_load_error(prelude_path, error))?;
        let source_id = sources.register_source(prelude_path, &source);
        let prelude = sources.parse(source_id, native_macros)?;
        reject_external_module_declarations(&prelude.statements)?;
        sources.push_root_program(prelude);
    }
    for file in project.modules() {
        let file_source = sources
            .source_id(&file.path)
            .expect("project modules were registered before loading");
        let module_program = if entry_source == Some(file_source) {
            entry_program.clone()
        } else {
            let source = fs::read_to_string(&file.path)
                .map_err(|error| module_load_error(&file.path, error))?;
            let source_id = sources.register_source(&file.path, &source);
            let program = sources.parse(source_id, native_macros)?;
            reject_external_module_declarations(&program.statements)?;
            program
        };
        sources.set_module_program(file_source, module_program);
    }
    Ok(())
}

fn prepare_project_entry(statements: Vec<ast::Stmt>) -> Result<Vec<ast::Stmt>, RilsError> {
    reject_external_module_declarations(&statements)?;
    let mut found = false;
    let mut prepared = Vec::with_capacity(statements.len());
    for statement in statements {
        match statement {
            ast::Stmt::Function {
                ref name,
                ref parameters,
                span,
                ..
            } if name == "main" => {
                if found {
                    return Err(RilsError::Runtime(RuntimeError {
                        message: "project entry contains more than one `fn main()`".into(),
                        span,
                        stack: Vec::new(),
                    }));
                }
                if !parameters.is_empty() {
                    return Err(RilsError::Runtime(RuntimeError {
                        message: "project entry `fn main()` must not have parameters".into(),
                        span,
                        stack: Vec::new(),
                    }));
                }
                found = true;
                prepared.push(ast::Stmt::Public {
                    statement: Box::new(statement),
                    span,
                });
            }
            ast::Stmt::Public { statement, span } => {
                if let ast::Stmt::Function {
                    name,
                    parameters,
                    span: function_span,
                    ..
                } = statement.as_ref()
                    && name == "main"
                {
                    if found {
                        return Err(RilsError::Runtime(RuntimeError {
                            message: "project entry contains more than one `fn main()`".into(),
                            span: *function_span,
                            stack: Vec::new(),
                        }));
                    }
                    if !parameters.is_empty() {
                        return Err(RilsError::Runtime(RuntimeError {
                            message: "project entry `fn main()` must not have parameters".into(),
                            span: *function_span,
                            stack: Vec::new(),
                        }));
                    }
                    found = true;
                }
                prepared.push(ast::Stmt::Public { statement, span });
            }
            statement => prepared.push(statement),
        }
    }
    if !found {
        return Err(module_message(
            "a rils.toml project entry must define a zero-parameter `fn main()`".into(),
        ));
    }
    Ok(prepared)
}

fn reject_external_module_declarations(statements: &[ast::Stmt]) -> Result<(), RilsError> {
    for statement in statements {
        let statement = match statement {
            ast::Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        if let ast::Stmt::Module {
            name,
            statements: None,
            span,
            ..
        } = statement
        {
            return Err(RilsError::Runtime(RuntimeError {
                message: format!(
                    "external `mod {name};` declarations are not used in rils.toml projects; reference the module with `use` or a qualified path"
                ),
                span: *span,
                stack: Vec::new(),
            }));
        }
        if let ast::Stmt::Module {
            statements: Some(statements),
            ..
        } = statement
        {
            reject_external_module_declarations(statements)?;
        }
    }
    Ok(())
}

fn module_load_error(path: &Path, error: impl fmt::Display) -> RilsError {
    module_message(format!(
        "failed to load module `{}`: {error}",
        path.display()
    ))
}

fn module_message(message: String) -> RilsError {
    RilsError::Runtime(RuntimeError {
        message,
        span: Span::default(),
        stack: Vec::new(),
    })
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

fn discover_entry_project(path: &Path) -> Result<Project, ProjectError> {
    Project::discover_configured(path, None)?
        .map(Ok)
        .unwrap_or_else(|| Project::for_legacy_entry(path))
}
