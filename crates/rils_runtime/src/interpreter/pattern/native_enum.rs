use super::*;
use rils_execution::value::native_instance::{
    NativeEnumVariant, NativeInstancePlace, borrow_variant_field, enum_variant,
};
use rils_value::DynamicPathStep;

pub(super) fn matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
    borrowed: bool,
) -> Option<bool> {
    if !matches!(
        pattern,
        Pattern::TupleVariant { .. } | Pattern::Record { .. } | Pattern::Path { .. }
    ) {
        return None;
    }
    let variant = match enum_variant(value) {
        Ok(Some(variant)) => variant,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(
        (|| {
            let (path, fields) = fields(pattern, &variant)?;
            if path.last()? != variant.name()
                || !nominal_type_matches(path, &variant.definition, environment)
            {
                return Some(false);
            }
            let mut probes = Vec::new();
            for (name, pattern) in &fields {
                let field = borrow_variant_field(value, variant.index, name).ok()?;
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
            let owner = NativeInstancePlace::new(object.clone())
                .ok()?
                .project(DynamicPathStep::Variant(variant.index))
                .ok()?;
            for (name, pattern) in fields {
                if !native_record::has_binding(pattern) {
                    continue;
                }
                let field = owner.field(&name).ok()?.take().ok()?;
                if !pattern_matches(pattern, &field, bindings, environment) {
                    return Some(false);
                }
            }
            Some(true)
        })()
        .unwrap_or(false),
    )
}

type PatternFields<'a> = (&'a [String], Vec<(String, &'a Pattern)>);

fn fields<'a>(pattern: &'a Pattern, variant: &NativeEnumVariant) -> Option<PatternFields<'a>> {
    match (pattern, variant.declaration()) {
        (Pattern::Path { path, .. }, EnumVariant::Unit { .. }) => Some((path, vec![])),
        (
            Pattern::TupleVariant { path, fields, .. },
            EnumVariant::Tuple {
                fields: declared, ..
            },
        ) if fields.len() == declared.len() => Some((
            path,
            fields
                .iter()
                .enumerate()
                .map(|(index, pattern)| (index.to_string(), pattern))
                .collect(),
        )),
        (
            Pattern::Record { path, fields, .. },
            EnumVariant::Record {
                fields: declared, ..
            },
        ) if fields.len() == declared.len()
            && fields
                .iter()
                .all(|(name, _)| declared.iter().any(|field| field.name == *name)) =>
        {
            Some((
                path,
                fields
                    .iter()
                    .map(|(name, pattern)| (name.clone(), pattern))
                    .collect(),
            ))
        }
        _ => None,
    }
}
