use super::*;

mod sums;

pub(super) fn install_builtins(environment: &EnvironmentRef) {
    let builtins = [
        NativeFunction {
            binding_name: "#rils_native_print",
            name: "print",
            min_arity: 1,
            max_arity: usize::MAX,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            body: NativeFunctionBody::Rust(|arguments| {
                let Some(format) = arguments
                    .first()
                    .and_then(|value| value.get_cloned::<String>().ok())
                else {
                    return Err("print! requires a format string".into());
                };
                print!(
                    "{}",
                    crate::formatting::format_arguments(&format, &arguments[1..])?
                );
                Ok(Value::from(rils_execution::value::native_unit()))
            }),
        },
        NativeFunction {
            binding_name: "#rils_native_println",
            name: "println",
            min_arity: 0,
            max_arity: usize::MAX,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            body: NativeFunctionBody::Rust(|arguments| {
                if arguments.is_empty() {
                    println!();
                    return Ok(Value::from(rils_execution::value::native_unit()));
                }
                let Some(format) = arguments
                    .first()
                    .and_then(|value| value.get_cloned::<String>().ok())
                else {
                    return Err("println! requires a format string".into());
                };
                println!(
                    "{}",
                    crate::formatting::format_arguments(&format, &arguments[1..])?
                );
                Ok(Value::from(rils_execution::value::native_unit()))
            }),
        },
        NativeFunction {
            binding_name: "type_of",
            name: "type_of",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(vec![Type::Unknown], Type::String)),
            body: NativeFunctionBody::Rust(|arguments| {
                Ok(Value::from(rils_execution::value::native_string(
                    arguments[0].type_name(),
                )))
            }),
        },
        NativeFunction {
            binding_name: "clone",
            name: "clone",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Reference {
                    mutable: false,
                    inner: Box::new(Type::Unknown),
                }],
                Type::Unknown,
            )),
            body: NativeFunctionBody::Rust(runtime_clone),
        },
        NativeFunction {
            binding_name: "#rils_native_assert",
            name: "assert",
            min_arity: 1,
            max_arity: 2,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            body: NativeFunctionBody::Rust(|arguments| match arguments[0] {
                ref value if value.get_cloned::<bool>().ok() == Some(true) => {
                    Ok(Value::from(rils_execution::value::native_unit()))
                }
                ref value if value.get_cloned::<bool>().ok() == Some(false) => {
                    let message = arguments
                        .get(1)
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "assertion failed".into());
                    Err(message)
                }
                ref value => Err(format!(
                    "`assert` expects bool, found {}",
                    value.type_name()
                )),
            }),
        },
        NativeFunction {
            binding_name: "Some",
            name: "Some",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Option(Box::new(Type::Unknown)),
            )),
            body: NativeFunctionBody::RustOwned(sums::some),
        },
        NativeFunction {
            binding_name: "Ok",
            name: "Ok",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Result(Box::new(Type::Unknown), Box::new(Type::Unknown)),
            )),
            body: NativeFunctionBody::RustOwned(sums::ok),
        },
        NativeFunction {
            binding_name: "Err",
            name: "Err",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Result(Box::new(Type::Unknown), Box::new(Type::Unknown)),
            )),
            body: NativeFunctionBody::RustOwned(sums::err),
        },
    ];

    for mut function in builtins {
        let declaration_path = match function.binding_name {
            "#rils_native_print" => Some("std::io::print"),
            "#rils_native_println" => Some("std::io::println"),
            "#rils_native_assert" => None,
            _ => Some(function.name),
        };
        if let Some(signature) = declaration_path
            .and_then(rils_frontend::standard_library::erased_standard_function_signature)
        {
            match &signature.parameters {
                Some(parameters) => {
                    function.min_arity = parameters.len();
                    function.max_arity = parameters.len();
                }
                None => {
                    function.min_arity = 0;
                    function.max_arity = usize::MAX;
                }
            }
            function.signature = Some(signature);
        }
        environment.borrow_mut().define(
            function.binding_name,
            Value::from_callable(Rc::new(function)),
            false,
            None,
        );
    }

    for declaration in rils_builtins::BUILTINS.iter().filter(|declaration| {
        declaration.kind == rils_builtins::BuiltinKind::Function
            && declaration.native_symbol.is_some()
    }) {
        let signature =
            rils_frontend::standard_library::erased_standard_function_signature(declaration.path)
                .expect("native standard function has a signature");
        let arity = signature
            .parameters
            .as_ref()
            .expect("native method aliases have fixed parameters")
            .len();
        let symbol = declaration.native_symbol.expect("filtered native symbol");
        environment.borrow_mut().define(
            declaration.path,
            Value::from_callable(Rc::new(NativeFunction {
                binding_name: declaration.path,
                name: declaration.path,
                min_arity: arity,
                max_arity: arity,
                signature: Some(signature),
                body: NativeFunctionBody::Symbol(symbol),
            })),
            false,
            None,
        );
    }

    install_builtin_traits(environment);
    install_format_types(environment);
    environment.borrow_mut().define(
        "Vec",
        crate::value::Declaration::BuiltinType(BuiltinType::Vec),
        false,
        None,
    );
    environment.borrow_mut().define(
        "HashMap",
        crate::value::Declaration::BuiltinType(BuiltinType::HashMap),
        false,
        None,
    );
    environment.borrow_mut().define(
        "HashSet",
        crate::value::Declaration::BuiltinType(BuiltinType::HashSet),
        false,
        None,
    );
    for name in [
        "Box",
        "Rc",
        "Weak",
        "Cell",
        "RefCell",
        "VecDeque",
        "BinaryHeap",
        "BTreeMap",
        "BTreeSet",
    ] {
        environment.borrow_mut().define(
            name,
            crate::value::Declaration::StructType(Rc::new(StructType {
                field_indices: Default::default(),
                name: name.into(),
                opaque_native: rils_builtins::builtin(name)
                    .is_some_and(|declaration| declaration.opaque_native),
                generic_parameters: if name == "BTreeMap" {
                    ["K", "V"]
                        .into_iter()
                        .map(|name| GenericParameter {
                            is_const: false,
                            name: name.into(),
                            bounds: Vec::new(),
                            span: Span::default(),
                        })
                        .collect()
                } else {
                    vec![GenericParameter {
                        is_const: false,
                        name: "T".into(),
                        bounds: Vec::new(),
                        span: Span::default(),
                    }]
                },
                fields: Vec::new(),
                methods: Default::default(),
                trait_methods: Default::default(),
                implemented_traits: RefCell::new(HashSet::new()),
                associated_types: RefCell::new(HashMap::new()),
            })),
            false,
            None,
        );
    }
    for integer in crate::IntegerType::ALL {
        environment.borrow_mut().define(
            integer.name(),
            crate::value::Declaration::BuiltinType(BuiltinType::Integer(integer)),
            false,
            None,
        );
    }
    for float in [crate::FloatType::F32, crate::FloatType::F64] {
        environment.borrow_mut().define(
            float.name(),
            crate::value::Declaration::BuiltinType(BuiltinType::Float(float)),
            false,
            None,
        );
    }
    environment.borrow_mut().define(
        "Range",
        crate::value::Declaration::StructType(Rc::new(StructType {
            field_indices: Default::default(),
            name: "Range".into(),
            opaque_native: rils_builtins::builtin("Range")
                .is_some_and(|declaration| declaration.opaque_native),
            generic_parameters: vec![GenericParameter {
                is_const: false,
                name: "T".into(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
            methods: Default::default(),
            trait_methods: Default::default(),
            implemented_traits: RefCell::new(
                ["Iterator".to_string(), "IntoIterator".to_string()]
                    .into_iter()
                    .collect(),
            ),
            associated_types: RefCell::new(HashMap::from([
                (
                    "Iterator".to_string(),
                    HashMap::from([(
                        "Item".to_string(),
                        TypeAliasType {
                            name: "Item".into(),
                            generic_parameters: Vec::new(),
                            target: Type::Variable("T".into()),
                        },
                    )]),
                ),
                (
                    "IntoIterator".to_string(),
                    HashMap::from([(
                        "IntoIter".to_string(),
                        TypeAliasType {
                            name: "IntoIter".into(),
                            generic_parameters: Vec::new(),
                            target: Type::Named {
                                name: "Range".into(),
                                arguments: vec![Type::Variable("T".into())],
                            },
                        },
                    )]),
                ),
            ])),
        })),
        false,
        None,
    );
    install_builtin_modules(environment);
}

fn runtime_clone(arguments: &[Value]) -> Result<Value, String> {
    let symbol = rils_builtins::builtin_member("Clone", "clone")
        .and_then(|member| member.native_symbol)
        .expect("Clone::clone exports a native symbol");
    crate::runtime_builtins::call_native_symbol(symbol, arguments)
        .expect("Clone::clone native adapter is registered")
}

fn install_builtin_modules(environment: &EnvironmentRef) {
    let core = builtin_module(environment, "core");
    let std_definition = builtin_module(environment, "std");
    let io_definition = match std_definition.members.borrow().get_declaration("io") {
        Some(crate::value::Declaration::Module(module)) => module,
        _ => unreachable!("generated std module contains io"),
    };
    crate::standard_library::install(environment, &std_definition, &io_definition);
    let prelude = builtin_module(environment, "prelude");
    environment
        .borrow_mut()
        .define("core", crate::value::Declaration::Module(core), false, None);
    environment.borrow_mut().define(
        "std",
        crate::value::Declaration::Module(std_definition),
        false,
        None,
    );
    environment.borrow_mut().define(
        "prelude",
        crate::value::Declaration::Module(prelude),
        false,
        None,
    );
}

fn builtin_module(environment: &EnvironmentRef, path: &str) -> Rc<ModuleValue> {
    let members = Environment::module_child(environment.clone());
    let mut public = HashSet::new();
    for &member in rils_builtins::builtin_module_members(path) {
        let child_path = format!("{path}::{member}");
        let value = if rils_builtins::BUILTIN_MODULES
            .iter()
            .any(|module| module.path == child_path)
        {
            Some(crate::value::Binding::Declaration(
                crate::value::Declaration::Module(builtin_module(environment, &child_path)),
            ))
        } else {
            let binding = match (path, member) {
                ("std::io", "print") => "#rils_native_print",
                ("std::io", "println") => "#rils_native_println",
                _ => member,
            };
            environment
                .borrow()
                .binding(binding)
                .or_else(|| environment.borrow().binding(&child_path))
        };
        if let Some(value) = value {
            public.insert(member.to_owned());
            members.borrow_mut().define(member, value, false, None);
        }
    }
    Rc::new(ModuleValue {
        name: path.rsplit("::").next().unwrap_or(path).into(),
        members,
        public: RefCell::new(public),
    })
}

fn install_format_types(environment: &EnvironmentRef) {
    for name in ["Formatter", "FormatError"] {
        environment.borrow_mut().define(
            name,
            crate::value::Declaration::StructType(Rc::new(StructType {
                field_indices: Default::default(),
                name: name.into(),
                opaque_native: rils_builtins::builtin(name)
                    .is_some_and(|declaration| declaration.opaque_native),
                generic_parameters: Vec::new(),
                fields: Vec::new(),
                methods: Default::default(),
                trait_methods: Default::default(),
                implemented_traits: Default::default(),
                associated_types: Default::default(),
            })),
            false,
            None,
        );
    }
}

fn install_builtin_traits(environment: &EnvironmentRef) {
    let span = Span::default();
    let self_type = Type::named("Self");
    for declaration in rils_builtins::BUILTINS
        .iter()
        .filter(|declaration| declaration.kind == rils_builtins::BuiltinKind::Trait)
    {
        let source_methods = declaration
            .source
            .map(|source| {
                let tokens = rils_frontend::lexer::lex(source)
                    .expect("generated built-in trait source must lex");
                rils_frontend::parser::parse_builtin_declarations(tokens)
                    .expect("generated built-in trait source must parse")
            })
            .and_then(|program| {
                program
                    .statements
                    .into_iter()
                    .find_map(|statement| match statement {
                        crate::ast::Stmt::Trait { methods, .. } => Some(methods),
                        _ => None,
                    })
            })
            .unwrap_or_default();
        let associated_types = declaration
            .members
            .iter()
            .filter(|member| member.kind == rils_builtins::BuiltinMemberKind::AssociatedType)
            .map(|member| AssociatedType {
                name: member.name.into(),
                name_span: span,
                generic_parameters: Vec::new(),
                value: None,
                span,
            })
            .collect();
        let methods = declaration
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member.kind,
                    rils_builtins::BuiltinMemberKind::Method
                        | rils_builtins::BuiltinMemberKind::AssociatedFunction
                )
            })
            .map(|member| {
                let Type::Function {
                    parameters: Some(parameters),
                    return_type,
                } = rils_frontend::standard_library::builtin_trait_member_type(
                    declaration.path,
                    &self_type,
                    member.name,
                )
                .expect("built-in trait methods have fixed signatures")
                else {
                    unreachable!("built-in trait methods have fixed signatures");
                };
                let mut parameters = parameters
                    .into_iter()
                    .enumerate()
                    .map(|(index, type_annotation)| Parameter {
                        name: format!("argument{index}"),
                        mutable: false,
                        type_annotation: Some(type_annotation),
                        span,
                    })
                    .collect::<Vec<_>>();
                if let Some(receiver) = member.receiver {
                    let type_annotation = match receiver {
                        rils_builtins::ReceiverMode::Owned => self_type.clone(),
                        rils_builtins::ReceiverMode::Shared => Type::Reference {
                            mutable: false,
                            inner: Box::new(self_type.clone()),
                        },
                        rils_builtins::ReceiverMode::Mutable => Type::Reference {
                            mutable: true,
                            inner: Box::new(self_type.clone()),
                        },
                    };
                    parameters.insert(
                        0,
                        Parameter {
                            name: "self".into(),
                            mutable: false,
                            type_annotation: Some(type_annotation),
                            span,
                        },
                    );
                }
                TraitMethod {
                    attributes: Vec::new(),
                    name: member.name.into(),
                    name_span: span,
                    generic_parameters: member
                        .type_parameters
                        .iter()
                        .map(|name| GenericParameter {
                            is_const: false,
                            name: (*name).into(),
                            bounds: Vec::new(),
                            span,
                        })
                        .collect(),
                    parameters,
                    return_type: Some(*return_type),
                    body: source_methods
                        .iter()
                        .find(|method| method.name == member.name)
                        .and_then(|method| method.body.clone()),
                    span,
                }
            })
            .collect();
        let definition = TraitType {
            name: declaration.path.into(),
            generic_parameters: declaration
                .type_parameters
                .iter()
                .map(|name| GenericParameter {
                    is_const: false,
                    name: (*name).into(),
                    bounds: Vec::new(),
                    span: Span::default(),
                })
                .collect(),
            bounds: declaration
                .supertraits
                .iter()
                .map(|bound| (*bound).into())
                .collect(),
            associated_types,
            methods,
        };
        let name = definition.name.clone();
        environment.borrow_mut().define(
            name,
            crate::value::Declaration::TraitType(Rc::new(definition)),
            false,
            None,
        );
    }
}
