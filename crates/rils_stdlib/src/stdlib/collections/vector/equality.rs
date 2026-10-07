//! Vec compares borrowed elements in order through the caller's visitor.

pub(super) fn equal(
    left: &rils_value::DynamicValueRef<'_>,
    right: &rils_value::DynamicValueRef<'_>,
    children: rils_native::EqualChildren,
) -> Result<bool, String> {
    let length = left.sequence_len()?;
    if length != right.sequence_len()? {
        return Ok(false);
    }
    for index in 0..length {
        if !children(&left.sequence_item(index)?, &right.sequence_item(index)?)? {
            return Ok(false);
        }
    }
    Ok(true)
}
