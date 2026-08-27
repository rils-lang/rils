use super::runtime_builtins::call as call_runtime_builtin;
use super::*;

pub(super) fn core_imports() -> Vec<(&'static str, FunctionSignature)> {
    vec![
        (
            "type_of",
            FunctionSignature::fixed(vec![Type::Unknown], Type::String),
        ),
        (
            "is_ok",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_err",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_some",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_none",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "clone",
            FunctionSignature::fixed(
                vec![Type::Reference {
                    mutable: false,
                    inner: Box::new(Type::Unknown),
                }],
                Type::Unknown,
            ),
        ),
        (
            "unwrap",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown),
        ),
        (
            "unwrap_or",
            FunctionSignature::fixed(vec![Type::Unknown, Type::Unknown], Type::Unknown),
        ),
        ("core::assert", FunctionSignature::variadic(Type::Unit)),
        (
            "core::vec::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::vec::from",
            FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::hash_map::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "HashMap".into(),
                    arguments: vec![Type::Unknown, Type::Unknown],
                },
            ),
        ),
        (
            "core::hash_set::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "HashSet".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
    ]
}

pub(super) fn call_core_import(name: &str, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    match name {
        "type_of" => Ok(Value::String(Rc::from(arguments[0].type_name()))),
        "clone" => call_runtime_builtin(BuiltinId::Clone, arguments),
        "is_ok" => call_runtime_builtin(BuiltinId::ResultIsOk, arguments),
        "is_err" => call_runtime_builtin(BuiltinId::ResultIsErr, arguments),
        "is_some" => call_runtime_builtin(BuiltinId::OptionIsSome, arguments),
        "is_none" => call_runtime_builtin(BuiltinId::OptionIsNone, arguments),
        "unwrap" => call_runtime_builtin(BuiltinId::OptionUnwrap, arguments),
        "unwrap_or" => call_runtime_builtin(BuiltinId::OptionUnwrapOr, arguments),
        "core::assert" => match arguments.first() {
            Some(Value::Bool(true)) => Ok(Value::Unit),
            Some(Value::Bool(false)) => Err(arguments
                .get(1)
                .map(ToString::to_string)
                .unwrap_or_else(|| "assertion failed".into())),
            Some(value) => Err(format!(
                "`assert` expects bool, found {}",
                value.type_name()
            )),
            None => Err("`assert` expects at least one argument".into()),
        },
        "core::vec::new" => Ok(Value::Vec(Rc::new(SequenceValue {
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        }))),
        "core::hash_map::new" => Ok(Value::HashMap(Rc::new(HashMapValue {
            entries: RefCell::new(HashMap::new()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        }))),
        "core::hash_set::new" => Ok(Value::HashSet(Rc::new(HashSetValue {
            entries: RefCell::new(HashSet::new()),
            element_type: RefCell::new(Type::Unknown),
        }))),
        "core::vec::from" => {
            let Value::Array(array) = &arguments[0] else {
                return Err("Vec::from expects an array".into());
            };
            if array
                .elements
                .borrow()
                .iter()
                .any(|slot| slot.references > 0)
            {
                return Err("cannot move an array into Vec while an element is referenced".into());
            }
            let elements = array.elements.borrow_mut().drain(..).collect();
            Ok(Value::Vec(Rc::new(SequenceValue {
                elements: RefCell::new(elements),
                element_type: RefCell::new(array.element_type.borrow().clone()),
            })))
        }
        _ => Err(format!("unknown core import `{name}`")),
    }
}
