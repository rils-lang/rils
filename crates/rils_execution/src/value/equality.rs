//! Borrowed comparison of native storage and execution handles.

use std::rc::Rc;

use rils_value::{DynamicValueRef, NativeLeafRef};

use super::{ReferenceValue, Value};
use crate::Type;

use super::borrowed::{Read, with_read};

impl Value {
    /// Compare values without cloning their native payloads.
    pub fn try_equal(&self, other: &Self) -> Result<bool, String> {
        if let (Self::Native(left), Self::Native(right)) = (self, other) {
            return Ok(super::native_ops::equal(left, right));
        }
        with_read(self, false, |left| {
            with_read(other, false, |right| compare(left, right))
        })??
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.try_equal(other).unwrap_or(false)
    }
}

/// Compare the referents passed to a shared standard-library query.
pub fn borrowed_equal(left: &Value, right: &Value) -> Result<bool, String> {
    with_read(left, true, |left| {
        with_read(right, true, |right| compare(left, right))
    })??
}

/// Match a literal against the original referent, without copying a borrowed value.
/// This dereferences the scrutinee only; ordinary reference equality keeps its identity rules.
pub fn literal_matches(literal: &Value, value: &Value) -> Result<bool, String> {
    with_read(literal, false, |literal| {
        super::borrowed::with_referent(value, |value| compare(literal, value))
    })??
}

pub(crate) fn view_query_equal(
    view: DynamicValueRef<'_>,
    argument: &Value,
) -> Result<bool, String> {
    with_read(argument, true, |value| compare(Read::View(view), value))?
}

fn compare(left: Read<'_>, right: Read<'_>) -> Result<bool, String> {
    match (left, right) {
        (Read::View(left), Read::View(right)) => equal_views(&left, &right),
        (Read::Leaf(left), Read::Leaf(right)) => equal_leaves(&left, &right),
        (Read::View(view), Read::Leaf(leaf)) | (Read::Leaf(leaf), Read::View(view)) => {
            if !view.layout()?.is_rust_value() {
                return Ok(false);
            }
            equal_leaves(&view.leaf()?, &leaf)
        }
        _ => Ok(false),
    }
}

fn equal_leaves(left: &NativeLeafRef<'_>, right: &NativeLeafRef<'_>) -> Result<bool, String> {
    if left.rils_type() != right.rils_type() {
        return Ok(false);
    }
    if left.is_rust_type::<Rc<ReferenceValue>>() && right.is_rust_type::<Rc<ReferenceValue>>() {
        return left.with_rust::<Rc<ReferenceValue>, _>(|left| {
            right.with_rust::<Rc<ReferenceValue>, _>(|right| Rc::ptr_eq(left, right))
        })?;
    }
    // Opaque execution handles retain their existing equality policy.
    if matches!(left.rils_type(), Type::Function { .. })
        || left.is_rust_type::<Rc<super::HostObject>>()
    {
        return Ok(false);
    }
    rils_stdlib::native::registry().equal_leaf(left, right)
}

fn equal_views(left: &DynamicValueRef<'_>, right: &DynamicValueRef<'_>) -> Result<bool, String> {
    let left_layout = left.layout()?;
    let right_layout = right.layout()?;
    if left_layout.rils_type() != right_layout.rils_type() {
        return Ok(false);
    }
    if let Some(result) = rils_stdlib::native::registry().equal_view(left, right, equal_views) {
        return result;
    }
    if left_layout.is_rust_value() && right_layout.is_rust_value() {
        return equal_leaves(&left.leaf()?, &right.leaf()?);
    }
    if !left_layout.compatible_with(&right_layout) {
        return Ok(false);
    }
    if left_layout.option_item().is_some() {
        return match (left.option_is_some()?, right.option_is_some()?) {
            (false, false) => Ok(true),
            (true, true) => equal_views(&left.option_item()?, &right.option_item()?),
            _ => Ok(false),
        };
    }
    if left_layout.variant_alternatives().is_some() {
        if left.variant_index()? != right.variant_index()? {
            return Ok(false);
        }
        return equal_views(&left.variant_payload()?, &right.variant_payload()?);
    }
    if let Some(fields) = left_layout.record_fields() {
        for index in 0..fields.len() {
            if !equal_views(&left.field(index)?, &right.field(index)?)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Err(format!(
        "{} has no registered native equality",
        left_layout.rils_type()
    ))
}
