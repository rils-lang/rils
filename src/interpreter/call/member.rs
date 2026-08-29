use super::*;

pub(super) fn resolve_numeric_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let method = match value {
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::Isize(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_) => rils_builtins::integer_method(name)
            .map(|method| BuiltinMethod::IntegerIntrinsic(method.id)),
        Value::F32(_) | Value::F64(_) => {
            rils_builtins::float_method(name).map(|method| BuiltinMethod::FloatIntrinsic(method.id))
        }
        _ => return Ok(None),
    }
    .ok_or_else(|| {
        RuntimeError::new(
            format!("{} has no method `{name}`", value.type_name()),
            span,
        )
    })?;
    Ok(Some(Value::BuiltinBoundMethod(Rc::new(
        BuiltinBoundMethod {
            receiver: Rc::new(value.clone()),
            method,
        },
    ))))
}

pub(super) fn resolve_host_or_builtin_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    if let Some((id, _)) = builtin_runtime_member(value, name) {
        return Ok(Some(Value::BuiltinBoundMethod(Rc::new(
            BuiltinBoundMethod {
                receiver: Rc::new(value.clone()),
                method: BuiltinMethod::Runtime(id),
            },
        ))));
    }
    let Value::HostObject(instance) = value else {
        return Ok(None);
    };
    let function = instance
        .type_definition
        .methods
        .borrow()
        .get(name)
        .cloned()
        .ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type `{}` has no method `{name}`",
                    instance.type_definition.name
                ),
                span,
            )
        })?;
    Ok(Some(Value::HostBoundMethod(Rc::new(HostBoundMethod {
        receiver: Rc::new(value.clone()),
        function,
    }))))
}

pub(super) fn take_tuple_field(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let Value::Tuple(sequence) = value else {
        return Ok(None);
    };
    let index = name
        .parse::<usize>()
        .map_err(|_| RuntimeError::new(format!("tuple has no field `{name}`"), span))?;
    let mut elements = sequence.elements.borrow_mut();
    let slot = elements
        .get_mut(index)
        .ok_or_else(|| RuntimeError::new(format!("tuple index {index} is out of bounds"), span))?;
    let field = slot
        .value
        .as_ref()
        .ok_or_else(|| RuntimeError::new(format!("use of moved tuple field `{index}`"), span))?;
    if field.is_copy() {
        return field
            .clone_owned()
            .map(Some)
            .map_err(|message| RuntimeError::new(message, span));
    }
    if slot.references > 0 {
        return Err(RuntimeError::new(
            format!("cannot move tuple field `{index}` while it is referenced"),
            span,
        ));
    }
    Ok(Some(
        slot.value.take().expect("tuple field value was checked"),
    ))
}

pub(super) fn take_struct_field(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let Value::Struct(instance) = value else {
        return Ok(None);
    };
    if !instance.fields.borrow().contains_key(name) {
        return Ok(None);
    }
    let mut fields = instance.fields.borrow_mut();
    let field = fields.get_mut(name).expect("field presence was checked");
    let value = field.value.as_ref().ok_or_else(|| {
        RuntimeError::new(
            format!(
                "use of moved field `{}.{name}`",
                instance.type_definition.name
            ),
            span,
        )
    })?;
    if value.is_copy() {
        return value
            .clone_owned()
            .map(Some)
            .map_err(|message| RuntimeError::new(message, span));
    }
    if field.references > 0 {
        return Err(RuntimeError::new(
            format!("cannot move field `{name}` while it is referenced"),
            span,
        ));
    }
    Ok(Some(field.value.take().expect("field value was checked")))
}

pub(super) fn read_borrowed_field(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let field = match value {
        Value::Tuple(sequence) => {
            let index = name
                .parse::<usize>()
                .map_err(|_| RuntimeError::new(format!("tuple has no field `{name}`"), span))?;
            let value = sequence
                .elements
                .borrow()
                .get(index)
                .and_then(|slot| slot.value.as_ref())
                .cloned()
                .ok_or_else(|| {
                    RuntimeError::new(format!("use of moved tuple field `{index}`"), span)
                })?;
            (value, format!("tuple field `{index}`"))
        }
        Value::Struct(instance) => {
            let fields = instance.fields.borrow();
            let Some(field) = fields.get(name) else {
                return Ok(None);
            };
            let value =
                field.value.as_ref().cloned().ok_or_else(|| {
                    RuntimeError::new(format!("use of moved field `{name}`"), span)
                })?;
            (value, format!("field `{name}`"))
        }
        _ => return Ok(None),
    };
    if !field.0.is_copy() {
        return Err(RuntimeError::new(
            format!("cannot move non-Copy {} through a reference", field.1),
            span,
        ));
    }
    field
        .0
        .clone_owned()
        .map(Some)
        .map_err(|message| RuntimeError::new(message, span))
}

pub(super) fn resolve_borrowed_host_or_builtin_member(
    borrowed: &Value,
    receiver: Value,
    receiver_mutable: bool,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    if let Some((id, mode)) = builtin_runtime_member(borrowed, name) {
        if mode == rils_builtins::ReceiverMode::Mutable && !receiver_mutable {
            return Err(RuntimeError::new(
                format!("{}::{name} requires `&mut self`", borrowed.type_name()),
                span,
            ));
        }
        return Ok(Some(Value::BuiltinBoundMethod(Rc::new(
            BuiltinBoundMethod {
                receiver: Rc::new(receiver),
                method: BuiltinMethod::Runtime(id),
            },
        ))));
    }
    let Value::HostObject(instance) = borrowed else {
        return Ok(None);
    };
    let function = instance
        .type_definition
        .methods
        .borrow()
        .get(name)
        .cloned()
        .ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type `{}` has no method `{name}`",
                    instance.type_definition.name
                ),
                span,
            )
        })?;
    Ok(Some(Value::HostBoundMethod(Rc::new(HostBoundMethod {
        receiver: Rc::new(receiver),
        function,
    }))))
}
