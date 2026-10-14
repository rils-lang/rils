use rils_frontend::format::{
    FormatAlignment, FormatKind, FormatPiece, FormatSpec, parse_format_string,
};

use std::rc::Rc;

pub use rils_stdlib::stdlib::fmt::FormatterBuffer;

use crate::Value;

/// Prepare the shared receiver expected by a formatting trait method.
pub fn receiver_reference(value: &Value) -> Result<Value, String> {
    use crate::value::{ReferenceValue, borrowed::Read};
    if let Some(mut reference) = value.as_reference() {
        loop {
            let nested = reference.with_borrowed_target(|target| match target {
                Read::View(view) if view.layout()?.is_rust_type::<Rc<ReferenceValue>>() => {
                    view.with_rust::<Rc<ReferenceValue>, _>(Rc::clone).map(Some)
                }
                Read::Leaf(leaf) if leaf.is_rust_type::<Rc<ReferenceValue>>() => {
                    leaf.with_rust::<Rc<ReferenceValue>, _>(Rc::clone).map(Some)
                }
                _ => Ok(None),
            })??;
            match nested {
                Some(inner) => reference = inner,
                None => return Ok(Value::from_reference(reference)),
            }
        }
    }
    let slot = Rc::new(std::cell::RefCell::new(
        crate::environment::StorageSlot::uninitialized(false),
    ));
    slot.borrow_mut().initialize(value.clone());
    Ok(Value::from_reference(Rc::new(ReferenceValue::new_storage(
        slot, false,
    ))))
}

pub fn formatter_value(buffer: Rc<FormatterBuffer>) -> Result<Value, String> {
    use rils_value::{DynamicLayout, DynamicType, DynamicValue};
    let ty = crate::Type::named("Formatter");
    let layout = DynamicLayout::of::<rils_stdlib::stdlib::fmt::Formatter>(ty);
    let value = DynamicValue::from_rust(
        layout.clone(),
        rils_stdlib::stdlib::fmt::Formatter::from_buffer(buffer),
    )?;
    crate::value::DynamicObject::new(Rc::new(DynamicType::new(layout)), value).map(Value::Dynamic)
}

pub fn buffer_from_value(value: &Value) -> Result<Rc<FormatterBuffer>, String> {
    use crate::value::borrowed::{Read, with_referent};
    use rils_stdlib::stdlib::fmt::Formatter;
    with_referent(value, |value| match value {
        Read::View(view) => view.with_rust::<Formatter, _>(Formatter::buffer),
        Read::Leaf(leaf) => leaf.with_rust::<Formatter, _>(Formatter::buffer),
        Read::Host(object) => object
            .payload
            .downcast_ref::<Rc<FormatterBuffer>>()
            .cloned()
            .ok_or_else(|| "invalid Formatter payload".into()),
    })?
}

pub fn format_arguments(format: &str, arguments: &[Value]) -> Result<String, String> {
    format_arguments_with(format, arguments, format_value)
}

pub fn format_arguments_with(
    format: &str,
    arguments: &[Value],
    mut render: impl FnMut(&Value, &FormatSpec) -> Result<String, String>,
) -> Result<String, String> {
    let pieces = parse_format_string(format).map_err(|error| error.message)?;
    let mut output = String::new();
    for piece in pieces {
        match piece {
            FormatPiece::Text(text) => output.push_str(&text),
            FormatPiece::Placeholder { argument, spec } => {
                let value = arguments.get(argument).ok_or_else(|| {
                    format!("format placeholder references missing argument {argument}")
                })?;
                output.push_str(&render(value, &spec)?);
            }
        }
    }
    Ok(output)
}

pub fn format_value(value: &Value, spec: &FormatSpec) -> Result<String, String> {
    let rendered = render_value(value, spec)?;
    Ok(apply_width(rendered, spec))
}

pub fn finish_rendered(rendered: String, spec: &FormatSpec) -> String {
    apply_width(rendered, spec)
}

fn render_value(value: &Value, spec: &FormatSpec) -> Result<String, String> {
    Ok(match spec.kind {
        FormatKind::Display => display(value, spec)?,
        FormatKind::Debug if spec.alternate => format!("{value:#?}"),
        FormatKind::Debug => format!("{value:?}"),
        FormatKind::Binary => integer_format(value, IntegerFormat::Binary, spec.alternate)?,
        FormatKind::Octal => integer_format(value, IntegerFormat::Octal, spec.alternate)?,
        FormatKind::LowerHex => integer_format(value, IntegerFormat::LowerHex, spec.alternate)?,
        FormatKind::UpperHex => integer_format(value, IntegerFormat::UpperHex, spec.alternate)?,
        FormatKind::LowerExp => float_format(value, false, spec.precision)?,
        FormatKind::UpperExp => float_format(value, true, spec.precision)?,
    })
}

fn display(value: &Value, spec: &FormatSpec) -> Result<String, String> {
    let receiver_type = crate::runtime_type::receiver_type(value);
    if !crate::value::native_ops::has_method(value, crate::value::native_ops::DISPLAY)?
        && !receiver_type.as_ref().is_some_and(|ty| {
            rils_frontend::format_traits::implements(ty, "Display", &|_, _| false)
        })
        && value.host_type_definition()?.is_none()
    {
        return Err(format!(
            "type `{}` does not implement required trait `Display`",
            value.type_name()
        ));
    }
    let rendered = match (receiver_type, spec.precision) {
        (Some(crate::Type::Float(crate::FloatType::F32)), Some(precision)) => {
            Ok(format!("{:.precision$}", value.get_cloned::<f32>()?))
        }
        (Some(crate::Type::Float(crate::FloatType::F64)), Some(precision)) => {
            Ok(format!("{:.precision$}", value.get_cloned::<f64>()?))
        }
        (Some(crate::Type::String), Some(precision)) => {
            value.with_ref::<String, _>(|text| text.chars().take(precision).collect())
        }
        (_, Some(_)) => Err(format!(
            "format precision is not supported for `{}`",
            value.type_name()
        )),
        _ => Ok(value.to_string()),
    }?;
    if spec.sign_plus && is_nonnegative_number(value) {
        Ok(format!("+{rendered}"))
    } else {
        Ok(rendered)
    }
}

fn is_nonnegative_number(value: &Value) -> bool {
    if let Ok(value) = value.get_cloned::<i8>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<i16>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<i32>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<i64>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<i128>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<isize>() {
        return value >= 0;
    }
    if let Ok(value) = value.get_cloned::<f32>() {
        return value >= 0.0;
    }
    if let Ok(value) = value.get_cloned::<f64>() {
        return value >= 0.0;
    }
    value.get_cloned::<u8>().ok().is_some()
        || value.get_cloned::<u16>().ok().is_some()
        || value.get_cloned::<u32>().ok().is_some()
        || value.get_cloned::<u64>().ok().is_some()
        || value.get_cloned::<u128>().ok().is_some()
        || value.get_cloned::<usize>().ok().is_some()
}

#[derive(Clone, Copy)]
enum IntegerFormat {
    Binary,
    Octal,
    LowerHex,
    UpperHex,
}

fn integer_format(value: &Value, kind: IntegerFormat, alternate: bool) -> Result<String, String> {
    macro_rules! render {
        ($value:expr) => {{
            match (kind, alternate) {
                (IntegerFormat::Binary, false) => format!("{:b}", $value),
                (IntegerFormat::Binary, true) => format!("{:#b}", $value),
                (IntegerFormat::Octal, false) => format!("{:o}", $value),
                (IntegerFormat::Octal, true) => format!("{:#o}", $value),
                (IntegerFormat::LowerHex, false) => format!("{:x}", $value),
                (IntegerFormat::LowerHex, true) => format!("{:#x}", $value),
                (IntegerFormat::UpperHex, false) => format!("{:X}", $value),
                (IntegerFormat::UpperHex, true) => format!("{:#X}", $value),
            }
        }};
    }
    if let Ok(value) = value.get_cloned::<i8>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<i16>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<i32>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<i64>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<i128>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<isize>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<u8>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<u16>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<u32>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<u64>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<u128>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<usize>() {
        return Ok(render!(value));
    }
    Err(format!(
        "integer formatting is not supported for `{}`",
        value.type_name()
    ))
}

fn float_format(value: &Value, upper: bool, precision: Option<usize>) -> Result<String, String> {
    macro_rules! render {
        ($value:expr) => {{
            match (upper, precision) {
                (false, Some(precision)) => format!("{:.precision$e}", $value),
                (true, Some(precision)) => format!("{:.precision$E}", $value),
                (false, None) => format!("{:e}", $value),
                (true, None) => format!("{:E}", $value),
            }
        }};
    }
    if let Ok(value) = value.get_cloned::<f32>() {
        return Ok(render!(value));
    }
    if let Ok(value) = value.get_cloned::<f64>() {
        return Ok(render!(value));
    }
    Err(format!(
        "exponential formatting is not supported for `{}`",
        value.type_name()
    ))
}

fn apply_width(rendered: String, spec: &FormatSpec) -> String {
    let Some(width) = spec.width else {
        return rendered;
    };
    let padding = width.saturating_sub(rendered.chars().count());
    if padding == 0 {
        return rendered;
    }
    if spec.zero_pad {
        let prefix = if rendered.starts_with('+') || rendered.starts_with('-') {
            1
        } else if rendered.starts_with("0x")
            || rendered.starts_with("0X")
            || rendered.starts_with("0b")
            || rendered.starts_with("0o")
        {
            2
        } else {
            0
        };
        let (head, tail) = rendered.split_at(prefix);
        return format!("{head}{}{tail}", "0".repeat(padding));
    }
    let fill = spec.fill.unwrap_or(' ');
    let (left, right) = match spec.alignment {
        FormatAlignment::Left => (0, padding),
        FormatAlignment::Center => (padding / 2, padding - padding / 2),
        FormatAlignment::Right | FormatAlignment::Unspecified => (padding, 0),
    };
    format!(
        "{}{}{}",
        fill.to_string().repeat(left),
        rendered,
        fill.to_string().repeat(right)
    )
}

/// The standard formatter success result uses the registered native layouts.
pub fn format_ok() -> Result<Value, String> {
    crate::value::storage::TypedStorageContext::new(&[], &[]).construct_result(
        &crate::Type::Result(
            Box::new(crate::Type::Unit),
            Box::new(crate::Type::named("FormatError")),
        ),
        Ok(Value::from(crate::value::native_unit())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_common_rust_style_specs() {
        assert_eq!(
            format_arguments(
                "value={:+6} hex={:#x} float={:.2}",
                &[
                    Value::from(crate::value::native_i32(12)),
                    Value::from(crate::value::native_u8(15)),
                    Value::from(crate::value::native_f64(1.234))
                ]
            )
            .unwrap(),
            "value=   +12 hex=0xf float=1.23"
        );
        assert_eq!(
            format_arguments(
                "{:+06} {:#06x}",
                &[
                    Value::from(crate::value::native_i32(12)),
                    Value::from(crate::value::native_u8(15))
                ]
            )
            .unwrap(),
            "+00012 0x000f"
        );
        assert_eq!(
            format_arguments("{:?}", &[Value::from(crate::value::native_string("hello"))]).unwrap(),
            "\"hello\""
        );
        assert_eq!(
            format_arguments("{:.2}", &[Value::from(crate::value::native_f32(1.234))]).unwrap(),
            "1.23"
        );
    }
}
