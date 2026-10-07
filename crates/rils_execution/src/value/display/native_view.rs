//! Formatting borrowed composed layouts without cloning their children.

use std::fmt::{self, Formatter};

use rils_value::DynamicValueRef;

use crate::Type;

use super::super::DynamicObject;

pub(super) fn display_reference(
    reference: &super::super::ReferenceValue,
    formatter: &mut Formatter<'_>,
) -> fmt::Result {
    format_reference(reference, formatter, false)
}

pub(super) fn debug_reference(
    reference: &super::super::ReferenceValue,
    formatter: &mut Formatter<'_>,
) -> fmt::Result {
    format_reference(reference, formatter, true)
}

fn format_reference(
    reference: &super::super::ReferenceValue,
    formatter: &mut Formatter<'_>,
    debug: bool,
) -> fmt::Result {
    match reference.with_native_view(|view| format_view(view, formatter, debug, true)) {
        Ok(result) => result,
        Err(_) => match reference.read() {
            Ok(value) if debug => fmt::Debug::fmt(&value, formatter),
            Ok(value) => fmt::Display::fmt(&value, formatter),
            Err(_) => formatter.write_str("<invalid reference>"),
        },
    }
}

pub(super) fn display(object: &DynamicObject, formatter: &mut Formatter<'_>) -> fmt::Result {
    format_object(object, formatter, false)
}

pub(super) fn debug(object: &DynamicObject, formatter: &mut Formatter<'_>) -> fmt::Result {
    format_object(object, formatter, true)
}

fn format_object(
    object: &DynamicObject,
    formatter: &mut Formatter<'_>,
    debug: bool,
) -> fmt::Result {
    match object.with(|payload| {
        let view = payload.view();
        if view.layout().is_err() {
            return formatter.write_str("<moved>");
        }
        format_view(view, formatter, debug, true)
    }) {
        Ok(result) => result,
        Err(_) => write!(formatter, "<{}>", object.descriptor().layout().rils_type()),
    }
}

fn format_view(
    view: DynamicValueRef<'_>,
    formatter: &mut Formatter<'_>,
    debug: bool,
    tuple_trailing_comma: bool,
) -> fmt::Result {
    let layout = view.layout().map_err(|_| fmt::Error)?;
    if let Some(result) = rils_stdlib::native::registry().format_view(
        &view,
        formatter,
        debug,
        format_registered_child,
    ) {
        return result.map_err(|_| fmt::Error)?;
    }
    match layout.rils_type() {
        Type::Unit => formatter.write_str("()"),
        Type::Option(_) => {
            if !view.option_is_some().map_err(|_| fmt::Error)? {
                return formatter.write_str("None");
            }
            formatter.write_str("Some(")?;
            format_view(
                view.option_item().map_err(|_| fmt::Error)?,
                formatter,
                debug,
                true,
            )?;
            formatter.write_str(")")
        }
        Type::Result(_, _) => {
            let index = view.variant_index().map_err(|_| fmt::Error)?;
            formatter.write_str(match index {
                0 => "Ok(",
                1 => "Err(",
                _ => return Err(fmt::Error),
            })?;
            format_view(
                view.variant_payload().map_err(|_| fmt::Error)?,
                formatter,
                debug,
                true,
            )?;
            formatter.write_str(")")
        }
        Type::Named { name, .. } if layout.variant_alternatives().is_some() => {
            let index = view.variant_index().map_err(|_| fmt::Error)?;
            let variant = layout.variant_name(index).ok_or(fmt::Error)?;
            let payload = view.variant_payload().map_err(|_| fmt::Error)?;
            let payload_layout = payload.layout().map_err(|_| fmt::Error)?;
            if matches!(payload_layout.rils_type(), Type::Unit) {
                return write!(formatter, "{name}::{variant}");
            }
            if matches!(payload_layout.rils_type(), Type::Named { .. }) {
                return format_view(payload, formatter, debug, true);
            }
            write!(formatter, "{name}::{variant}")?;
            format_view(payload, formatter, debug, false)
        }
        ty if layout.record_fields().is_some() => {
            let fields = layout.record_fields().expect("checked record layout");
            let (prefix, open, close, named) = match ty {
                Type::Named { name, .. } => (name.as_str(), " { ", " }", true),
                Type::Tuple(_) => ("", "(", ")", false),
                Type::Array { .. } => ("", "[", "]", false),
                _ => return write!(formatter, "<{ty}>"),
            };
            formatter.write_str(prefix)?;
            formatter.write_str(open)?;
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    formatter.write_str(", ")?;
                }
                if named {
                    write!(formatter, "{}: ", field.name())?;
                }
                match view.field(index) {
                    Ok(child) => format_view(child, formatter, debug, true)?,
                    Err(_) => formatter.write_str("<moved>")?,
                }
            }
            if tuple_trailing_comma && matches!(ty, Type::Tuple(_)) && fields.len() == 1 {
                formatter.write_str(",")?;
            }
            formatter.write_str(close)
        }
        ty => {
            if layout.is_rust_type::<std::rc::Rc<super::super::ReferenceValue>>() {
                return view
                    .with_rust::<std::rc::Rc<super::super::ReferenceValue>, _>(|reference| {
                        format_reference(reference, formatter, debug)
                    })
                    .map_err(|_| fmt::Error)?;
            }
            write!(formatter, "<{ty}>")
        }
    }
}

fn format_registered_child(
    view: DynamicValueRef<'_>,
    formatter: &mut Formatter<'_>,
    debug: bool,
) -> fmt::Result {
    format_view(view, formatter, debug, true)
}
