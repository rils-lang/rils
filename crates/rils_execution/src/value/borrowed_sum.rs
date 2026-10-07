//! Sum patterns inspect tags and retain payload paths without materializing bytes.

use std::rc::Rc;

use rils_value::DynamicPathStep;

use super::Value;
use crate::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    Some,
    None,
    Ok,
    Err,
}

/// Inspect only native borrowed sums; compatibility values are handled by callers.
pub fn branch(value: &Value) -> Result<Option<Branch>, String> {
    let Value::Reference(reference) = value else {
        return Ok(None);
    };
    let Some(layout) = reference.native_layout()? else {
        return Ok(None);
    };
    match layout.rils_type() {
        Type::Option(_) => Ok(Some(
            if reference.with_native_view(|view| view.option_is_some())?? {
                Branch::Some
            } else {
                Branch::None
            },
        )),
        Type::Result(_, _) => match reference.with_native_view(|view| view.variant_index())?? {
            0 => Ok(Some(Branch::Ok)),
            1 => Ok(Some(Branch::Err)),
            _ => Err("Result has an invalid variant tag".into()),
        },
        _ => Ok(None),
    }
}

/// Bind shared references as required by Rils borrowed-pattern semantics.
pub fn payload(value: &Value, branch: Branch) -> Result<Value, String> {
    if self::branch(value)? != Some(branch) {
        return Err("borrowed sum payload does not match its branch".into());
    }
    let Value::Reference(reference) = value else {
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
    Ok(Value::Reference(Rc::new(child.reborrow(false)?)))
}
