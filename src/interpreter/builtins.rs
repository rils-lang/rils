use super::*;

pub(super) fn install_builtins(environment: &EnvironmentRef) {
    let builtins = [
        NativeFunction {
            binding_name: "#rils_native_print",
            name: "print",
            min_arity: 1,
            max_arity: usize::MAX,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            function: |arguments| {
                let Some(Value::String(format)) = arguments.first() else {
                    return Err("print! requires a format string".into());
                };
                print!(
                    "{}",
                    crate::formatting::format_arguments(format, &arguments[1..])?
                );
                Ok(Value::Unit)
            },
        },
        NativeFunction {
            binding_name: "#rils_native_println",
            name: "println",
            min_arity: 0,
            max_arity: usize::MAX,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            function: |arguments| {
                if arguments.is_empty() {
                    println!();
                    return Ok(Value::Unit);
                }
                let Some(Value::String(format)) = arguments.first() else {
                    return Err("println! requires a format string".into());
                };
                println!(
                    "{}",
                    crate::formatting::format_arguments(format, &arguments[1..])?
                );
                Ok(Value::Unit)
            },
        },
        NativeFunction {
            binding_name: "type_of",
            name: "type_of",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(vec![Type::Unknown], Type::String)),
            function: |arguments| Ok(Value::String(Rc::from(arguments[0].type_name()))),
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
            function: |arguments| match &arguments[0] {
                Value::Reference(reference) => reference.read()?.clone_owned(),
                value => Err(format!(
                    "`clone` expects a reference, found {}; use `clone(&value)`",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "#rils_native_assert",
            name: "assert",
            min_arity: 1,
            max_arity: 2,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            function: |arguments| match arguments[0] {
                Value::Bool(true) => Ok(Value::Unit),
                Value::Bool(false) => {
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
            },
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
            function: |arguments| {
                if arguments[0].contains_reference() {
                    return Err("references cannot be stored in Option".into());
                }
                Ok(Value::Option {
                    value: Some(Rc::new(arguments[0].clone())),
                    element_type: Type::of_value(&arguments[0]),
                })
            },
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
            function: |arguments| {
                if arguments[0].contains_reference() {
                    return Err("references cannot be stored in Result".into());
                }
                Ok(Value::Result {
                    value: Ok(Rc::new(arguments[0].clone())),
                    ok_type: Type::of_value(&arguments[0]),
                    error_type: None,
                })
            },
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
            function: |arguments| {
                if arguments[0].contains_reference() {
                    return Err("references cannot be stored in Result".into());
                }
                Ok(Value::Result {
                    value: Err(Rc::new(arguments[0].clone())),
                    ok_type: None,
                    error_type: Type::of_value(&arguments[0]),
                })
            },
        },
        NativeFunction {
            binding_name: "is_ok",
            name: "is_ok",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(Type::Unknown),
                )],
                Type::Bool,
            )),
            function: |arguments| match &arguments[0] {
                Value::Result { value, .. } => Ok(Value::Bool(value.is_ok())),
                value => Err(format!(
                    "`is_ok` expects Result, found {}",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "is_err",
            name: "is_err",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(Type::Unknown),
                )],
                Type::Bool,
            )),
            function: |arguments| match &arguments[0] {
                Value::Result { value, .. } => Ok(Value::Bool(value.is_err())),
                value => Err(format!(
                    "`is_err` expects Result, found {}",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "is_some",
            name: "is_some",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Option(Box::new(Type::Unknown))],
                Type::Bool,
            )),
            function: |arguments| match &arguments[0] {
                Value::Option { value, .. } => Ok(Value::Bool(value.is_some())),
                value => Err(format!(
                    "`is_some` expects Option, found {}",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "is_none",
            name: "is_none",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Option(Box::new(Type::Unknown))],
                Type::Bool,
            )),
            function: |arguments| match &arguments[0] {
                Value::Option { value, .. } => Ok(Value::Bool(value.is_none())),
                value => Err(format!(
                    "`is_none` expects Option, found {}",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "unwrap",
            name: "unwrap",
            min_arity: 1,
            max_arity: 1,
            signature: Some(FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown)),
            function: |arguments| match &arguments[0] {
                Value::Option {
                    value: Some(value), ..
                } => Ok((**value).clone()),
                Value::Option { value: None, .. } => Err("called `unwrap` on `None`".into()),
                Value::Result {
                    value: Ok(value), ..
                } => Ok((**value).clone()),
                Value::Result {
                    value: Err(value), ..
                } => Err(format!("called `unwrap` on Err({value})")),
                value => Err(format!(
                    "`unwrap` expects Option or Result, found {}",
                    value.type_name()
                )),
            },
        },
        NativeFunction {
            binding_name: "unwrap_or",
            name: "unwrap_or",
            min_arity: 2,
            max_arity: 2,
            signature: Some(FunctionSignature::fixed(
                vec![Type::Unknown, Type::Unknown],
                Type::Unknown,
            )),
            function: |arguments| match &arguments[0] {
                Value::Option {
                    value,
                    element_type,
                } => {
                    if let Some(expected) = element_type
                        && !expected.accepts(&arguments[1])
                    {
                        return Err(format!(
                            "`unwrap_or` default must be {expected}, found {}",
                            arguments[1].type_name()
                        ));
                    }
                    Ok(value
                        .as_ref()
                        .map_or_else(|| arguments[1].clone(), |value| (**value).clone()))
                }
                Value::Result { value, ok_type, .. } => {
                    if let Some(expected) = ok_type
                        && !expected.accepts(&arguments[1])
                    {
                        return Err(format!(
                            "`unwrap_or` default must be {expected}, found {}",
                            arguments[1].type_name()
                        ));
                    }
                    Ok(match value {
                        Ok(value) => (**value).clone(),
                        Err(_) => arguments[1].clone(),
                    })
                }
                value => Err(format!(
                    "`unwrap_or` expects Option or Result, found {}",
                    value.type_name()
                )),
            },
        },
    ];

    environment.borrow_mut().define(
        "None",
        Value::Option {
            value: None,
            element_type: None,
        },
        false,
        None,
    );

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
            Value::NativeFunction(function),
            false,
            None,
        );
    }

    install_builtin_traits(environment);
    install_format_types(environment);
    environment
        .borrow_mut()
        .define("Vec", Value::BuiltinType(BuiltinType::Vec), false, None);
    environment.borrow_mut().define(
        "HashMap",
        Value::BuiltinType(BuiltinType::HashMap),
        false,
        None,
    );
    environment.borrow_mut().define(
        "HashSet",
        Value::BuiltinType(BuiltinType::HashSet),
        false,
        None,
    );
    for integer in crate::IntegerType::ALL {
        environment.borrow_mut().define(
            integer.name(),
            Value::BuiltinType(BuiltinType::Integer(integer)),
            false,
            None,
        );
    }
    for float in [crate::FloatType::F32, crate::FloatType::F64] {
        environment.borrow_mut().define(
            float.name(),
            Value::BuiltinType(BuiltinType::Float(float)),
            false,
            None,
        );
    }
    environment.borrow_mut().define(
        "Range",
        Value::StructType(Rc::new(StructType {
            name: "Range".into(),
            generic_parameters: vec![GenericParameter {
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

fn install_builtin_modules(environment: &EnvironmentRef) {
    let core = builtin_module(environment, "core");
    let std_definition = builtin_module(environment, "std");
    let io_definition = match std_definition.members.borrow().get("io") {
        Some(Value::Module(module)) => module,
        _ => unreachable!("generated std module contains io"),
    };
    crate::standard_library::install(environment, &std_definition, &io_definition);
    let prelude = builtin_module(environment, "prelude");
    environment
        .borrow_mut()
        .define("core", Value::Module(core), false, None);
    environment
        .borrow_mut()
        .define("std", Value::Module(std_definition), false, None);
    environment
        .borrow_mut()
        .define("prelude", Value::Module(prelude), false, None);
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
            Some(Value::Module(builtin_module(environment, &child_path)))
        } else {
            let binding = match (path, member) {
                ("std::io", "print") => "#rils_native_print",
                ("std::io", "println") => "#rils_native_println",
                _ => member,
            };
            environment.borrow().get(binding)
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
    environment.borrow_mut().define(
        "Formatter",
        Value::HostType(Rc::new(HostType {
            name: "Formatter".into(),
            base_types: HashSet::new(),
            copy: false,
            methods: RefCell::new(HashMap::new()),
        })),
        false,
        None,
    );
    environment.borrow_mut().define(
        "FormatError",
        Value::StructType(Rc::new(StructType {
            name: "FormatError".into(),
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

fn install_builtin_traits(environment: &EnvironmentRef) {
    let span = Span::default();
    let self_type = Type::named("Self");
    for declaration in rils_builtins::BUILTINS
        .iter()
        .filter(|declaration| declaration.kind == rils_builtins::BuiltinKind::Trait)
    {
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
                member.required
                    && matches!(
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
                            name: (*name).into(),
                            bounds: Vec::new(),
                            span,
                        })
                        .collect(),
                    parameters,
                    return_type: Some(*return_type),
                    span,
                }
            })
            .collect();
        let definition = TraitType {
            name: declaration.path.into(),
            bounds: Vec::new(),
            associated_types,
            methods,
        };
        let name = definition.name.clone();
        environment
            .borrow_mut()
            .define(name, Value::TraitType(Rc::new(definition)), false, None);
    }
}
