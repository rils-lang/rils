use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Write,
    rc::Rc,
};

use crate::{
    ast::{EnumVariant, NamedField},
    environment::{Environment, EnvironmentRef},
    source::Span,
    types::Type,
    value::{
        EnumInstance, EnumPayload, EnumType, FieldSlot, HostFunction, ModuleValue, SequenceValue,
        StructInstance, StructType, Value,
    },
};

pub(super) fn install(
    environment: &EnvironmentRef,
    std_module: &Rc<ModuleValue>,
    io_module: &Rc<ModuleValue>,
) {
    let error_kind_declaration = rils_builtins::builtin("std::io::ErrorKind")
        .expect("std::io::ErrorKind is declared in rils_builtins");
    let error_kind = Rc::new(EnumType {
        name: "std::io::ErrorKind".into(),
        generic_parameters: Vec::new(),
        variants: error_kind_declaration
            .members
            .iter()
            .filter(|member| member.kind == rils_builtins::BuiltinMemberKind::Variant)
            .map(|member| EnumVariant::Unit {
                name: member.name.into(),
                span: Span::default(),
            })
            .collect(),
        methods: RefCell::new(HashMap::new()),
        trait_methods: RefCell::new(HashMap::new()),
        implemented_traits: RefCell::new(HashSet::new()),
        associated_types: RefCell::new(HashMap::new()),
    });
    let error_declaration = rils_builtins::builtin("std::io::Error")
        .expect("std::io::Error is declared in rils_builtins");
    let error = Rc::new(StructType {
        name: "std::io::Error".into(),
        generic_parameters: Vec::new(),
        fields: error_declaration
            .members
            .iter()
            .filter(|member| member.kind == rils_builtins::BuiltinMemberKind::Field)
            .map(|member| NamedField {
                name: member.name.into(),
                type_annotation: rils_frontend::standard_library::resolve_type_pattern(
                    member.value_type.expect("built-in fields have a type"),
                ),
                span: Span::default(),
            })
            .collect(),
        methods: RefCell::new(HashMap::new()),
        trait_methods: RefCell::new(HashMap::new()),
        implemented_traits: RefCell::new(HashSet::new()),
        associated_types: RefCell::new(HashMap::new()),
    });

    publish(io_module, "ErrorKind", Value::EnumType(error_kind.clone()));
    publish(io_module, "Error", Value::StructType(error.clone()));
    install_io_functions(io_module, error.clone(), error_kind.clone());

    let fs_module = Rc::new(ModuleValue {
        name: "fs".into(),
        members: Environment::module_child(environment.clone()),
        public: RefCell::new(HashSet::new()),
    });
    install_fs_functions(&fs_module, error, error_kind);
    publish(std_module, "fs", Value::Module(fs_module));
}

fn install_io_functions(module: &Rc<ModuleValue>, error: Rc<StructType>, error_kind: Rc<EnumType>) {
    publish(
        module,
        "read_line",
        host_function("std::io::read_line", 0, 0, {
            let error = error.clone();
            let error_kind = error_kind.clone();
            move |_| {
                let mut line = String::new();
                match std::io::stdin().read_line(&mut line) {
                    Ok(_) => Ok(result_ok(Value::String(Rc::from(line)), Type::String)),
                    Err(source) => Ok(result_error(
                        &error,
                        &error_kind,
                        source,
                        None,
                        Type::String,
                    )),
                }
            }
        }),
    );
    publish(
        module,
        "write",
        host_function("std::io::write", 1, 1, {
            let error = error.clone();
            let error_kind = error_kind.clone();
            move |arguments| {
                let text = arguments[0].to_string();
                let mut stdout = std::io::stdout().lock();
                match stdout.write_all(text.as_bytes()) {
                    Ok(()) => Ok(result_ok(Value::Unit, Type::Unit)),
                    Err(source) => Ok(result_error(&error, &error_kind, source, None, Type::Unit)),
                }
            }
        }),
    );
    publish(
        module,
        "write_line",
        host_function("std::io::write_line", 1, 1, {
            let error = error.clone();
            let error_kind = error_kind.clone();
            move |arguments| {
                let text = format!("{}\n", arguments[0]);
                let mut stdout = std::io::stdout().lock();
                match stdout.write_all(text.as_bytes()) {
                    Ok(()) => Ok(result_ok(Value::Unit, Type::Unit)),
                    Err(source) => Ok(result_error(&error, &error_kind, source, None, Type::Unit)),
                }
            }
        }),
    );
    publish(
        module,
        "flush",
        host_function("std::io::flush", 0, 0, move |_| {
            match std::io::stdout().lock().flush() {
                Ok(()) => Ok(result_ok(Value::Unit, Type::Unit)),
                Err(source) => Ok(result_error(&error, &error_kind, source, None, Type::Unit)),
            }
        }),
    );
}

fn install_fs_functions(module: &Rc<ModuleValue>, error: Rc<StructType>, error_kind: Rc<EnumType>) {
    publish(
        module,
        "read_to_string",
        fs_function(
            "read_to_string",
            Type::String,
            error.clone(),
            error_kind.clone(),
            |path| std::fs::read_to_string(path).map(|text| Value::String(Rc::from(text))),
        ),
    );
    publish(
        module,
        "write",
        fs_text_function("write", error.clone(), error_kind.clone(), |path, text| {
            std::fs::write(path, text)
        }),
    );
    publish(
        module,
        "append",
        fs_text_function("append", error.clone(), error_kind.clone(), |path, text| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?
                .write_all(text.as_bytes())
        }),
    );
    publish(
        module,
        "try_exists",
        fs_function(
            "try_exists",
            Type::Bool,
            error.clone(),
            error_kind.clone(),
            |path| path.try_exists().map(Value::Bool),
        ),
    );
    publish(
        module,
        "create_dir_all",
        fs_unit_function(
            "create_dir_all",
            error.clone(),
            error_kind.clone(),
            |path| std::fs::create_dir_all(path),
        ),
    );
    publish(
        module,
        "remove_file",
        fs_unit_function("remove_file", error.clone(), error_kind.clone(), |path| {
            std::fs::remove_file(path)
        }),
    );
    publish(
        module,
        "remove_dir",
        fs_unit_function("remove_dir", error.clone(), error_kind.clone(), |path| {
            std::fs::remove_dir(path)
        }),
    );
    publish(
        module,
        "read_dir",
        fs_function(
            "read_dir",
            Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::String],
            },
            error,
            error_kind,
            |path| {
                let mut paths = std::fs::read_dir(path)?
                    .map(|entry| entry.map(|entry| entry.path().to_string_lossy().into_owned()))
                    .collect::<Result<Vec<_>, _>>()?;
                paths.sort();
                Ok(string_vec(paths))
            },
        ),
    );
}

fn fs_function<F>(
    name: &'static str,
    ok_type: Type,
    error: Rc<StructType>,
    error_kind: Rc<EnumType>,
    operation: F,
) -> Value
where
    F: Fn(&std::path::Path) -> std::io::Result<Value> + 'static,
{
    host_function(&format!("std::fs::{name}"), 1, 1, move |arguments| {
        let path = string_argument(arguments, 0, name)?;
        match operation(std::path::Path::new(path)) {
            Ok(value) => Ok(result_ok(value, ok_type.clone())),
            Err(source) => Ok(result_error(
                &error,
                &error_kind,
                source,
                Some(path),
                ok_type.clone(),
            )),
        }
    })
}

fn fs_text_function<F>(
    name: &'static str,
    error: Rc<StructType>,
    error_kind: Rc<EnumType>,
    operation: F,
) -> Value
where
    F: Fn(&std::path::Path, &str) -> std::io::Result<()> + 'static,
{
    host_function(&format!("std::fs::{name}"), 2, 2, move |arguments| {
        let path = string_argument(arguments, 0, name)?;
        let text = string_argument(arguments, 1, name)?;
        match operation(std::path::Path::new(path), text) {
            Ok(()) => Ok(result_ok(Value::Unit, Type::Unit)),
            Err(source) => Ok(result_error(
                &error,
                &error_kind,
                source,
                Some(path),
                Type::Unit,
            )),
        }
    })
}

fn fs_unit_function<F>(
    name: &'static str,
    error: Rc<StructType>,
    error_kind: Rc<EnumType>,
    operation: F,
) -> Value
where
    F: Fn(&std::path::Path) -> std::io::Result<()> + 'static,
{
    fs_function(name, Type::Unit, error, error_kind, move |path| {
        operation(path).map(|()| Value::Unit)
    })
}

fn host_function(
    name: &str,
    min_arity: usize,
    max_arity: usize,
    function: impl Fn(&[Value]) -> Result<Value, String> + 'static,
) -> Value {
    Value::HostFunction(Rc::new(HostFunction {
        name: name.into(),
        min_arity,
        max_arity,
        signature: rils_frontend::standard_library::standard_function_signature(name),
        function: Rc::new(function),
    }))
}

pub(crate) fn bytecode_host_functions() -> Vec<(String, Rc<HostFunction>)> {
    let environment = Environment::global();
    let io = Rc::new(ModuleValue {
        name: "io".into(),
        members: Environment::module_child(environment.clone()),
        public: RefCell::new(HashSet::new()),
    });
    let std = Rc::new(ModuleValue {
        name: "std".into(),
        members: Environment::module_child(environment),
        public: RefCell::new(HashSet::new()),
    });
    install(&Environment::global(), &std, &io);

    let mut functions = Vec::new();
    let Some(Value::Module(fs)) = std.members.borrow().get("fs") else {
        return functions;
    };
    for declaration in rils_builtins::BUILTINS {
        let rils_builtins::BuiltinBackend::Host(_) = declaration.backend else {
            continue;
        };
        let Some((module, name)) = declaration.path.rsplit_once("::") else {
            continue;
        };
        let source = match module {
            "std::io" => &io,
            "std::fs" => &fs,
            _ => continue,
        };
        if let Some(Value::HostFunction(function)) = source.members.borrow().get(name) {
            functions.push((declaration.path.to_owned(), function));
        }
    }
    functions
}

fn publish(module: &Rc<ModuleValue>, name: &str, value: Value) {
    module.members.borrow_mut().define(name, value, false, None);
    module.public.borrow_mut().insert(name.into());
}

fn string_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    function: &str,
) -> Result<&'a str, String> {
    match &arguments[index] {
        Value::String(value) => Ok(value),
        value => Err(format!(
            "std::fs::{function} argument {} must be string, found {}",
            index + 1,
            value.type_name()
        )),
    }
}

fn result_ok(value: Value, ok_type: Type) -> Value {
    Value::Result {
        value: Ok(Rc::new(value)),
        ok_type: Some(ok_type),
        error_type: Some(Type::named("std::io::Error")),
    }
}

fn result_error(
    definition: &Rc<StructType>,
    kind_definition: &Rc<EnumType>,
    source: std::io::Error,
    path: Option<&str>,
    ok_type: Type,
) -> Value {
    let error = io_error(definition, kind_definition, &source, path);
    Value::Result {
        value: Err(Rc::new(error)),
        ok_type: Some(ok_type),
        error_type: Some(Type::named("std::io::Error")),
    }
}

fn io_error(
    definition: &Rc<StructType>,
    kind_definition: &Rc<EnumType>,
    source: &std::io::Error,
    path: Option<&str>,
) -> Value {
    let kind = error_kind_name(source.kind());
    let fields = HashMap::from([
        (
            "kind".into(),
            FieldSlot {
                value: Some(Value::Enum(Rc::new(EnumInstance {
                    type_definition: kind_definition.clone(),
                    variant: kind.into(),
                    payload: EnumPayload::Unit,
                    type_arguments: Vec::new(),
                }))),
                type_annotation: Type::named("std::io::ErrorKind"),
                references: 0,
            },
        ),
        (
            "message".into(),
            FieldSlot {
                value: Some(Value::String(Rc::from(source.to_string()))),
                type_annotation: Type::String,
                references: 0,
            },
        ),
        (
            "path".into(),
            FieldSlot {
                value: Some(Value::Option {
                    value: path.map(|path| Rc::new(Value::String(Rc::from(path)))),
                    element_type: Some(Type::String),
                }),
                type_annotation: Type::Option(Box::new(Type::String)),
                references: 0,
            },
        ),
    ]);
    Value::Struct(Rc::new(StructInstance {
        type_definition: definition.clone(),
        fields: RefCell::new(fields),
        type_arguments: Vec::new(),
    }))
}

fn string_vec(values: Vec<String>) -> Value {
    Value::Vec(Rc::new(SequenceValue {
        elements: RefCell::new(
            values
                .into_iter()
                .map(|value| FieldSlot {
                    value: Some(Value::String(Rc::from(value))),
                    type_annotation: Type::String,
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(Some(Type::String)),
    }))
}

fn error_kind_name(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "NotFound",
        std::io::ErrorKind::PermissionDenied => "PermissionDenied",
        std::io::ErrorKind::AlreadyExists => "AlreadyExists",
        std::io::ErrorKind::InvalidInput => "InvalidInput",
        std::io::ErrorKind::InvalidData => "InvalidData",
        std::io::ErrorKind::TimedOut => "TimedOut",
        std::io::ErrorKind::Interrupted => "Interrupted",
        std::io::ErrorKind::UnexpectedEof => "UnexpectedEof",
        std::io::ErrorKind::WriteZero => "WriteZero",
        _ => "Other",
    }
}
