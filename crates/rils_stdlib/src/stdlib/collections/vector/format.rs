//! Vec owns its native formatting policy; children use the caller's visitor.

pub(super) fn format(
    view: &rils_value::DynamicValueRef<'_>,
    formatter: &mut std::fmt::Formatter<'_>,
    debug: bool,
    child: rils_native::FormatChild,
) -> Result<std::fmt::Result, String> {
    // Validate and project under the original owner, without cloning any item.
    let length = view.sequence_len()?;
    let mut render = || {
        formatter.write_str("[")?;
        for index in 0..length {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            child(
                view.sequence_item(index).map_err(|_| std::fmt::Error)?,
                formatter,
                debug,
            )?;
        }
        formatter.write_str("]")
    };
    Ok(render())
}
