//! Keep lexical reference ownership in the execution layer during borrowed iteration.
use super::*;

pub(crate) struct BorrowedSequence<'a> {
    value: &'a Value,
    schema: &'a rils_stdlib::storage::EntrySchema,
    method: &'a str,
}
impl rils_stdlib::storage::SequenceIter for BorrowedSequence<'_> {
    type IterOutput<'a>
        = Result<Value, String>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        crate::runtime_builtins::indexed_iter::borrow(
            std::slice::from_ref(self.value),
            self.schema,
            self.method,
        )
    }
}

pub(crate) fn with_borrowed_sequence(
    arguments: &[Value],
    schema: &rils_stdlib::storage::EntrySchema,
    method: &str,
    call: impl FnOnce(BorrowedSequence<'_>) -> Result<Value, String>,
) -> Result<Value, String> {
    let [value] = arguments else {
        return Err("borrowed sequence call expects one receiver".into());
    };
    if value.as_reference().is_none() {
        return Err("borrowed sequence call requires a reference".into());
    }
    call(BorrowedSequence {
        value,
        schema,
        method,
    })
}

/// Validate and retain native storage before calling an owning standard-library body.
pub(crate) fn with_owned_sequence(
    arguments: Vec<Value>,
    schema: &rils_stdlib::storage::EntrySchema,
    context: &crate::runtime_builtins::NativeOwnedContext,
    call: impl FnOnce(rils_value::OwnedElements) -> rils_value::OwnedElements,
) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err("owning sequence call expects one receiver".into());
    }
    let value = arguments.into_iter().next().expect("arity checked");
    if value.as_reference().is_some() {
        return Err("owning sequence call requires an owned value".into());
    }
    let Value::Dynamic(object) = value else {
        return Err("owning sequence call requires native storage".into());
    };
    let ty = object.descriptor().layout().rils_type();
    schema.validate(object.descriptor().layout())?;
    let iterator_type = crate::iteration::declared_iterator_type(ty)
        .ok_or("sequence has no declared iterator type")?;
    let (items, codec) = if matches!(ty, Type::Array { .. }) {
        crate::value::native_array::into_cursor(object, context)?
    } else {
        let codec = crate::value::sum::codec(&object, &context.structs, &context.enums)?;
        let items = object.with_mut(|payload| payload.drain_sequence())??;
        (items, codec)
    };
    let item_layout = items.item_layout().clone();
    let items = call(items);
    if !item_layout.compatible_with(items.item_layout()) {
        return Err("sequence method returned a different iterator element layout".into());
    }
    let iterator = crate::value::OwnedIteratorValue::from_cursor(items, codec)?
        .with_iterator_type(iterator_type);
    Ok(Value::from_owned_iterator(iterator))
}
