//! Resolve storage declarations from call arguments without retaining payloads.

use super::*;

pub(super) fn resolve_return_type<'a>(
    function: &BytecodeFunction,
    arguments: impl Iterator<Item = &'a Value>,
) -> Option<Type> {
    let expected = function.return_type.as_ref()?;
    let mut bindings = HashMap::new();
    for (parameter, argument) in function.parameter_types.iter().zip(arguments) {
        if let (Some(parameter), Some(actual)) = (parameter, Type::of_value(argument)) {
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
    Some(expected.substitute(&bindings))
}
