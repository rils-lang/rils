use super::*;
use crate::ast::EnumVariant;
use rils_execution::value::native_instance::{
    NativeEnumVariant, NativeInstancePlace, borrow_variant_field, enum_variant,
};
use rils_value::DynamicPathStep;

pub(super) fn matches(pattern: &HirPattern, value: &Value) -> Option<bool> {
    if !matches!(
        pattern,
        HirPattern::TupleVariant { .. } | HirPattern::Record { .. } | HirPattern::Path(_)
    ) {
        return None;
    }
    let variant = match enum_variant(value) {
        Ok(Some(variant)) => variant,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(fields(pattern, &variant).is_some_and(|(path, fields)| {
        pattern_variant(path).is_some_and(|(enum_path, name)| {
            name == variant.name() && type_path_matches(&variant.definition.name, enum_path)
        }) && fields.iter().all(|(name, pattern)| {
            borrow_variant_field(value, variant.index, name)
                .is_ok_and(|field| pattern_matches(pattern, &field))
        })
    }))
}

pub(super) fn collect(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
    borrowed: bool,
) -> Option<Result<(), String>> {
    if !matches!(
        pattern,
        HirPattern::TupleVariant { .. } | HirPattern::Record { .. } | HirPattern::Path(_)
    ) {
        return None;
    }
    let variant = match enum_variant(value) {
        Ok(Some(variant)) => variant,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    Some((|| {
        let (_, fields) =
            fields(pattern, &variant).ok_or("pattern does not match enum variant shape")?;
        for (name, pattern) in fields {
            if !native_record::has_binding(pattern) {
                continue;
            }
            let field = match value {
                Value::Dynamic(object) if !borrowed => NativeInstancePlace::new(object.clone())?
                    .project(DynamicPathStep::Variant(variant.index))?
                    .field(&name)?
                    .take()?,
                _ => borrow_variant_field(value, variant.index, &name)?,
            };
            collect_pattern_bindings(pattern, &field, bindings)?;
        }
        Ok(())
    })())
}

type PatternFields<'a> = (&'a [String], Vec<(String, &'a HirPattern)>);

fn fields<'a>(pattern: &'a HirPattern, variant: &NativeEnumVariant) -> Option<PatternFields<'a>> {
    match (pattern, variant.declaration()) {
        (HirPattern::Path(path), EnumVariant::Unit { .. }) => Some((path, vec![])),
        (
            HirPattern::TupleVariant { path, fields },
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
            HirPattern::Record { path, fields },
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
