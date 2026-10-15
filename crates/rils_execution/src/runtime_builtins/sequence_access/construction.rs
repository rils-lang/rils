//! Attach concrete layouts to the results of standard-library constructor bodies.
use crate::{
    Type, Value, runtime_builtins::NativeOwnedContext, value::record_codec::NativeRecordCodec,
};
use rils_stdlib::storage::{EntrySchema, OwnedSequence};
use rils_value::{DynamicValue, OwnedElements};
use std::rc::Rc;

pub(crate) fn construct_empty(
    arguments: Vec<Value>,
    schema: &EntrySchema,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
    call: impl FnOnce() -> OwnedSequence,
) -> Result<Value, String> {
    let owner = schema.owner;
    if !arguments.is_empty() {
        return Err(format!("{owner} constructor expects no arguments"));
    }
    let ty = expected.filter(|ty| ty.is_concrete_type()).ok_or_else(|| format!(
        "cannot infer the type arguments of `{owner}::new()`; add a type annotation or explicit type arguments"
    ))?;
    let layout = context.layout(ty)?;
    schema.validate(&layout)?;
    let payload = DynamicValue::sequence(layout, call().into_elements())?;
    crate::value::native_instance::from_native(
        payload,
        Rc::new(NativeRecordCodec::with_definitions(
            &context.structs,
            &context.enums,
        )),
    )
}

pub(crate) fn construct_array(
    arguments: Vec<Value>,
    owner: &str,
    context: &NativeOwnedContext,
    expected: Option<&Type>,
    call: impl FnOnce(OwnedElements) -> Result<OwnedSequence, String>,
) -> Result<Value, String> {
    if arguments.len() != 1 {
        return Err(format!("{owner} constructor expects one owned array"));
    }
    let Value::Dynamic(object) = arguments.into_iter().next().expect("arity checked") else {
        return Err("sequence constructor requires an owned array".into());
    };
    let Type::Array { element, .. } = object.descriptor().layout().rils_type() else {
        return Err("sequence constructor requires an owned array".into());
    };
    let ty = Type::Named {
        name: owner.into(),
        arguments: vec![*element.clone()],
    };
    if let Some(expected) = expected
        && crate::types::merge_types(expected, &ty).is_none()
    {
        return Err(format!(
            "sequence constructor produces {ty}, expected {expected}"
        ));
    }
    let (items, codec) = crate::value::native_array::into_cursor(object, context)?;
    let layout = codec.resolve_layout(&ty, &context.hosts)?;
    let payload = DynamicValue::sequence(layout, call(items)?.into_elements())?;
    crate::value::native_instance::from_native(payload, Rc::new(codec))
}
