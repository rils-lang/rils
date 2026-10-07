use super::*;
use crate::formatting::FormatterBuffer;
use rils_frontend::format::{FormatKind, FormatSpec};

impl VirtualMachine<'_> {
    pub(super) fn format_import_arguments(
        &self,
        format: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<String, BytecodeError> {
        crate::formatting::format_arguments_with(format, arguments, |value, spec| {
            self.format_value(value, spec, span)
                .map_err(|error| error.message)
        })
        .map_err(|message| BytecodeError::new(message, span))
    }

    pub(super) fn format_value(
        &self,
        value: &Value,
        spec: &FormatSpec,
        span: Span,
    ) -> Result<String, BytecodeError> {
        if matches!(value, Value::HostObject(_))
            && let Some(formatter) = &self.host_value_formatter
        {
            let kind = match spec.kind {
                FormatKind::Display => Some(crate::HostFormatKind::Display),
                FormatKind::Debug => Some(crate::HostFormatKind::Debug),
                _ => None,
            };
            if let Some(kind) = kind
                && let Some(rendered) = formatter(
                    value,
                    crate::HostFormatSpec {
                        kind,
                        alternate: spec.alternate,
                        precision: spec.precision,
                    },
                )
                .map_err(|message| BytecodeError::new(message, span))?
            {
                return Ok(crate::formatting::finish_rendered(rendered, spec));
            }
        }
        let trait_name = match spec.kind {
            FormatKind::Display => Some("Display"),
            FormatKind::Debug => Some("Debug"),
            _ => None,
        };
        let Some(trait_name) = trait_name else {
            return crate::formatting::format_value(value, spec)
                .map_err(|message| BytecodeError::new(message, span));
        };
        let Some(function) = self.format_method(value, trait_name) else {
            return crate::formatting::format_value(value, spec)
                .map_err(|message| BytecodeError::new(message, span));
        };
        let buffer = Rc::new(FormatterBuffer::new(spec.alternate));
        self.execute_format_method(function, value, buffer.clone(), span)?;
        Ok(crate::formatting::finish_rendered(buffer.finish(), spec))
    }

    fn execute_format_method(
        &self,
        function: usize,
        value: &Value,
        buffer: Rc<FormatterBuffer>,
        span: Span,
    ) -> Result<(), BytecodeError> {
        let remaining_call_depth = self.max_call_depth.saturating_sub(self.frames.len());
        if remaining_call_depth == 0 {
            return Err(BytecodeError::new(
                "formatting exceeded the bytecode call depth limit",
                span,
            ));
        }
        let self_reference = match value {
            Value::Reference(reference) => Value::Reference(reference.clone()),
            value => {
                let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
                slot.borrow_mut().initialize(value.clone());
                Value::Reference(Rc::new(ReferenceValue::new_storage(slot, false)))
            }
        };
        let formatter = crate::formatting::formatter_value(buffer)
            .map_err(|message| BytecodeError::new(message, span))?;
        let formatter_slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        formatter_slot.borrow_mut().initialize(formatter);
        let arguments = vec![
            self_reference,
            Value::Reference(Rc::new(ReferenceValue::new_storage(formatter_slot, true))),
        ];
        let result = VirtualMachine::new_call(
            self.module,
            self.imports.clone(),
            self.host_value_formatter.clone(),
            self.native_context.clone(),
            crate::ExecutionLimits {
                max_steps: self.max_steps.saturating_sub(self.steps),
                max_call_depth: remaining_call_depth,
            },
            function,
            arguments,
        )?
        .execute()?;
        let result = crate::value::owned_sum::materialize(result, &[], &[])
            .map_err(|message| BytecodeError::new(message, span))?;
        match result {
            Value::Result {
                value: Ok(value), ..
            } if matches!(value.as_ref(), Value::Unit) => Ok(()),
            Value::Result {
                value: Err(error), ..
            } => Err(BytecodeError::new(
                format!("formatting failed: {error}"),
                span,
            )),
            value => Err(BytecodeError::new(
                format!(
                    "format method returned {}, expected Result<(), FormatError>",
                    value.type_name()
                ),
                span,
            )),
        }
    }

    fn format_method(&self, value: &Value, trait_name: &str) -> Option<usize> {
        let ty = Type::of_value(value)?;
        let target = match &ty {
            Type::Named { name, .. } => name.clone(),
            Type::Reference { inner, .. } => match inner.as_ref() {
                Type::Named { name, .. } => name.clone(),
                ty => ty.to_string(),
            },
            ty => ty.to_string(),
        };
        self.module
            .trait_implementations
            .iter()
            .find(|implementation| {
                implementation.target == target
                    && trait_name_matches(&implementation.trait_name, trait_name)
            })?
            .methods
            .get("fmt")
            .copied()
    }

    pub(super) fn write_derived_debug_builtin(
        &self,
        formatter: &Value,
        value: &Value,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let buffer = crate::formatting::buffer_from_value(formatter)
            .map_err(|message| BytecodeError::new(message, span))?;
        if matches!(
            rils_execution::value::native_instance::value_definition(value)
                .map_err(|message| BytecodeError::new(message, span))?,
            Some(Value::StructType(_) | Value::EnumType(_))
        ) {
            self.write_structural_debug(&buffer, value, span)?;
            return Ok(format_ok());
        }
        let value = match value {
            Value::Reference(reference) => reference
                .read()
                .map_err(|message| BytecodeError::new(message, span))?,
            value => value.clone(),
        };
        self.write_structural_debug(&buffer, &value, span)?;
        Ok(format_ok())
    }

    fn write_structural_debug(
        &self,
        buffer: &Rc<FormatterBuffer>,
        value: &Value,
        span: Span,
    ) -> Result<(), BytecodeError> {
        let (name, fields, tuple) = match value {
            value
                if rils_execution::value::native_instance::enum_variant(value)
                    .map_err(|message| BytecodeError::new(message, span))?
                    .is_some() =>
            {
                let variant = rils_execution::value::native_instance::enum_variant(value)
                    .map_err(|message| BytecodeError::new(message, span))?
                    .expect("checked enum");
                let name = format!("{}::{}", variant.definition.name, variant.name());
                if matches!(variant.declaration(), crate::ast::EnumVariant::Unit { .. }) {
                    buffer.write_str(&name);
                    return Ok(());
                }
                let tuple = matches!(variant.declaration(), crate::ast::EnumVariant::Tuple { .. });
                let fields = variant
                    .field_names()
                    .into_iter()
                    .map(|field| {
                        rils_execution::value::native_instance::borrow_variant_field(
                            value,
                            variant.index,
                            &field,
                        )
                        .map(|value| (if tuple { None } else { Some(field) }, value))
                        .map_err(|message| BytecodeError::new(message, span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (name, fields, tuple)
            }
            value
                if rils_execution::value::native_instance::record_definition(value)
                    .map_err(|message| BytecodeError::new(message, span))?
                    .is_some() =>
            {
                let definition = rils_execution::value::native_instance::record_definition(value)
                    .map_err(|message| BytecodeError::new(message, span))?
                    .expect("checked record");
                let fields = definition
                    .fields
                    .iter()
                    .map(|field| {
                        rils_execution::value::native_instance::borrow_field(value, &field.name)
                            .map(|value| (Some(field.name.clone()), value))
                            .map_err(|message| BytecodeError::new(message, span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (definition.name.clone(), fields, false)
            }
            Value::Struct(instance) => {
                let slots = instance.fields.borrow();
                let values = instance
                    .type_definition
                    .fields
                    .iter()
                    .map(|field| {
                        slots
                            .get(&field.name)
                            .and_then(|slot| slot.value.clone())
                            .map(|value| (Some(field.name.clone()), value))
                            .ok_or_else(|| {
                                BytecodeError::new(
                                    format!("cannot format moved field `{}`", field.name),
                                    span,
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (instance.type_definition.name.clone(), values, false)
            }
            Value::Enum(instance) => match &instance.payload {
                EnumPayload::Unit => {
                    buffer.write_str(&format!(
                        "{}::{}",
                        instance.type_definition.name, instance.variant
                    ));
                    return Ok(());
                }
                EnumPayload::Tuple(values) => (
                    format!("{}::{}", instance.type_definition.name, instance.variant),
                    values.iter().cloned().map(|value| (None, value)).collect(),
                    true,
                ),
                EnumPayload::Record(values) => (
                    format!("{}::{}", instance.type_definition.name, instance.variant),
                    values
                        .iter()
                        .map(|(name, value)| (Some(name.clone()), value.clone()))
                        .collect(),
                    false,
                ),
            },
            value => {
                return Err(BytecodeError::new(
                    format!("derived Debug cannot format `{}`", value.type_name()),
                    span,
                ));
            }
        };
        buffer.write_str(&name);
        buffer.write_str(if tuple { "(" } else { " {" });
        let depth = buffer.depth();
        buffer.set_depth(depth + 1);
        for (index, (field, value)) in fields.iter().enumerate() {
            if buffer.alternate() {
                buffer.write_str("\n");
                buffer.write_str(&"    ".repeat(depth + 1));
            } else if index > 0 {
                buffer.write_str(", ");
            } else if !tuple {
                buffer.write_str(" ");
            }
            if let Some(field) = field {
                buffer.write_str(field);
                buffer.write_str(": ");
            }
            if let Some(function) = self.format_method(value, "Debug") {
                self.execute_format_method(function, value, buffer.clone(), span)?;
            } else {
                let spec = FormatSpec {
                    kind: FormatKind::Debug,
                    alternate: buffer.alternate(),
                    ..FormatSpec::default()
                };
                buffer.write_str(
                    &crate::formatting::format_value(value, &spec)
                        .map_err(|message| BytecodeError::new(message, span))?,
                );
            }
            if buffer.alternate() {
                buffer.write_str(",");
            }
        }
        buffer.set_depth(depth);
        if buffer.alternate() && !fields.is_empty() {
            buffer.write_str("\n");
            buffer.write_str(&"    ".repeat(depth));
        } else if !tuple && !fields.is_empty() {
            buffer.write_str(" ");
        }
        buffer.write_str(if tuple { ")" } else { "}" });
        Ok(())
    }
}

pub(super) fn format_ok() -> Value {
    Value::Result {
        value: Ok(Rc::new(Value::Unit)),
        ok_type: Some(Type::Unit),
        error_type: Some(Type::named("FormatError")),
    }
}
