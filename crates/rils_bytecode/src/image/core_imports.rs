use super::*;

#[derive(Clone, Copy)]
pub(super) enum CoreImport {
    Native(&'static str),
    TypeOf,
    Assert,
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
    imports.push(("core::assert", FunctionSignature::variadic(Type::Unit)));
    imports
}

pub(super) fn resolve_core_import(name: &str) -> Option<CoreImport> {
    if let Some(symbol) = rils_builtins::builtin_function(name).and_then(|item| item.native_symbol)
    {
        return Some(CoreImport::Native(symbol));
    }
    Some(match name {
        "type_of" => CoreImport::TypeOf,
        "core::assert" => CoreImport::Assert,
        _ => return None,
    })
}

pub(super) fn call_core_import(import: CoreImport, arguments: &[Value]) -> Result<Value, String> {
    match import {
        CoreImport::Native(symbol) => {
            crate::runtime_builtins::call_native_symbol(symbol, arguments)
                .ok_or_else(|| format!("native method `{symbol}` is unavailable"))?
        }
        CoreImport::TypeOf => Ok(rils_execution::value::native_string(
            arguments[0].type_name(),
        )),
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
    }
}
