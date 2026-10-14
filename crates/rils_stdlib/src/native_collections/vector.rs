//! Native vector algorithms over borrowed and owned layout views.
use rils_value::{DynamicValue, DynamicValueMut, DynamicValueRef};

pub fn contains(
    view: DynamicValueRef<'_>,
    mut equal: impl FnMut(DynamicValueRef<'_>) -> Result<bool, String>,
) -> Result<bool, String> {
    if let rils_syntax::Type::Array { length, .. } = view.layout()?.rils_type() {
        for index in 0..*length {
            if equal(view.field(index)?)? {
                return Ok(true);
            }
        }
    } else {
        for index in 0..view.sequence_len()? {
            if equal(view.sequence_item(index)?)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn extend(
    mut destination: DynamicValueMut<'_>,
    mut source: DynamicValue,
) -> Result<(), String> {
    if !destination
        .view()
        .layout()?
        .compatible_with(source.descriptor())
    {
        return Err("Vec element types do not match".into());
    }
    destination
        .sequence_borrows()?
        .check_structural_mutation()?;
    let items = source.take_all_sequence_items()?;
    for item in items {
        destination.push_sequence_item(item)?;
    }
    Ok(())
}
