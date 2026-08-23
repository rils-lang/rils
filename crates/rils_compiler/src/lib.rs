pub mod hir;
mod host;
pub mod mir;

pub use host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_INLINE_VALUE_MAX_BYTES,
    HOST_INLINE_VALUE_MAX_FIELDS, HOST_MANIFEST_FORMAT_VERSION, HOST_MANIFEST_HEADER_SIZE,
    HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES, HOST_MANIFEST_MAGIC,
    HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS, HOST_MANIFEST_MAX_MODULES,
    HOST_MANIFEST_MAX_PARAMETERS, HOST_MANIFEST_MAX_TYPES, HostCallKind, HostContract,
    HostFunctionDeclaration, HostModuleDeclaration, HostReceiver, HostThreadAffinity,
    HostTypeDeclaration, HostTypeTransport, HostValueFieldType, HostValueLayout,
};

mod ast {
    pub(crate) use rils_frontend::ast::*;
}
mod bytecode {
    pub(crate) use crate::CompileError;
}
mod source {
    pub(crate) use rils_frontend::source::*;
}
mod types {
    pub(crate) use rils_frontend::types::*;
}

use std::{error::Error, fmt};

use rils_frontend::{
    analysis::DiagnosticSeverity,
    ast::Program,
    source::{SourceFile, Span},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
    source_name: Option<String>,
    source: Option<String>,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            source_name: None,
            source: None,
        }
    }

    pub fn unsupported(message: impl Into<String>, span: Span) -> Self {
        Self::new(message, span)
    }

    pub fn with_source(
        mut self,
        source_name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        self.source_name = Some(source_name.into());
        self.source = Some(source.into());
        self
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        let source_name = self.source_name().unwrap_or(source_name);
        let source = self.source_text().unwrap_or(source);
        rils_frontend::source::format_diagnostic(
            source_name,
            source,
            self.span,
            &format!("compile error: {}", self.message),
        )
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "compile error: {}", self.message)
    }
}

impl Error for CompileError {}

pub fn compile(source: &str) -> Result<mir::MirProgram, CompileError> {
    compile_with_host(source, &HostContract::new())
}

pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<mir::MirProgram, CompileError> {
    let tokens = rils_frontend::lexer::lex(source)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    let program = rils_frontend::parser::parse(tokens)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    compile_program_with_host(&program, host)
}

pub fn compile_program(program: &Program) -> Result<mir::MirProgram, CompileError> {
    compile_program_with_host(program, &HostContract::new())
}

pub fn compile_program_with_host(
    program: &Program,
    host: &HostContract,
) -> Result<mir::MirProgram, CompileError> {
    compile_program_with_host_and_sources(program, host, Vec::new())
}

pub fn compile_program_with_host_and_sources(
    program: &Program,
    host: &HostContract,
    sources: Vec<SourceFile>,
) -> Result<mir::MirProgram, CompileError> {
    let mut program = program.clone();
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    if let Some(error) = rils_frontend::resolve_host_type_names(&mut program, &host_types)
        .into_iter()
        .next()
    {
        return Err(CompileError::new(error.message, error.span));
    }
    rils_frontend::resolve_numeric_literals_with_host_functions(&mut program, &signatures)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    let analysis = rils_frontend::analysis::analyze_program_with_host_declarations(
        &program,
        &signatures,
        &host_types,
    );
    if let Some(diagnostic) = analysis
        .diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError::new(diagnostic.message, diagnostic.span));
    }
    mir::lower(hir::lower_with_host(
        &program,
        host,
        &analysis.expression_types,
        sources,
    )?)
}

#[cfg(test)]
mod tests {
    use super::{compile, compile_with_host};
    use crate::{HostContract, HostReceiver, HostTypeTransport, HostValueLayout};
    use rils_frontend::{FloatType, FunctionSignature, IntegerType, Type};

    #[test]
    fn compiles_source_through_static_analysis_hir_and_mir() {
        let program = compile("fn add(left: i32, right: i32) -> i32 { left + right } add(1, 2)")
            .expect("source should lower to MIR");

        assert_eq!(program.entry, 0);
        assert_eq!(program.functions.len(), 2);
        assert!(
            program
                .functions
                .iter()
                .all(|function| !function.blocks.is_empty())
        );
    }

    #[test]
    fn rejects_static_errors_before_lowering() {
        let error = match compile("let value = 1; value = 2;") {
            Ok(_) => panic!("assignment should fail"),
            Err(error) => error,
        };

        assert!(error.message.contains("immutable"));
    }

    #[test]
    fn rils_source_functions_remain_non_overloadable() {
        let error = match compile(
            "fn choose(value: i32) -> i32 { value } \
             fn choose(value: f32) -> f32 { value }",
        ) {
            Ok(_) => panic!("Rils source functions must not define overloads"),
            Err(error) => error,
        };
        assert!(
            error
                .message
                .contains("`choose` is already defined in this scope"),
            "{}",
            error.message
        );
    }

    #[test]
    fn lowers_host_receiver_method_calls() {
        let mut host = HostContract::new();
        host.register_function_with_options_and_receiver(
            900,
            "unity::game_object::active_self",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::Bool),
            "unity.game_object",
            crate::HostCallKind::Direct,
            crate::HostThreadAffinity::MainThread,
            Some(HostReceiver::Ref),
        )
        .unwrap();
        compile_with_host(
            "fn check(object: HostHandle) -> bool { object.active_self() }",
            &host,
        )
        .expect("host receiver calls should lower");
    }

    #[test]
    fn resolves_overloaded_host_receiver_methods() {
        let mut host = HostContract::new();
        host.register_type(
            "unity_engine::Object",
            None::<&str>,
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        for (id, value_type) in [
            (905, Type::Integer(IntegerType::I32)),
            (906, Type::Float(FloatType::F32)),
        ] {
            host.register_function_with_options_and_receiver(
                id,
                "unity_engine::object::set_value",
                FunctionSignature::fixed(
                    vec![Type::named("unity_engine::Object"), value_type],
                    Type::Unit,
                ),
                "unity.object",
                crate::HostCallKind::Direct,
                crate::HostThreadAffinity::MainThread,
                Some(HostReceiver::RefMut),
            )
            .unwrap();
        }

        compile_with_host(
            "fn update(mut object: unity_engine::Object) { \
             object.set_value(1i32); object.set_value(1.0f32); }",
            &host,
        )
        .expect("receiver overloads should resolve after adding the implicit receiver argument");
    }

    #[test]
    fn lowers_inherited_named_host_receiver_methods() {
        let mut host = HostContract::new();
        host.register_type(
            "unity_engine::Object",
            None::<&str>,
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        host.register_type(
            "unity_engine::GameObject",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        host.register_function_with_options_and_receiver(
            901,
            "unity_engine::object::instance_id",
            FunctionSignature::fixed(
                vec![Type::named("unity_engine::Object")],
                Type::Integer(IntegerType::I64),
            ),
            "unity_engine.object",
            crate::HostCallKind::Direct,
            crate::HostThreadAffinity::MainThread,
            Some(HostReceiver::Ref),
        )
        .unwrap();
        compile_with_host(
            "fn id(object: unity_engine::GameObject) -> i64 { object.instance_id() }",
            &host,
        )
        .expect("derived host types should inherit receiver methods");

        for source in [
            "use unity_engine::*; fn id(object: GameObject) -> i64 { object.instance_id() }",
            "use unity_engine::GameObject; fn id(object: GameObject) -> i64 { object.instance_id() }",
            "use unity_engine::GameObject as Go; fn id(object: Go) -> i64 { object.instance_id() }",
            "fn id(object: GameObject) -> i64 { object.instance_id() } use unity_engine::*;",
            "mod nested { use unity_engine::*; fn id(object: GameObject) -> i64 { object.instance_id() } }",
            "use unity_engine::GameObject as Go; struct Holder { object: Go }",
        ] {
            compile_with_host(source, &host)
                .expect("imported host type identities should be canonical before lowering");
        }
    }

    #[test]
    fn lowers_associated_host_functions_through_glob_imported_types() {
        let mut host = HostContract::new();
        host.register_value_type("unity_engine::Vector3", HostValueLayout::F32x3)
            .unwrap();
        host.register_function(
            902,
            "unity_engine::Vector3::new",
            FunctionSignature::fixed(
                vec![Type::Float(FloatType::F32); 3],
                Type::named("unity_engine::Vector3"),
            ),
            "unity_engine.math",
        )
        .unwrap();

        compile_with_host(
            "use unity_engine::*; fn make() -> Vector3 { Vector3::new(1.0f32, 2.0f32, 3.0f32) }",
            &host,
        )
        .expect("glob-imported host types should qualify their associated functions");
    }

    #[test]
    fn resolves_host_overloads_by_exact_argument_types() {
        let mut host = HostContract::new();
        host.register_function(
            910,
            "unity_engine::math::pick",
            FunctionSignature::fixed(
                vec![Type::Integer(IntegerType::I32)],
                Type::Integer(IntegerType::I32),
            ),
            "unity_engine.math",
        )
        .unwrap();
        host.register_function(
            911,
            "unity_engine::math::pick",
            FunctionSignature::fixed(
                vec![Type::Float(FloatType::F32)],
                Type::Float(FloatType::F32),
            ),
            "unity_engine.math",
        )
        .unwrap();

        compile_with_host(
            "use unity_engine::math::*; pick(1i32); pick(1.0f32);",
            &host,
        )
        .expect("exact argument types should select different host overloads");
        compile_with_host("use unity_engine::math::pick; pick(pick(1i32));", &host)
            .expect("a selected overload return type should drive an enclosing overload call");

        let error = match compile_with_host("use unity_engine::math::pick; pick(true);", &host) {
            Ok(_) => panic!("an unmatched overload should fail before bytecode generation"),
            Err(error) => error,
        };
        assert!(error.message.contains("no host overload"));
        assert!(error.message.contains("pick(i32)"));
        assert!(error.message.contains("pick(f32)"));
    }

    #[test]
    fn prefers_the_nearest_host_base_type_and_reports_equal_candidates() {
        let mut host = HostContract::new();
        host.register_type(
            "unity_engine::Object",
            None::<&str>,
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        host.register_type(
            "unity_engine::Component",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        host.register_type(
            "unity_engine::Transform",
            Some("unity_engine::Component"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
        for (id, parameter) in [
            (920, "unity_engine::Object"),
            (921, "unity_engine::Component"),
        ] {
            host.register_function(
                id,
                "unity_engine::inspect",
                FunctionSignature::fixed(vec![Type::named(parameter)], Type::Bool),
                "unity_engine",
            )
            .unwrap();
        }
        compile_with_host(
            "fn inspect_transform(value: unity_engine::Transform) -> bool { \
             unity_engine::inspect(value) }",
            &host,
        )
        .expect("the Component overload should beat the Object overload");

        host.register_function(
            922,
            "unity_engine::compare",
            FunctionSignature::fixed(
                vec![
                    Type::named("unity_engine::Object"),
                    Type::named("unity_engine::Component"),
                ],
                Type::Bool,
            ),
            "unity_engine",
        )
        .unwrap();
        host.register_function(
            923,
            "unity_engine::compare",
            FunctionSignature::fixed(
                vec![
                    Type::named("unity_engine::Component"),
                    Type::named("unity_engine::Object"),
                ],
                Type::Bool,
            ),
            "unity_engine",
        )
        .unwrap();
        let error = match compile_with_host(
            "fn compare_transform(value: unity_engine::Transform) -> bool { \
             unity_engine::compare(value, value) }",
            &host,
        ) {
            Ok(_) => panic!("equally specific overloads should be ambiguous"),
            Err(error) => error,
        };
        assert!(error.message.contains("ambiguous host call"));
        assert!(error.message.contains("explicit type annotations or casts"));
    }

    #[test]
    fn reports_missing_and_ambiguous_host_type_imports_before_lowering() {
        let mut host = HostContract::new();
        for name in ["alpha::Object", "beta::Object"] {
            host.register_type(name, None::<&str>, HostTypeTransport::HostHandle)
                .unwrap();
        }

        let missing = match compile_with_host("fn inspect(value: Object) {}", &host) {
            Ok(_) => panic!("unimported host type should fail"),
            Err(error) => error,
        };
        assert!(
            missing
                .message
                .contains("host type `Object` is not in scope")
        );

        let ambiguous = match compile_with_host(
            "use alpha::*; use beta::*; fn inspect(value: Object) {}",
            &host,
        ) {
            Ok(_) => panic!("ambiguous host type should fail"),
            Err(error) => error,
        };
        assert!(
            ambiguous
                .message
                .contains("host type `Object` is ambiguous")
        );
        assert!(ambiguous.message.contains("alpha::Object"));
        assert!(ambiguous.message.contains("beta::Object"));
    }
}
