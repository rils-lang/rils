use super::*;
use rils_execution::value::borrowed_sum::{self, Branch};

pub(super) fn matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
) -> Option<bool> {
    let (expected, inner) = match pattern {
        Pattern::Some { inner, .. } => (Branch::Some, Some(inner)),
        Pattern::None { .. } => (Branch::None, None),
        Pattern::Ok { inner, .. } => (Branch::Ok, Some(inner)),
        Pattern::Err { inner, .. } => (Branch::Err, Some(inner)),
        _ => return None,
    };
    let actual = match borrowed_sum::branch(value) {
        Ok(Some(branch)) => branch,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some(
        actual == expected
            && inner.is_none_or(|pattern| {
                borrowed_sum::payload(value, actual)
                    .is_ok_and(|child| pattern_matches(pattern, &child, bindings, environment))
            }),
    )
}
