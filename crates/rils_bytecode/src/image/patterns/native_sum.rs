use super::*;
use rils_execution::value::borrowed_sum::{self, Branch};

fn parts(pattern: &HirPattern) -> Option<(Branch, Option<&HirPattern>)> {
    match pattern {
        HirPattern::Some(inner) => Some((Branch::Some, Some(inner))),
        HirPattern::None => Some((Branch::None, None)),
        HirPattern::Ok(inner) => Some((Branch::Ok, Some(inner))),
        HirPattern::Err(inner) => Some((Branch::Err, Some(inner))),
        _ => None,
    }
}

pub(super) fn matches(pattern: &HirPattern, value: &Value) -> Option<bool> {
    let (expected, inner) = parts(pattern)?;
    let actual = match borrowed_sum::branch(value) {
        Ok(Some(branch)) => branch,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(
        actual == expected
            && inner.is_none_or(|pattern| {
                borrowed_sum::payload(value, actual)
                    .is_ok_and(|child| pattern_matches(pattern, &child))
            }),
    )
}

pub(super) fn collect(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
) -> Option<Result<(), String>> {
    let (expected, inner) = parts(pattern)?;
    let actual = match borrowed_sum::branch(value) {
        Ok(Some(branch)) => branch,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    Some((|| {
        if actual != expected {
            return Err("pattern does not match sum branch".into());
        }
        if let Some(pattern) = inner {
            let child = borrowed_sum::payload(value, actual)?;
            collect_pattern_bindings(pattern, &child, bindings)?;
        }
        Ok(())
    })())
}
