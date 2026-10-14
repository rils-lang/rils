//! Resolve storage declarations from call arguments without retaining payloads.

use super::*;

pub(super) fn callable_signature(function: &BytecodeFunction, bound: &[Value]) -> Type {
    let bindings = resolve_type_bindings(function, bound.iter());
    Type::function(
        function
            .parameter_types
            .iter()
            .skip(bound.len())
            .map(|ty| ty.clone().unwrap_or(Type::Unknown).substitute(&bindings))
            .collect(),
        function
            .return_type
            .clone()
            .unwrap_or(Type::Unknown)
            .substitute(&bindings),
    )
}

pub(super) fn resolve_type_bindings<'a>(
    function: &BytecodeFunction,
    arguments: impl Iterator<Item = &'a Value>,
) -> HashMap<String, Type> {
    let mut bindings = HashMap::new();
    for (parameter, argument) in function.parameter_types.iter().zip(arguments) {
        if let Some(parameter) = parameter
            && !parameter.is_concrete_type()
            && let Some(actual) = Type::of_value(argument)
        {
            // Aliases have already been checked by the frontend. Only retain
            // bindings established by a matching concrete signature shape.
            let mut inferred = bindings.clone();
            if rils_frontend::types::infer_generic_arguments(parameter, &actual, &mut inferred)
                .is_ok()
            {
                bindings = inferred;
            }
        }
    }
    bindings.retain(|_, ty| *ty != Type::Unknown);
    bindings
}
