pub mod bytecode;
mod environment;
mod interpreter;
mod runtime_type;
mod standard_library;
mod value;

mod hir {
    pub(crate) use rils_compiler::hir::*;
}
mod mir {
    pub(crate) use rils_compiler::mir::*;
}

pub mod analysis {
    pub use rils_frontend::analysis::{
        AnalysisDiagnostic, DiagnosticSeverity, DocumentAnalysis, InlayTypeHint, SymbolKind,
        SymbolOccurrence,
    };

    pub fn analyze(source: &str) -> Result<DocumentAnalysis, crate::RilsError> {
        rils_frontend::analysis::analyze(source).map_err(Into::into)
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
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

pub use bytecode::{
    BYTECODE_HOST_ABI_VERSION, BytecodeError, BytecodeHost, BytecodeImport, BytecodeModule,
    CompileError,
};
pub use rils_frontend::{FrontendError, FunctionSignature, RuntimeValue, Span, Type};
pub use value::Value;

pub type NativeFunctionHandler = fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub struct NativeTypeHandle {
    definition: std::rc::Rc<value::HostType>,
}

impl NativeTypeHandle {
    pub fn value<T: 'static>(&self, payload: T) -> Value {
        Value::HostObject(std::rc::Rc::new(value::HostObject {
            type_definition: self.definition.clone(),
            payload: std::rc::Rc::new(payload),
        }))
    }

    pub fn register_method<F>(
        &self,
        name: &str,
        min_arity: usize,
        max_arity: usize,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid method name"));
        }
        if min_arity > max_arity {
            return Err("method minimum arity cannot exceed maximum arity".into());
        }
        if self.definition.methods.borrow().contains_key(name) {
            return Err(format!("method `{name}` is already registered"));
        }
        self.definition.methods.borrow_mut().insert(
            name.into(),
            std::rc::Rc::new(value::HostFunction {
                name: name.into(),
                min_arity,
                max_arity,
                signature: None,
                function: std::rc::Rc::new(function),
            }),
        );
        Ok(())
    }

    pub fn register_typed_method<F>(
        &self,
        name: &str,
        parameters: Vec<Type>,
        return_type: Type,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid method name"));
        }
        if self.definition.methods.borrow().contains_key(name) {
            return Err(format!("method `{name}` is already registered"));
        }
        let arity = parameters.len();
        self.definition.methods.borrow_mut().insert(
            name.into(),
            std::rc::Rc::new(value::HostFunction {
                name: name.into(),
                min_arity: arity,
                max_arity: arity,
                signature: Some(FunctionSignature::fixed(parameters, return_type)),
                function: std::rc::Rc::new(function),
            }),
        );
        Ok(())
    }
}

use interpreter::{Interpreter, RuntimeError};
use lexer::LexError;
use parser::ParseError;
use source::format_diagnostic;

#[derive(Clone, Debug, PartialEq)]
pub enum RilsError {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
}

impl From<FrontendError> for RilsError {
    fn from(error: FrontendError) -> Self {
        match error {
            FrontendError::Lex(error) => Self::Lex(error),
            FrontendError::Parse(error) => Self::Parse(error),
        }
    }
}

impl RilsError {
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span,
            Self::Parse(error) => error.span,
            Self::Runtime(error) => error.span,
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        match self {
            Self::Lex(error) => format_diagnostic(
                source_name,
                source,
                error.span,
                &format!("lex error: {}", error.message),
            ),
            Self::Parse(error) => format_diagnostic(
                source_name,
                source,
                error.span,
                &format!("parse error: {}", error.message),
            ),
            Self::Runtime(error) => {
                let mut diagnostic = format_diagnostic(
                    source_name,
                    source,
                    error.span,
                    &format!("runtime error: {}", error.message),
                );
                if !error.stack.is_empty() {
                    diagnostic.push_str("\n\nRils stack:");
                    for function in error.stack.iter().rev() {
                        diagnostic.push_str(&format!("\n  in {function}"));
                    }
                }
                diagnostic
            }
        }
    }
}

impl fmt::Display for RilsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(f, "lex error: {}", error.message),
            Self::Parse(error) => write!(f, "parse error: {}", error.message),
            Self::Runtime(error) => write!(f, "runtime error: {}", error.message),
        }
    }
}

impl Error for RilsError {}

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
        self.interpreter
            .execute(&program)
            .map_err(RilsError::Runtime)
    }

    pub fn eval_file(&mut self, path: impl AsRef<Path>) -> Result<Value, RilsError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| module_load_error(path, error))?;
        let tokens = lexer::lex(&source).map_err(RilsError::Lex)?;
        let mut program = parser::parse_with_native_macros(tokens, &self.native_macros)
            .map_err(RilsError::Parse)?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let mut loading = HashSet::new();
        if let Ok(canonical) = path.canonicalize() {
            loading.insert(canonical);
        }
        load_external_modules(
            &mut program.statements,
            base,
            &self.native_macros,
            &mut loading,
        )?;
        self.interpreter
            .execute(&program)
            .map_err(RilsError::Runtime)
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

fn is_identifier(name: &str) -> bool {
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
            load_external_modules(statements, base, native_macros, loading)?;
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
        let tokens = lexer::lex(&source).map_err(RilsError::Lex)?;
        let mut module =
            parser::parse_with_native_macros(tokens, native_macros).map_err(RilsError::Parse)?;
        load_external_modules(
            &mut module.statements,
            path.parent().unwrap_or(base),
            native_macros,
            loading,
        )?;
        loading.remove(&canonical);
        *statements = Some(module.statements);
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

/// Compiles Rils source into a verified, reusable in-memory bytecode module.
pub fn compile(source: &str) -> Result<BytecodeModule, CompileError> {
    bytecode::compile(source)
}

/// Loads a Rils source file and its external modules, then compiles them into a
/// reusable in-memory bytecode module.
pub fn compile_file(path: impl AsRef<Path>) -> Result<BytecodeModule, CompileError> {
    let path = path.as_ref();
    let source = fs::read_to_string(path).map_err(|error| CompileError {
        message: format!("failed to load `{}`: {error}", path.display()),
        span: Span::default(),
    })?;
    let tokens = lexer::lex(&source).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    let mut program = parser::parse(tokens).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut loading = HashSet::new();
    if let Ok(canonical) = path.canonicalize() {
        loading.insert(canonical);
    }
    load_external_modules(&mut program.statements, base, &[], &mut loading).map_err(|error| {
        CompileError {
            message: error.to_string(),
            span: error.span(),
        }
    })?;
    bytecode::compile_program(&program)
}

#[cfg(test)]
mod tests;
