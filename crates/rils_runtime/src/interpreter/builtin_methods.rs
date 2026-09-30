use super::*;

impl Interpreter {
    pub(super) fn call_builtin_method(
        &mut self,
        method: &BuiltinBoundMethod,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let arity = match method.method {
            BuiltinMethod::IteratorIdentity => 0,
            BuiltinMethod::Runtime(id) => rils_builtins::runtime_member(id)
                .and_then(|(_, member)| member.signature)
                .map_or(0, |signature| signature.parameters.len()),
            BuiltinMethod::Native(symbol) => rils_builtins::native_member(symbol)
                .and_then(|member| member.signature)
                .map(|signature| signature.parameters.len())
                .or_else(|| {
                    rils_builtins::intrinsic_by_symbol(symbol)
                        .map(|(item, _)| item.signature.parameters.len())
                })
                .unwrap_or(0),
        };
        check_arity("builtin method", arity, arity, arguments.len(), span)?;
        if let BuiltinMethod::Runtime(id) = method.method
            && let Some(symbol) =
                rils_builtins::runtime_member(id).and_then(|(_, member)| member.native_symbol)
        {
            let mut values = Vec::with_capacity(arguments.len() + 1);
            values.push((*method.receiver).clone());
            values.extend_from_slice(arguments);
            return self.call_native_symbol(symbol, &values, span);
        }
        if let BuiltinMethod::Runtime(id) = method.method
            && rils_builtins::is_iterator_default_builtin(id)
            && let Some((_, member)) = rils_builtins::runtime_member(id)
            && let Some(bound) =
                self.bind_exported_iterator_default(method.receiver.as_ref(), member.name)
        {
            return self.call(bound, arguments, span);
        }
        match method.method {
            BuiltinMethod::Native(symbol) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                self.call_native_symbol(symbol, &values, span)
            }
            BuiltinMethod::IteratorIdentity => Ok((*method.receiver).clone()),
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::HashMapClear
                | rils_builtins::BuiltinId::HashMapContainsKey
                | rils_builtins::BuiltinId::HashMapInsert
                | rils_builtins::BuiltinId::HashMapGetCloned
                | rils_builtins::BuiltinId::HashMapRemove
                | rils_builtins::BuiltinId::HashMapKeysCloned
                | rils_builtins::BuiltinId::HashMapValuesCloned
                | rils_builtins::BuiltinId::HashSetClear
                | rils_builtins::BuiltinId::HashSetContains
                | rils_builtins::BuiltinId::HashSetInsert
                | rils_builtins::BuiltinId::HashSetRemove
                | rils_builtins::BuiltinId::HashSetIsSubset
                | rils_builtins::BuiltinId::HashSetIsSuperset
                | rils_builtins::BuiltinId::HashSetIsDisjoint
                | rils_builtins::BuiltinId::HashSetUnion
                | rils_builtins::BuiltinId::HashSetIntersection
                | rils_builtins::BuiltinId::HashSetDifference
                | rils_builtins::BuiltinId::HashSetSymmetricDifference),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::BtreeSetClear
                | rils_builtins::BuiltinId::BtreeSetContains
                | rils_builtins::BuiltinId::BtreeSetInsert
                | rils_builtins::BuiltinId::BtreeSetRemove
                | rils_builtins::BuiltinId::BtreeSetFirstCloned
                | rils_builtins::BuiltinId::BtreeSetLastCloned
                | rils_builtins::BuiltinId::BtreeSetIsSubset
                | rils_builtins::BuiltinId::BtreeSetIsSuperset
                | rils_builtins::BuiltinId::BtreeSetIsDisjoint
                | rils_builtins::BuiltinId::BtreeSetUnion
                | rils_builtins::BuiltinId::BtreeSetIntersection
                | rils_builtins::BuiltinId::BtreeSetDifference
                | rils_builtins::BuiltinId::BtreeSetSymmetricDifference
                | rils_builtins::BuiltinId::BtreeMapClear
                | rils_builtins::BuiltinId::BtreeMapContainsKey
                | rils_builtins::BuiltinId::BtreeMapInsert
                | rils_builtins::BuiltinId::BtreeMapGetCloned
                | rils_builtins::BuiltinId::BtreeMapRemove
                | rils_builtins::BuiltinId::BtreeMapFirstKeyCloned
                | rils_builtins::BuiltinId::BtreeMapLastKeyCloned),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(id) => Err(RuntimeError::new(
                format!("unknown runtime member ID {:#x}", id.as_raw()),
                span,
            )),
        }
    }
}

pub(super) fn format_ok() -> Value {
    Value::Result {
        value: Ok(Rc::new(Value::Unit)),
        ok_type: Some(Type::Unit),
        error_type: Some(Type::named("FormatError")),
    }
}
