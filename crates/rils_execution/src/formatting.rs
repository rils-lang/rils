use rils_frontend::format::{
    FormatAlignment, FormatKind, FormatPiece, FormatSpec, parse_format_string,
};

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::Value;

#[derive(Default)]
pub struct FormatterBuffer {
    output: RefCell<String>,
    alternate: Cell<bool>,
    depth: Cell<usize>,
}

impl FormatterBuffer {
    pub fn new(alternate: bool) -> Self {
        Self {
            output: RefCell::new(String::new()),
            alternate: Cell::new(alternate),
            depth: Cell::new(0),
        }
    }

    pub fn write_str(&self, value: &str) {
        self.output.borrow_mut().push_str(value);
    }

    pub fn finish(&self) -> String {
        self.output.borrow().clone()
    }

    pub fn alternate(&self) -> bool {
        self.alternate.get()
    }

    pub fn depth(&self) -> usize {
        self.depth.get()
    }

    pub fn set_depth(&self, depth: usize) {
        self.depth.set(depth);
    }
}

pub fn buffer_from_value(value: &Value) -> Result<Rc<FormatterBuffer>, String> {
    let mut value = value.clone();
    while let Value::Reference(reference) = value {
        value = reference.read()?;
    }
    let Value::HostObject(object) = value else {
        return Err("formatter receiver is not a Formatter".into());
    };
    object
        .payload
        .downcast_ref::<Rc<FormatterBuffer>>()
        .cloned()
        .ok_or_else(|| "invalid Formatter payload".into())
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
    let rendered = match (value, spec.precision) {
        (Value::F32(value), Some(precision)) => Ok(format!("{value:.precision$}")),
        (Value::F64(value), Some(precision)) => Ok(format!("{value:.precision$}")),
        (Value::String(value), Some(precision)) => Ok(value.chars().take(precision).collect()),
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
    match value {
        Value::I8(value) => *value >= 0,
        Value::I16(value) => *value >= 0,
        Value::I32(value) => *value >= 0,
        Value::I64(value) => *value >= 0,
        Value::I128(value) => *value >= 0,
        Value::Isize(value) => *value >= 0,
        Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_) => true,
        Value::F32(value) => *value >= 0.0,
        Value::F64(value) => *value >= 0.0,
        _ => false,
    }
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
    Ok(match value {
        Value::I8(value) => render!(value),
        Value::I16(value) => render!(value),
        Value::I32(value) => render!(value),
        Value::I64(value) => render!(value),
        Value::I128(value) => render!(value),
        Value::Isize(value) => render!(value),
        Value::U8(value) => render!(value),
        Value::U16(value) => render!(value),
        Value::U32(value) => render!(value),
        Value::U64(value) => render!(value),
        Value::U128(value) => render!(value),
        Value::Usize(value) => render!(value),
        _ => {
            return Err(format!(
                "integer formatting is not supported for `{}`",
                value.type_name()
            ));
        }
    })
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
    match value {
        Value::F32(value) => Ok(render!(value)),
        Value::F64(value) => Ok(render!(value)),
        _ => Err(format!(
            "exponential formatting is not supported for `{}`",
            value.type_name()
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_common_rust_style_specs() {
        assert_eq!(
            format_arguments(
                "value={:+6} hex={:#x} float={:.2}",
                &[Value::I32(12), Value::U8(15), Value::F64(1.234)]
            )
            .unwrap(),
            "value=   +12 hex=0xf float=1.23"
        );
        assert_eq!(
            format_arguments("{:+06} {:#06x}", &[Value::I32(12), Value::U8(15)]).unwrap(),
            "+00012 0x000f"
        );
        assert_eq!(
            format_arguments("{:?}", &[Value::String("hello".into())]).unwrap(),
            "\"hello\""
        );
    }
}
