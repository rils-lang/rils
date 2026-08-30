use super::*;
use crate::formatting::FormatterBuffer;
use rils_frontend::format::{FormatKind, FormatSpec};

impl Interpreter {
    pub(super) fn format_arguments(
        &mut self,
        format: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<String, RuntimeError> {
        crate::formatting::format_arguments_with(format, arguments, |value, spec| {
            self.format_value(value, spec, span)
                .map_err(|error| error.message)
        })
        .map_err(|message| RuntimeError::new(message, span))
    }

    fn format_value(
        &mut self,
        value: &Value,
        spec: &FormatSpec,
        span: Span,
    ) -> Result<String, RuntimeError> {
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
                .map_err(|message| RuntimeError::new(message, span))?
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
                .map_err(|message| RuntimeError::new(message, span));
        };
        if self.trait_format_method(value, trait_name).is_none() {
            return crate::formatting::format_value(value, spec)
                .map_err(|message| RuntimeError::new(message, span));
        }

        let buffer = Rc::new(FormatterBuffer::new(spec.alternate));
        self.call_format_method(value, trait_name, buffer.clone(), span)?;
        Ok(crate::formatting::finish_rendered(buffer.finish(), spec))
    }

    fn call_format_method(
        &mut self,
        value: &Value,
        trait_name: &str,
        buffer: Rc<FormatterBuffer>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let function = self.trait_format_method(value, trait_name).ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type `{}` does not implement `{trait_name}`",
                    value.type_name()
                ),
                span,
            )
        })?;
        let self_storage = Rc::new(RefCell::new(
            crate::environment::StorageSlot::uninitialized(false),
        ));
        self_storage.borrow_mut().initialize(value.clone());
        let self_reference =
            Value::Reference(Rc::new(ReferenceValue::new_storage(self_storage, false)));
        let formatter_value = Value::HostObject(Rc::new(HostObject {
            type_definition: self.formatter_type(span)?,
            payload: Rc::new(buffer),
        }));
        let formatter_storage = Rc::new(RefCell::new(
            crate::environment::StorageSlot::uninitialized(true),
        ));
        formatter_storage.borrow_mut().initialize(formatter_value);
        let formatter_reference = Value::Reference(Rc::new(ReferenceValue::new_storage(
            formatter_storage,
            true,
        )));
        let result = self.call(
            Value::Function(function),
            &[self_reference, formatter_reference],
            span,
        )?;
        match result {
            Value::Result {
                value: Ok(value), ..
            } if matches!(value.as_ref(), Value::Unit) => Ok(()),
            Value::Result {
                value: Err(error), ..
            } => Err(RuntimeError::new(
                format!("formatting failed: {error}"),
                span,
            )),
            value => Err(RuntimeError::new(
                format!(
                    "{trait_name}::fmt returned {}, expected Result<(), FormatError>",
                    value.type_name()
                ),
                span,
            )),
        }
    }

    fn trait_format_method(&self, value: &Value, trait_name: &str) -> Option<Rc<UserFunction>> {
        let value = match value {
            Value::Reference(reference) => reference.read().ok()?,
            value => value.clone(),
        };
        match value {
            Value::Struct(instance) => instance
                .type_definition
                .trait_methods
                .borrow()
                .get(trait_name)
                .and_then(|methods| methods.get("fmt"))
                .cloned(),
            Value::Enum(instance) => instance
                .type_definition
                .trait_methods
                .borrow()
                .get(trait_name)
                .and_then(|methods| methods.get("fmt"))
                .cloned(),
            _ => None,
        }
    }

    fn formatter_type(&self, span: Span) -> Result<Rc<HostType>, RuntimeError> {
        match self.globals.borrow().get("Formatter") {
            Some(Value::HostType(definition)) => Ok(definition),
            _ => Err(RuntimeError::new(
                "Formatter runtime type is unavailable",
                span,
            )),
        }
    }

    pub(super) fn write_derived_debug(
        &mut self,
        formatter: &Value,
        value: &Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let buffer = formatter_buffer(formatter, span)?;
        let value = dereference_value(value, span)?;
        match value {
            Value::Struct(instance) => {
                let fields = instance.fields.borrow();
                let mut values = Vec::with_capacity(instance.type_definition.fields.len());
                for field in &instance.type_definition.fields {
                    let value = fields
                        .get(&field.name)
                        .and_then(|slot| slot.value.clone())
                        .ok_or_else(|| {
                            RuntimeError::new(
                                format!("cannot format moved field `{}`", field.name),
                                span,
                            )
                        })?;
                    values.push((field.name.clone(), value));
                }
                self.write_debug_record(&buffer, &instance.type_definition.name, &values, span)
            }
            Value::Enum(instance) => match &instance.payload {
                EnumPayload::Unit => {
                    buffer.write_str(&format!(
                        "{}::{}",
                        instance.type_definition.name, instance.variant
                    ));
                    Ok(())
                }
                EnumPayload::Tuple(values) => {
                    let name = format!("{}::{}", instance.type_definition.name, instance.variant);
                    self.write_debug_tuple(&buffer, &name, values, span)
                }
                EnumPayload::Record(values) => {
                    let variant = instance
                        .type_definition
                        .variants
                        .iter()
                        .find(|variant| enum_variant_name(variant) == instance.variant)
                        .ok_or_else(|| {
                            RuntimeError::new("enum variant metadata is unavailable", span)
                        })?;
                    let EnumVariant::Record { fields, .. } = variant else {
                        return Err(RuntimeError::new(
                            "enum payload does not match its metadata",
                            span,
                        ));
                    };
                    let ordered = fields
                        .iter()
                        .map(|field| {
                            values
                                .get(&field.name)
                                .cloned()
                                .map(|value| (field.name.clone(), value))
                                .ok_or_else(|| {
                                    RuntimeError::new(
                                        format!("cannot format moved field `{}`", field.name),
                                        span,
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let name = format!("{}::{}", instance.type_definition.name, instance.variant);
                    self.write_debug_record(&buffer, &name, &ordered, span)
                }
            },
            value => Err(RuntimeError::new(
                format!("derived Debug cannot format `{}`", value.type_name()),
                span,
            )),
        }
    }

    fn write_debug_record(
        &mut self,
        buffer: &Rc<FormatterBuffer>,
        name: &str,
        fields: &[(String, Value)],
        span: Span,
    ) -> Result<(), RuntimeError> {
        buffer.write_str(name);
        buffer.write_str(" {");
        let depth = buffer.depth();
        buffer.set_depth(depth + 1);
        for (index, (field, value)) in fields.iter().enumerate() {
            if buffer.alternate() {
                buffer.write_str("\n");
                buffer.write_str(&"    ".repeat(depth + 1));
            } else if index > 0 {
                buffer.write_str(", ");
            } else {
                buffer.write_str(" ");
            }
            buffer.write_str(field);
            buffer.write_str(": ");
            self.write_debug_value(buffer.clone(), value, span)?;
            if buffer.alternate() {
                buffer.write_str(",");
            }
        }
        buffer.set_depth(depth);
        if buffer.alternate() && !fields.is_empty() {
            buffer.write_str("\n");
            buffer.write_str(&"    ".repeat(depth));
        } else if !fields.is_empty() {
            buffer.write_str(" ");
        }
        buffer.write_str("}");
        Ok(())
    }

    fn write_debug_tuple(
        &mut self,
        buffer: &Rc<FormatterBuffer>,
        name: &str,
        values: &[Value],
        span: Span,
    ) -> Result<(), RuntimeError> {
        buffer.write_str(name);
        buffer.write_str("(");
        let depth = buffer.depth();
        buffer.set_depth(depth + 1);
        for (index, value) in values.iter().enumerate() {
            if buffer.alternate() {
                buffer.write_str("\n");
                buffer.write_str(&"    ".repeat(depth + 1));
            } else if index > 0 {
                buffer.write_str(", ");
            }
            self.write_debug_value(buffer.clone(), value, span)?;
            if buffer.alternate() {
                buffer.write_str(",");
            }
        }
        buffer.set_depth(depth);
        if buffer.alternate() && !values.is_empty() {
            buffer.write_str("\n");
            buffer.write_str(&"    ".repeat(depth));
        }
        buffer.write_str(")");
        Ok(())
    }

    fn write_debug_value(
        &mut self,
        buffer: Rc<FormatterBuffer>,
        value: &Value,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if self.trait_format_method(value, "Debug").is_some() {
            return self.call_format_method(value, "Debug", buffer, span);
        }
        let spec = FormatSpec {
            kind: FormatKind::Debug,
            alternate: buffer.alternate(),
            ..FormatSpec::default()
        };
        let rendered = crate::formatting::format_value(value, &spec)
            .map_err(|message| RuntimeError::new(message, span))?;
        buffer.write_str(&rendered);
        Ok(())
    }
}

pub(super) fn formatter_buffer(
    value: &Value,
    span: Span,
) -> Result<Rc<FormatterBuffer>, RuntimeError> {
    crate::formatting::buffer_from_value(value).map_err(|message| RuntimeError::new(message, span))
}

fn dereference_value(value: &Value, span: Span) -> Result<Value, RuntimeError> {
    match value {
        Value::Reference(reference) => reference
            .read()
            .map_err(|message| RuntimeError::new(message, span)),
        value => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate_binding(source: &str, name: &str) -> (Interpreter, Value) {
        let tokens = crate::lexer::lex(source).expect("source should lex");
        let program =
            crate::parser::parse_with_native_macros(tokens, crate::macros::STANDARD_NATIVE_MACROS)
                .expect("source should parse");
        let analysis = rils_frontend::analysis::analyze_program(&program);
        let mut interpreter = Interpreter::new();
        interpreter
            .execute_with_analysis(&program, &analysis)
            .expect("source should execute");
        let value = interpreter
            .globals
            .borrow()
            .get(name)
            .expect("binding should exist");
        (interpreter, value)
    }

    #[test]
    fn custom_display_writes_through_formatter() {
        let (mut interpreter, value) = evaluate_binding(
            r#"
                struct Label { value: i32 }
                impl Display for Label {
                    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
                        formatter.write_str("custom label")
                    }
                }
                let label = Label { value: 1 };
            "#,
            "label",
        );
        assert_eq!(
            interpreter
                .format_arguments("value={}", &[value], Span::default())
                .unwrap(),
            "value=custom label"
        );
    }

    #[test]
    fn derived_debug_uses_nested_custom_debug() {
        let (mut interpreter, value) = evaluate_binding(
            r#"
                struct Leaf { value: i32 }
                impl Debug for Leaf {
                    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
                        formatter.write_str("special leaf")
                    }
                }
                #[derive(Debug)]
                struct Wrapper { leaf: Leaf }
                let wrapper = Wrapper { leaf: Leaf { value: 1 } };
            "#,
            "wrapper",
        );
        assert_eq!(
            interpreter
                .format_arguments("{:?}", &[value], Span::default())
                .unwrap(),
            "Wrapper { leaf: special leaf }"
        );
    }
}
