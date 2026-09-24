use super::*;
use crate::runtime_builtins::call as call_runtime_builtin;

#[derive(Clone, Copy)]
pub(super) enum CoreImport {
    Builtin(rils_builtins::BuiltinId),
    Native(&'static str),
    TypeOf,
    Assert,
    VecNew,
    VecFrom,
    HashMapNew,
    HashSetNew,
    RcNew,
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

    if let Some(symbol) = rils_builtins::builtin_function(name).and_then(|item| item.native_symbol)
    {
        return Some(CoreImport::Native(symbol));
    }
    Some(match name {
        "type_of" => CoreImport::TypeOf,
        "clone" => CoreImport::Builtin(BuiltinId::Clone),
        "unwrap" => CoreImport::Builtin(BuiltinId::OptionUnwrap),
        "unwrap_or" => CoreImport::Builtin(BuiltinId::OptionUnwrapOr),
        "core::assert" => CoreImport::Assert,
        "core::vec::new" => CoreImport::VecNew,
        "core::vec::from" => CoreImport::VecFrom,
        "core::hash_map::new" => CoreImport::HashMapNew,
        "core::hash_set::new" => CoreImport::HashSetNew,
        "core::rc::new" => CoreImport::RcNew,
        _ => return None,
    })
}

pub(super) fn call_core_import(import: CoreImport, arguments: &[Value]) -> Result<Value, String> {
    match import {
        CoreImport::Builtin(id) => call_runtime_builtin(id, arguments),
        CoreImport::Native(symbol) => {
            crate::runtime_builtins::call_native_symbol(symbol, arguments)
                .ok_or_else(|| format!("native method `{symbol}` is unavailable"))?
        }
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
            active_iterators: std::cell::Cell::new(0),
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        }))),
        CoreImport::HashMapNew => Ok(Value::HashMap(Rc::new(HashMapValue {
            borrowed: std::cell::Cell::new(0),
            entries: RefCell::new(HashMap::new()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        }))),
        CoreImport::HashSetNew => Ok(Value::HashSet(Rc::new(HashSetValue {
            borrowed: std::cell::Cell::new(0),
            entries: RefCell::new(HashSet::new()),
            element_type: RefCell::new(Type::Unknown),
        }))),
        CoreImport::RcNew => {
            let value = arguments
                .first()
                .cloned()
                .ok_or_else(|| "Rc::new expects one value".to_owned())?;
            let type_argument = Type::of_value(&value).unwrap_or(Type::Unknown);
            Ok(Value::Rc(Rc::new(rils_execution::value::RcValue {
                value,
                type_argument,
            })))
        }
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
                active_iterators: std::cell::Cell::new(0),
                elements: RefCell::new(elements),
                element_type: RefCell::new(array.element_type.borrow().clone()),
            })))
        }
    }
}
