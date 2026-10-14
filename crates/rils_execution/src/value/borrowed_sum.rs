//! Sum patterns inspect tags and retain payload paths without materializing bytes.

use std::rc::Rc;

use rils_value::DynamicPathStep;

use super::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    Some,
    None,
    Ok,
    Err,
}

/// Inspect a borrowed sum using the same layout operation as owned values.
pub fn branch(value: &Value) -> Result<Option<Branch>, String> {
    if value.as_reference().is_none() {
        return Ok(None);
    }
    super::sum::branch(value)
}

/// Bind shared references as required by Rils borrowed-pattern semantics.
pub fn payload(value: &Value, branch: Branch) -> Result<Value, String> {
    if self::branch(value)? != Some(branch) {
        return Err("borrowed sum payload does not match its branch".into());
    }
    let Some(reference) = value.as_reference() else {
        return Err("borrowed sum payload requires a reference".into());
    };
    let step = match branch {
        Branch::Some => DynamicPathStep::Some,
        Branch::Ok => DynamicPathStep::Variant(0),
        Branch::Err => DynamicPathStep::Variant(1),
        Branch::None => return Err("None has no payload".into()),
    };
    let child = reference
        .project_native_step(step)?
        .ok_or("borrowed sum has no native storage")?;
    Ok(Value::from_reference(Rc::new(child.reborrow(false)?)))
}
