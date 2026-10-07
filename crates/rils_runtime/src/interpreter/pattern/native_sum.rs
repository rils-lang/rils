use super::*;
use rils_execution::value::{borrowed_sum::Branch, sum};

pub(super) fn matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
    borrowed: bool,
) -> Option<bool> {
    let (expected, inner) = match pattern {
        Pattern::Some { inner, .. } => (Branch::Some, Some(inner)),
        Pattern::None { .. } => (Branch::None, None),
        Pattern::Ok { inner, .. } => (Branch::Ok, Some(inner)),
        Pattern::Err { inner, .. } => (Branch::Err, Some(inner)),
        _ => return None,
    };
    let actual = match sum::branch(value) {
        Ok(Some(branch)) => branch,
        Ok(None) => return None,
        Err(_) => return Some(false),
    };
    Some((|| {
        if actual != expected {
            return false;
        }
        let Some(pattern) = inner else { return true };
        let Ok(child) = sum::borrow_payload(value, actual) else {
            return false;
        };
        let mut probes = Vec::new();
        if !pattern_matches(pattern, &child, &mut probes, environment) {
            return false;
        }
        if borrowed || matches!(value, Value::Reference(_)) {
            bindings.extend(probes);
            return true;
        }
        drop(probes);
        drop(child);
        if !native_record::has_binding(pattern) {
            return true;
        }
        sum::bind_payload(value, actual)
            .is_ok_and(|child| pattern_matches(pattern, &child, bindings, environment))
    })())
}
