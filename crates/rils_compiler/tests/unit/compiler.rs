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
fn hir_lowering_distinguishes_calls_with_the_same_span() {
    let tokens = rils_frontend::lexer::lex(
        "let value: Option<i32> = Some(1); value.is_some(); value.unwrap();",
    )
    .expect("lex calls");
    let mut program = rils_frontend::parser::parse(tokens).expect("parse calls");
    let first_span = match &program.statements[1] {
        rils_frontend::ast::Stmt::Expr {
            expression: rils_frontend::ast::Expr::Call { span, .. },
            ..
        } => *span,
        _ => panic!("expected first call"),
    };
    match &mut program.statements[2] {
        rils_frontend::ast::Stmt::Expr {
            expression: rils_frontend::ast::Expr::Call { span, .. },
            ..
        } => *span = first_span,
        _ => panic!("expected second call"),
    }
    let analysis = rils_frontend::analysis::analyze_program(&program);

    let hir =
        crate::hir::lower_with_host(&program, &HostContract::new(), &analysis, Vec::new(), None)
            .expect("lower calls by expression identity");
    let builtins = hir.functions[0]
        .statements
        .iter()
        .filter_map(|statement| match statement {
            crate::hir::HirStatement::Expression {
                expression: crate::hir::HirExpression::CallRuntime { builtin, .. },
                ..
            } => Some(*builtin),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        builtins,
        [
            rils_builtins::BuiltinId::OptionIsSome,
            rils_builtins::BuiltinId::OptionUnwrap,
        ]
    );
}

#[test]
fn resolved_definition_selects_between_same_named_inherent_methods() {
    compile(
        r#"
            struct Left { value: i32 }
            struct Right { value: i32 }

            impl Left {
                fn read(&self) -> i32 { self.value }
            }

            impl Right {
                fn read(&self) -> i32 { self.value }
            }

            let left = Left { value: 1 };
            left.read()
        "#,
    )
    .expect("the resolved method definition should avoid name-only ambiguity");
}

#[test]
fn semantic_call_identity_covers_trait_ufcs_modules_and_imports() {
    compile(
        r#"
            trait Read {
                fn read(&self) -> i32;
            }

            struct Left { value: i32 }
            struct Right { value: i32 }
            struct Owned { value: i32 }

            impl Owned {
                fn read(self) -> i32 { self.value }
            }

            impl Read for Left {
                fn read(&self) -> i32 { self.value }
            }

            impl Read for Right {
                fn read(&self) -> i32 { self.value }
            }

            mod values {
                pub fn answer() -> i32 { 42 }

                pub fn local_answer() -> i32 {
                    answer()
                }
            }

            use values::answer as imported_answer;

            let left = Left { value: 1 };
            left.read();
            <Left as Read>::read(&left);
            values::local_answer();
            imported_answer();

            let module_function = values::answer;
            module_function();
            let imported_function = imported_answer;
            imported_function();
            let owned = Owned { value: 2 };
            let bound_method = owned.read;
            bound_method();
            let trait_function = <Left as Read>::read;
            trait_function(&left)
        "#,
    )
    .expect("frontend identities should select every user-defined direct call");
}

#[test]
fn host_enum_variants_are_real_extensible_rils_enums() {
    let mut host = HostContract::new();
    host.register_enum_type(
        "unity_engine::CameraType",
        IntegerType::I32,
        false,
        [("Game".to_owned(), 1), ("SceneView".to_owned(), 2)],
    )
    .unwrap();

    compile_with_host(
        r#"
            use unity_engine::CameraType;
            impl CameraType {
                fn is_game(&self) -> bool {
                    match self {
                        CameraType::Game => true,
                        CameraType::SceneView => false,
                    }
                }
            }
            CameraType::Game.is_game()
        "#,
        &host,
    )
    .expect("host enums should use normal enum construction, matching, and impls");
}

#[test]
fn flags_host_enums_automatically_implement_bit_flags() {
    let mut host = HostContract::new();
    host.register_enum_type(
        "unity_engine::HideFlags",
        IntegerType::I32,
        true,
        [("None".to_owned(), 0), ("HideInHierarchy".to_owned(), 1)],
    )
    .unwrap();

    compile_with_host(
        r#"
            use unity_engine::HideFlags;
            fn accepts_flags<T: BitFlags>(value: T) -> T { value }
            accepts_flags(HideFlags::HideInHierarchy)
        "#,
        &host,
    )
    .expect("flags enums should satisfy the built-in BitFlags bound");
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
fn preserves_inferred_host_receiver_types_through_local_bindings() {
    let mut host = HostContract::new();
    for name in ["unity_engine::GameObject", "unity_engine::Transform"] {
        host.register_type(name, None::<&str>, HostTypeTransport::HostHandle)
            .unwrap();
    }
    host.register_value_type("unity_engine::Vector3", HostValueLayout::F32x3)
        .unwrap();
    for (id, name, receiver, return_type) in [
        (
            930,
            "unity_engine::game_object::transform",
            "unity_engine::GameObject",
            Type::named("unity_engine::Transform"),
        ),
        (
            931,
            "unity_engine::transform::local_position",
            "unity_engine::Transform",
            Type::named("unity_engine::Vector3"),
        ),
        (
            932,
            "unity_engine::Vector3::x",
            "unity_engine::Vector3",
            Type::Float(FloatType::F32),
        ),
    ] {
        host.register_function_with_options_and_receiver(
            id,
            name,
            FunctionSignature::fixed(vec![Type::named(receiver)], return_type),
            "unity.generated",
            crate::HostCallKind::Direct,
            crate::HostThreadAffinity::MainThread,
            Some(HostReceiver::Ref),
        )
        .unwrap();
    }
    for (id, parameter_count) in [(933, 2), (934, 3)] {
        host.register_function(
            id,
            "unity_engine::Vector3::new",
            FunctionSignature::fixed(
                vec![Type::Float(FloatType::F32); parameter_count],
                Type::named("unity_engine::Vector3"),
            ),
            "unity.generated",
        )
        .unwrap();
    }

    compile_with_host(
        "fn read(go: unity_engine::GameObject) -> f32 { \
         let transform = go.transform(); \
         let position = transform.local_position(); \
         position.x() }",
        &host,
    )
    .expect("inferred host return types should remain available during HIR lowering");

    compile_with_host(
        "let position = unity_engine::Vector3::new(1.0f32, 2.0f32, 3.0f32); \
         position.x();",
        &host,
    )
    .expect("a common overload return type should flow into a local receiver");
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
