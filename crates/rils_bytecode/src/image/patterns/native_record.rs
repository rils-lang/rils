use super::*;
use rils_execution::value::native_instance::{
    NativeInstancePlace, borrow_field, record_definition, record_field_names,
};

pub(super) fn matches(pattern: &HirPattern, value: &Value) -> Option<bool> {
    let HirPattern::Record { path, fields } = pattern else {
        return None;
    };
    let definition = match record_definition(value) {
        Ok(Some(definition)) => definition,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(
        type_path_matches(&definition.name, path)
            && record_field_names(value).is_ok_and(|names| names.len() == fields.len())
            && fields.iter().all(|(name, pattern)| {
                borrow_field(value, name).is_ok_and(|field| pattern_matches(pattern, &field))
            }),
    )
}

pub(super) fn collect(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
    borrowed: bool,
) -> Option<Result<(), String>> {
    let HirPattern::Record { fields, .. } = pattern else {
        return None;
    };
    match record_definition(value) {
        Ok(Some(_)) => {}
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    }
    Some((|| {
        for (name, pattern) in fields {
            if !has_binding(pattern) {
                continue;
            }
            let field = match value {
                Value::Dynamic(object) if !borrowed => NativeInstancePlace::new(object.clone())?
                    .field(name)?
                    .take()?,
                _ => borrow_field(value, name)?,
            };
            collect_pattern_bindings(pattern, &field, bindings)?;
        }
        Ok(())
    })())
}

pub(super) fn has_binding(pattern: &HirPattern) -> bool {
    match pattern {
        HirPattern::Binding(_) => true,
        HirPattern::Some(inner) | HirPattern::Ok(inner) | HirPattern::Err(inner) => {
            has_binding(inner)
        }
        HirPattern::Record { fields, .. } => fields.iter().any(|(_, pattern)| has_binding(pattern)),
        HirPattern::TupleVariant { fields, .. } => fields.iter().any(has_binding),
        _ => false,
    }
}
