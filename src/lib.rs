pub mod bytecode;
mod environment;
mod hash_collections;
mod interpreter;
mod numeric;
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

    pub fn analyze_with_host(
        source: &str,
        host: &crate::HostContract,
    ) -> Result<DocumentAnalysis, crate::RilsError> {
        let signatures = host
            .functions()
            .map(|function| (function.name.clone(), function.signature.clone()))
            .collect();
        rils_frontend::analysis::analyze_with_host_functions(source, &signatures)
            .map_err(Into::into)
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
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

pub use bytecode::{
    BYTECODE_FORMAT_VERSION, BYTECODE_HOST_ABI_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeError,
    BytecodeFormatError, BytecodeHost, BytecodeImport, BytecodeModule, CompileError,
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_MANIFEST_FORMAT_VERSION,
    HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES,
    HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HostCallKind, HostContract,
    HostFunctionDeclaration, HostModuleDeclaration, HostReceiver, HostThreadAffinity,
};
pub use opaque_host::{OpaqueHostHandle, opaque_host_handle, opaque_host_value};
pub use rils_frontend::{
    FloatType, FrontendError, FunctionSignature, IntegerType, RuntimeValue, SourceFile, SourceId,
    Span, Type,
};
pub use rils_project::{Project, ProjectDependency, ProjectError, ProjectFile};
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
    Located {
        error: Box<RilsError>,
        source_name: String,
        source: String,
    },
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
            Self::Located { error, .. } => error.span(),
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        match self {
            Self::Located {
                error,
                source_name,
                source,
            } => error.render(source_name, source),
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
            Self::Located { error, .. } => error.fmt(f),
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
        let mut program = parser::parse_with_native_macros(tokens, &self.native_macros)
            .map_err(RilsError::Parse)?;
        rils_frontend::resolve_numeric_literals(&mut program).map_err(numeric_resolution_error)?;
        self.interpreter
            .execute(&program)
            .map_err(RilsError::Runtime)
    }

    pub fn eval_file(&mut self, path: impl AsRef<Path>) -> Result<Value, RilsError> {
        let path = path.as_ref();
        let mut sources = SourceRegistry::default();
        let result = (|| {
            let project =
                discover_entry_project(path).map_err(|error| module_message(error.to_string()))?;
            sources.register_project(&project);
            let source =
                fs::read_to_string(path).map_err(|error| module_load_error(path, error))?;
            let source_id = sources.register_source(path, &source);
            let tokens = lexer::lex_with_source_id(&source, source_id).map_err(RilsError::Lex)?;
            let mut program = parser::parse_with_native_macros(tokens, &self.native_macros)
                .map_err(RilsError::Parse)?;
            load_file_modules(
                &mut program.statements,
                path,
                &project,
                &self.native_macros,
                &mut sources,
                true,
            )?;
            rils_frontend::resolve_numeric_literals(&mut program)
                .map_err(numeric_resolution_error)?;
            self.interpreter
                .execute(&program)
                .map_err(RilsError::Runtime)
        })();
        result.map_err(|error| locate_rils_error(error, &sources))
    }
}

#[derive(Default)]
struct SourceRegistry {
    next_id: u32,
    by_path: HashMap<PathBuf, SourceId>,
    records: BTreeMap<SourceId, SourceRecord>,
}

struct SourceRecord {
    file: SourceFile,
    source: Option<String>,
}

impl SourceRegistry {
    fn register_project(&mut self, project: &Project) {
        for file in project.modules() {
            self.register_path(&file.path);
        }
    }

    fn register_path(&mut self, path: &Path) -> SourceId {
        let key = source_path_key(path);
        if let Some(id) = self.by_path.get(&key) {
            return *id;
        }
        self.next_id += 1;
        let id = SourceId::new(self.next_id);
        self.by_path.insert(key, id);
        self.records.insert(
            id,
            SourceRecord {
                file: SourceFile {
                    id,
                    name: path.to_string_lossy().into_owned(),
                },
                source: None,
            },
        );
        id
    }

    fn register_source(&mut self, path: &Path, source: &str) -> SourceId {
        let id = self.register_path(path);
        self.records.get_mut(&id).expect("registered source").source = Some(source.into());
        id
    }

    fn source_files(&self) -> Vec<SourceFile> {
        self.records
            .values()
            .map(|record| record.file.clone())
            .collect()
    }

    fn location(&self, id: SourceId) -> Option<(&str, &str)> {
        let record = self.records.get(&id)?;
        Some((&record.file.name, record.source.as_deref()?))
    }
}

fn source_path_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn locate_rils_error(error: RilsError, sources: &SourceRegistry) -> RilsError {
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
    sources: &mut SourceRegistry,
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
        let tokens = lexer::lex_with_source_id(&source, source_id).map_err(RilsError::Lex)?;
        let mut module =
            parser::parse_with_native_macros(tokens, native_macros).map_err(RilsError::Parse)?;
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

#[derive(Default)]
struct ProjectModuleNode {
    statements: Vec<ast::Stmt>,
    children: BTreeMap<String, ProjectModuleNode>,
}

fn load_file_modules(
    statements: &mut Vec<ast::Stmt>,
    entry_path: &Path,
    project: &Project,
    native_macros: &[macros::NativeMacroDefinition],
    sources: &mut SourceRegistry,
    require_entry: bool,
) -> Result<(), RilsError> {
    if project.manifest_path().is_none() {
        let base = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let mut loading = HashSet::new();
        if let Ok(canonical) = entry_path.canonicalize() {
            loading.insert(canonical);
        }
        return load_external_modules(statements, base, native_macros, &mut loading, sources);
    }
    let entry = project.module_for_file(entry_path).ok_or_else(|| {
        module_message(format!(
            "entry script `{}` is outside the script_paths configured by `{}`",
            entry_path.display(),
            project.manifest_path().unwrap().display()
        ))
    })?;
    let entry_statements = if require_entry {
        prepare_project_entry(std::mem::take(statements))?
    } else {
        reject_external_module_declarations(statements)?;
        std::mem::take(statements)
    };
    let mut root = ProjectModuleNode::default();
    for dependency in project.dependencies() {
        let Some(prelude_path) = dependency.prelude.as_deref() else {
            continue;
        };
        let source = fs::read_to_string(prelude_path)
            .map_err(|error| module_load_error(prelude_path, error))?;
        let source_id = sources.register_source(prelude_path, &source);
        let tokens = lexer::lex_with_source_id(&source, source_id).map_err(RilsError::Lex)?;
        let prelude =
            parser::parse_with_native_macros(tokens, native_macros).map_err(RilsError::Parse)?;
        reject_external_module_declarations(&prelude.statements)?;
        root.statements.extend(prelude.statements);
    }
    for file in project.modules() {
        let module_statements = if file.module_path == entry.module_path {
            entry_statements.clone()
        } else {
            let source = fs::read_to_string(&file.path)
                .map_err(|error| module_load_error(&file.path, error))?;
            let source_id = sources.register_source(&file.path, &source);
            let tokens = lexer::lex_with_source_id(&source, source_id).map_err(RilsError::Lex)?;
            let program = parser::parse_with_native_macros(tokens, native_macros)
                .map_err(RilsError::Parse)?;
            reject_external_module_declarations(&program.statements)?;
            program.statements
        };
        insert_project_module(&mut root, &file.module_path, module_statements);
    }
    *statements = project_module_statements(root);
    if require_entry {
        let mut entry_path = entry
            .module_path
            .split("::")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        entry_path.push("main".into());
        statements.push(ast::Stmt::Expr {
            expression: ast::Expr::Call {
                callee: Box::new(ast::Expr::Path {
                    segments: entry_path,
                    span: Span::default(),
                }),
                arguments: Vec::new(),
                span: Span::default(),
            },
            terminated: false,
        });
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

fn insert_project_module(
    root: &mut ProjectModuleNode,
    module_path: &str,
    statements: Vec<ast::Stmt>,
) {
    let mut node = root;
    for segment in module_path.split("::") {
        node = node.children.entry(segment.to_owned()).or_default();
    }
    node.statements = statements;
}

fn project_module_statements(node: ProjectModuleNode) -> Vec<ast::Stmt> {
    let mut statements = node.statements;
    for (name, child) in node.children {
        let module = ast::Stmt::Module {
            name: name.clone(),
            name_span: Span::default(),
            statements: Some(project_module_statements(child)),
            span: Span::default(),
        };
        statements.push(ast::Stmt::Public {
            statement: Box::new(module),
            span: Span::default(),
        });
    }
    statements
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

fn numeric_resolution_error(error: rils_frontend::NumericResolutionError) -> RilsError {
    RilsError::Runtime(RuntimeError {
        message: error.message,
        span: error.span,
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

/// Compiles Rils source using declarations supplied by a host contract.
pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    bytecode::compile_with_host(source, host)
}

/// Loads a Rils source file and its external modules, then compiles them into a
/// reusable in-memory bytecode module.
pub fn compile_file(path: impl AsRef<Path>) -> Result<BytecodeModule, CompileError> {
    let path = path.as_ref();
    let project = discover_entry_project(path)
        .map_err(|error| CompileError::new(error.to_string(), Span::default()))?;
    let mut host: Option<HostContract> = None;
    for manifest in project.host_manifests() {
        let bytes = fs::read(manifest).map_err(|error| {
            CompileError::new(
                format!(
                    "failed to read host manifest `{}`: {error}",
                    manifest.display()
                ),
                Span::default(),
            )
        })?;
        let fragment = HostContract::from_manifest_bytes(&bytes).map_err(|message| {
            CompileError::new(
                format!("invalid host manifest `{}`: {message}", manifest.display()),
                Span::default(),
            )
        })?;
        if let Some(host) = &mut host {
            host.merge(&fragment).map_err(|message| {
                CompileError::new(
                    format!(
                        "cannot merge host manifest `{}`: {message}",
                        manifest.display()
                    ),
                    Span::default(),
                )
            })?;
        } else {
            host = Some(fragment);
        }
    }
    let host = host.unwrap_or_default();
    compile_project_file_with_host(path, &project, &host, project.requires_entry())
}

/// Loads and compiles a Rils module tree using declarations supplied by a host contract.
pub fn compile_file_with_host(
    path: impl AsRef<Path>,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    let path = path.as_ref();
    let project = discover_entry_project(path)
        .map_err(|error| CompileError::new(error.to_string(), Span::default()))?;
    compile_project_file_with_host(path, &project, host, project.requires_entry())
}

fn discover_entry_project(path: &Path) -> Result<Project, ProjectError> {
    Project::discover_configured(path, None)?
        .map(Ok)
        .unwrap_or_else(|| Project::for_legacy_entry(path))
}

fn compile_project_file_with_host(
    path: &Path,
    project: &Project,
    host: &HostContract,
    require_entry: bool,
) -> Result<BytecodeModule, CompileError> {
    let mut sources = SourceRegistry::default();
    sources.register_project(project);
    let result = (|| {
        let source = fs::read_to_string(path).map_err(|error| {
            CompileError::new(
                format!("failed to load `{}`: {error}", path.display()),
                Span::default(),
            )
        })?;
        let source_id = sources.register_source(path, &source);
        let tokens = lexer::lex_with_source_id(&source, source_id)
            .map_err(|error| CompileError::new(error.message, error.span))?;
        let mut program =
            parser::parse(tokens).map_err(|error| CompileError::new(error.message, error.span))?;
        load_file_modules(
            &mut program.statements,
            path,
            project,
            &[],
            &mut sources,
            require_entry,
        )
        .map_err(|error| CompileError::new(error.to_string(), error.span()))?;
        bytecode::compile_program_with_host_and_sources(&program, host, sources.source_files())
    })();
    result.map_err(|error| locate_compile_error(error, &sources))
}

fn locate_compile_error(error: CompileError, sources: &SourceRegistry) -> CompileError {
    if error.source_name().is_some() {
        return error;
    }
    let Some((source_name, source)) = sources.location(error.span.source) else {
        return error;
    };
    error.with_source(source_name, source)
}

#[cfg(test)]
mod tests;
