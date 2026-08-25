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

    fn format_value(
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
        let self_slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
        self_slot.borrow_mut().initialize(value.clone());
        let formatter = Value::HostObject(Rc::new(crate::value::HostObject {
            type_definition: Rc::new(crate::value::HostType {
                name: "Formatter".into(),
                base_types: HashSet::new(),
                copy: false,
                methods: RefCell::new(HashMap::new()),
            }),
            payload: Rc::new(buffer),
        }));
        let formatter_slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        formatter_slot.borrow_mut().initialize(formatter);
        let arguments = vec![
            Value::Reference(Rc::new(ReferenceValue::new_storage(self_slot, false))),
            Value::Reference(Rc::new(ReferenceValue::new_storage(formatter_slot, true))),
        ];
        let result = VirtualMachine::new_call(
            self.module,
            self.imports.clone(),
            self.host_value_formatter.clone(),
            crate::ExecutionLimits {
                max_steps: self.max_steps.saturating_sub(self.steps),
                max_call_depth: remaining_call_depth,
            },
            function,
            arguments,
        )?
        .execute()?;
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
        let target = match value {
            Value::Reference(reference) => reference.read().ok()?.type_name(),
            value => value.type_name(),
        };
        self.module
            .trait_implementations
            .iter()
            .find(|implementation| {
                implementation.target.rsplit("::").next() == Some(target.as_str())
                    && trait_name_matches(&implementation.trait_name, trait_name)
            })?
            .methods
            .get("fmt")
            .copied()
    }

    pub(super) fn write_derived_debug_import(
        &self,
        formatter: &Value,
        value: &Value,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let buffer = crate::formatting::buffer_from_value(formatter)
            .map_err(|message| BytecodeError::new(message, span))?;
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
