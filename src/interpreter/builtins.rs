use super::*;

pub(super) fn install_builtins(environment: &EnvironmentRef) {
    let builtins = [
        NativeFunction {
            binding_name: "#rils_native_print",
            name: "print",
            min_arity: 0,
            max_arity: usize::MAX,
            signature: Some(FunctionSignature::variadic(Type::Unit)),
            function: |arguments| {
                for value in arguments {
                    print!("{value}");
                }
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
                for (index, value) in arguments.iter().enumerate() {
                    if index > 0 {
                        print!(" ");
                    }
                    print!("{value}");
                }
                println!();
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

    for function in builtins {
        environment.borrow_mut().define(
            function.binding_name,
            Value::NativeFunction(function),
            false,
            None,
        );
    }

    install_builtin_traits(environment);
    environment
        .borrow_mut()
        .define("Vec", Value::BuiltinType(BuiltinType::Vec), false, None);
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
    let module = |name: &str, values: Vec<(&str, Value)>| {
        let members = Environment::child(environment.clone());
        let mut public = std::collections::HashSet::new();
        for (member, value) in values {
            public.insert(member.to_string());
            members.borrow_mut().define(member, value, false, None);
        }
        Value::Module(Rc::new(ModuleValue {
            name: name.into(),
            members,
            public: RefCell::new(public),
        }))
    };

    let get = |name: &str| {
        environment
            .borrow()
            .get(name)
            .expect("builtin module member exists")
    };
    let option = module(
        "option",
        vec![
            ("Some", get("Some")),
            ("None", get("None")),
            ("is_some", get("is_some")),
            ("is_none", get("is_none")),
            ("unwrap", get("unwrap")),
            ("unwrap_or", get("unwrap_or")),
        ],
    );
    let result = module(
        "result",
        vec![
            ("Ok", get("Ok")),
            ("Err", get("Err")),
            ("is_ok", get("is_ok")),
            ("is_err", get("is_err")),
            ("unwrap", get("unwrap")),
            ("unwrap_or", get("unwrap_or")),
        ],
    );
    let iter = module(
        "iter",
        vec![
            ("Iterator", get("Iterator")),
            ("IntoIterator", get("IntoIterator")),
            ("Range", get("Range")),
        ],
    );
    let clone = module(
        "clone",
        vec![
            ("Clone", get("Clone")),
            ("Copy", get("Copy")),
            ("clone", get("clone")),
        ],
    );
    let core = module(
        "core",
        vec![
            ("option", option),
            ("result", result),
            ("iter", iter),
            ("clone", clone),
        ],
    );

    let collections = module("collections", vec![("Vec", get("Vec"))]);
    let io = module(
        "io",
        vec![
            ("print", get("#rils_native_print")),
            ("println", get("#rils_native_println")),
        ],
    );
    let io_definition = match &io {
        Value::Module(module) => module.clone(),
        _ => unreachable!("io is a module"),
    };
    let std = module("std", vec![("collections", collections), ("io", io)]);
    let std_definition = match &std {
        Value::Module(module) => module.clone(),
        _ => unreachable!("std is a module"),
    };
    crate::standard_library::install(environment, &std_definition, &io_definition);
    let prelude = module(
        "prelude",
        vec![
            ("Some", get("Some")),
            ("None", get("None")),
            ("Ok", get("Ok")),
            ("Err", get("Err")),
            ("Vec", get("Vec")),
            ("Copy", get("Copy")),
            ("Clone", get("Clone")),
            ("Iterator", get("Iterator")),
            ("IntoIterator", get("IntoIterator")),
        ],
    );
    environment.borrow_mut().define("core", core, false, None);
    environment.borrow_mut().define("std", std, false, None);
    environment
        .borrow_mut()
        .define("prelude", prelude, false, None);
}

fn install_builtin_traits(environment: &EnvironmentRef) {
    let span = Span::default();
    let self_type = Type::named("Self");
    let shared_self = Type::Reference {
        mutable: false,
        inner: Box::new(self_type.clone()),
    };
    let mutable_self = Type::Reference {
        mutable: true,
        inner: Box::new(self_type.clone()),
    };
    let traits = [
        TraitType {
            name: "Copy".into(),
            associated_types: Vec::new(),
            methods: Vec::new(),
        },
        TraitType {
            name: "Clone".into(),
            associated_types: Vec::new(),
            methods: vec![TraitMethod {
                name: "clone".into(),
                name_span: span,
                generic_parameters: Vec::new(),
                parameters: vec![Parameter {
                    name: "self".into(),
                    mutable: false,
                    type_annotation: Some(shared_self),
                    span,
                }],
                return_type: Some(self_type.clone()),
                span,
            }],
        },
        TraitType {
            name: "Iterator".into(),
            associated_types: vec![AssociatedType {
                name: "Item".into(),
                name_span: span,
                generic_parameters: Vec::new(),
                value: None,
                span,
            }],
            methods: vec![TraitMethod {
                name: "next".into(),
                name_span: span,
                generic_parameters: Vec::new(),
                parameters: vec![Parameter {
                    name: "self".into(),
                    mutable: false,
                    type_annotation: Some(mutable_self),
                    span,
                }],
                return_type: Some(Type::Option(Box::new(Type::Associated {
                    base: Box::new(self_type.clone()),
                    trait_name: Some("Iterator".into()),
                    name: "Item".into(),
                    arguments: Vec::new(),
                }))),
                span,
            }],
        },
        TraitType {
            name: "IntoIterator".into(),
            associated_types: vec![AssociatedType {
                name: "IntoIter".into(),
                name_span: span,
                generic_parameters: Vec::new(),
                value: None,
                span,
            }],
            methods: vec![TraitMethod {
                name: "into_iter".into(),
                name_span: span,
                generic_parameters: Vec::new(),
                parameters: vec![Parameter {
                    name: "self".into(),
                    mutable: false,
                    type_annotation: Some(self_type.clone()),
                    span,
                }],
                return_type: Some(Type::Associated {
                    base: Box::new(self_type),
                    trait_name: Some("IntoIterator".into()),
                    name: "IntoIter".into(),
                    arguments: Vec::new(),
                }),
                span,
            }],
        },
    ];

    for definition in traits {
        let name = definition.name.clone();
        environment
            .borrow_mut()
            .define(name, Value::TraitType(Rc::new(definition)), false, None);
    }
}
