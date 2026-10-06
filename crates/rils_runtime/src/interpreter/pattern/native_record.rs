use super::*;
use rils_execution::value::native_instance::{
    NativeInstancePlace, borrow_field, record_definition, record_field_names,
};

pub(super) fn matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
    borrowed: bool,
) -> Option<bool> {
    let Pattern::Record { path, fields, .. } = pattern else {
        return None;
    };
    let definition = match record_definition(value) {
        Ok(Some(definition)) => definition,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(
        (|| {
            if !struct_type_matches(path, &definition, environment)
                || record_field_names(value).ok()?.len() != fields.len()
            {
                return Some(false);
            }
            // Probe every child before transferring any ownership. Failed arms must
            // leave the scrutinee intact, and probes retain the original field place.
            let mut probes = Vec::new();
            for (name, pattern) in fields {
                let field = borrow_field(value, name).ok()?;
                if !pattern_matches(pattern, &field, &mut probes, environment) {
                    return Some(false);
                }
            }
            if borrowed || matches!(value, Value::Reference(_)) {
                bindings.extend(probes);
                return Some(true);
            }
            drop(probes);
            let Value::Dynamic(object) = value else {
                return Some(false);
            };
            let owner = NativeInstancePlace::new(object.clone()).ok()?;
            for (name, pattern) in fields {
                if !has_binding(pattern) {
                    continue;
                }
                let field = owner.field(name).ok()?.take().ok()?;
                if !pattern_matches(pattern, &field, bindings, environment) {
                    return Some(false);
                }
            }
            Some(true)
        })()
        .unwrap_or(false),
    )
}

fn has_binding(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Binding { .. } => true,
        Pattern::Some { inner, .. } | Pattern::Ok { inner, .. } | Pattern::Err { inner, .. } => {
            has_binding(inner)
        }
        Pattern::Record { fields, .. } => fields.iter().any(|(_, pattern)| has_binding(pattern)),
        Pattern::TupleVariant { fields, .. } => fields.iter().any(has_binding),
        _ => false,
    }
}
