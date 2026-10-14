use super::*;
use crate::environment::StorageSlot;

pub(super) fn resolve_numeric_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let method = match super::helpers::receiver_type(value) {
        Some(Type::Integer(_)) => {
            rils_builtins::integer_method(name).map(|method| BuiltinMethod::Native(method.symbol))
        }
        Some(Type::Float(_)) => {
            rils_builtins::float_method(name).map(|method| BuiltinMethod::Native(method.symbol))
        }
        _ => return Ok(None),
    }
    .ok_or_else(|| {
        RuntimeError::new(
            format!("{} has no method `{name}`", value.type_name()),
            span,
        )
    })?;
    Ok(Some(Value::from_callable(Rc::new(BuiltinBoundMethod {
        receiver: Rc::new(value.clone()),
        method,
    }))))
}

pub(super) fn resolve_host_or_builtin_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    if let Some((method, mode)) = builtin_runtime_member(value, name) {
        if mode == rils_builtins::ReceiverMode::Mutable
            && value
                .as_reference()
                .is_some_and(|reference| !reference.mutable)
        {
            return Err(RuntimeError::new(
                format!("method `{name}` requires `&mut self`"),
                span,
            ));
        }
        return Ok(Some(Value::from_callable(Rc::new(BuiltinBoundMethod {
            receiver: Rc::new(value.clone()),
            method,
        }))));
    }
    if name == "into_iter"
        && Type::of_value(value).is_some_and(|ty| {
            let Type::Named { name, .. } = ty else {
                return false;
            };
            rils_builtins::builtin(&name)
                .is_some_and(|definition| definition.member("next").is_some())
        })
    {
        return Ok(Some(Value::from_callable(Rc::new(BuiltinBoundMethod {
            receiver: Rc::new(value.clone()),
            method: BuiltinMethod::IteratorIdentity,
        }))));
    }
    let Some(definition) = value
        .host_type_definition()
        .map_err(|message| RuntimeError::new(message, span))?
    else {
        return Ok(None);
    };
    let function = definition
        .methods
        .borrow()
        .get(name)
        .cloned()
        .ok_or_else(|| {
            RuntimeError::new(
                format!("type `{}` has no method `{name}`", definition.name),
                span,
            )
        })?;
    Ok(Some(Value::from_callable(Rc::new(HostBoundMethod {
        receiver: Rc::new(value.clone()),
        function,
    }))))
}

pub(super) fn take_struct_field(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    if let Value::Dynamic(object) = value {
        if object
            .descriptor()
            .layout()
            .record_field_index(name)
            .is_none()
        {
            return Ok(None);
        }
        return rils_execution::value::native_instance::NativeInstancePlace::new(object.clone())
            .and_then(|place| place.field(name))
            .and_then(|place| place.take())
            .map(Some)
            .map_err(|message| RuntimeError::new(message, span));
    }
    Ok(None)
}

pub(super) fn bind_rils_method(
    receiver: Value,
    methods: &std::cell::RefCell<HashMap<String, Rc<UserFunction>>>,
    trait_methods: &std::cell::RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    type_name: &str,
    name: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    let function = select_method(methods, trait_methods, name).map_err(|traits| {
        RuntimeError::new(
            format!(
                "method `{name}` is ambiguous for `{type_name}`; candidates come from traits {}",
                traits.join(", ")
            ),
            span,
        )
    })?;
    let Some(function) = function else {
        if name == "clone" {
            return Ok(Value::from_callable(Rc::new(BuiltinBoundMethod {
                receiver: Rc::new(receiver),
                method: clone_method(),
            })));
        }
        return Err(RuntimeError::new(
            format!("type `{type_name}` has no member `{name}`"),
            span,
        ));
    };
    if function
        .parameters
        .first()
        .map(|parameter| parameter.name.as_str())
        != Some("self")
    {
        return Err(RuntimeError::new(
            format!("`{type_name}::{name}` is an associated function, not a method"),
            span,
        ));
    }
    let receiver = match function
        .parameters
        .first()
        .and_then(|parameter| parameter.type_annotation.as_ref())
    {
        Some(Type::Reference { mutable, .. }) if receiver.as_reference().is_none() => {
            let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(*mutable)));
            storage.borrow_mut().initialize(receiver);
            Value::from_reference(Rc::new(ReferenceValue::new_storage(storage, *mutable)))
        }
        _ => receiver,
    };
    Ok(Value::from_callable(Rc::new(BoundMethod {
        receiver: Rc::new(receiver),
        function,
    })))
}
