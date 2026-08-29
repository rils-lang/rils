use super::runtime_builtins::call as call_runtime_builtin;
use super::*;

#[derive(Clone, Copy)]
pub(super) enum CoreImport {
    Builtin(rils_builtins::BuiltinId),
    TypeOf,
    Assert,
    VecNew,
    VecFrom,
    HashMapNew,
    HashSetNew,
}

pub(super) fn core_imports() -> Vec<(&'static str, FunctionSignature)> {
    let mut imports = rils_builtins::BUILTINS
        .iter()
        .filter(|declaration| {
            declaration.kind == rils_builtins::BuiltinKind::Function
                && declaration.backend == rils_builtins::BuiltinBackend::Runtime
        })
        .map(|declaration| {
            (
                declaration.path,
                rils_frontend::standard_library::standard_function_signature(declaration.path)
                    .expect("runtime built-in function has a signature"),
            )
        })
        .collect::<Vec<_>>();
    imports.extend(rils_builtins::BUILTINS.iter().flat_map(|declaration| {
        declaration.members.iter().filter_map(|member| {
            Some((
                member.runtime_import?,
                rils_frontend::standard_library::builtin_associated_function_signature(
                    declaration.path,
                    member.name,
                )?,
            ))
        })
    }));
    imports.push(("core::assert", FunctionSignature::variadic(Type::Unit)));
    imports
}

pub(super) fn resolve_core_import(name: &str) -> Option<CoreImport> {
    use rils_builtins::BuiltinId;

    Some(match name {
        "type_of" => CoreImport::TypeOf,
        "clone" => CoreImport::Builtin(BuiltinId::Clone),
        "is_ok" => CoreImport::Builtin(BuiltinId::ResultIsOk),
        "is_err" => CoreImport::Builtin(BuiltinId::ResultIsErr),
        "is_some" => CoreImport::Builtin(BuiltinId::OptionIsSome),
        "is_none" => CoreImport::Builtin(BuiltinId::OptionIsNone),
        "unwrap" => CoreImport::Builtin(BuiltinId::OptionUnwrap),
        "unwrap_or" => CoreImport::Builtin(BuiltinId::OptionUnwrapOr),
        "core::assert" => CoreImport::Assert,
        "core::vec::new" => CoreImport::VecNew,
        "core::vec::from" => CoreImport::VecFrom,
        "core::hash_map::new" => CoreImport::HashMapNew,
        "core::hash_set::new" => CoreImport::HashSetNew,
        _ => return None,
    })
}

pub(super) fn call_core_import(import: CoreImport, arguments: &[Value]) -> Result<Value, String> {
    match import {
        CoreImport::Builtin(id) => call_runtime_builtin(id, arguments),
        CoreImport::TypeOf => Ok(Value::String(Rc::from(arguments[0].type_name()))),
        CoreImport::Assert => match arguments.first() {
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
        CoreImport::VecNew => Ok(Value::Vec(Rc::new(SequenceValue {
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        }))),
        CoreImport::HashMapNew => Ok(Value::HashMap(Rc::new(HashMapValue {
            entries: RefCell::new(HashMap::new()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        }))),
        CoreImport::HashSetNew => Ok(Value::HashSet(Rc::new(HashSetValue {
            entries: RefCell::new(HashSet::new()),
            element_type: RefCell::new(Type::Unknown),
        }))),
        CoreImport::VecFrom => {
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
    }
}
