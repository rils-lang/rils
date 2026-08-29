use super::*;

pub(super) fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    let Value::String(value) = import_receiver(&arguments[0])? else {
        return Err("string method receiver is not string".into());
    };
    let argument = |index: usize| match arguments.get(index) {
        Some(Value::String(value)) => Ok(value.as_ref()),
        Some(value) => Err(format!(
            "string argument must be string, found {}",
            value.type_name()
        )),
        None => Err("missing string argument".into()),
    };
    match id {
        BuiltinId::StringStartsWith => Ok(Value::Bool(value.starts_with(argument(1)?))),
        BuiltinId::StringEndsWith => Ok(Value::Bool(value.ends_with(argument(1)?))),
        BuiltinId::StringFind => Ok(Value::Option {
            value: value
                .find(argument(1)?)
                .map(|offset| Rc::new(Value::Usize(offset))),
            element_type: Some(Type::USIZE),
        }),
        BuiltinId::StringTrim => Ok(Value::String(Rc::from(value.trim()))),
        BuiltinId::StringTrimStart => Ok(Value::String(Rc::from(value.trim_start()))),
        BuiltinId::StringTrimEnd => Ok(Value::String(Rc::from(value.trim_end()))),
        BuiltinId::StringToLowercase => Ok(Value::String(Rc::from(value.to_lowercase()))),
        BuiltinId::StringToUppercase => Ok(Value::String(Rc::from(value.to_uppercase()))),
        BuiltinId::StringRepeat => {
            let Some(Value::Usize(count)) = arguments.get(1) else {
                return Err("string repeat count must be usize".into());
            };
            Ok(Value::String(Rc::from(value.repeat(*count))))
        }
        BuiltinId::StringRfind => Ok(Value::Option {
            value: value
                .rfind(argument(1)?)
                .map(|offset| Rc::new(Value::Usize(offset))),
            element_type: Some(Type::USIZE),
        }),
        BuiltinId::StringStripPrefix | BuiltinId::StringStripSuffix => {
            let pattern = argument(1)?;
            let stripped = if id == BuiltinId::StringStripPrefix {
                value.strip_prefix(pattern)
            } else {
                value.strip_suffix(pattern)
            };
            Ok(Value::Option {
                value: stripped.map(|text| Rc::new(Value::String(Rc::from(text)))),
                element_type: Some(Type::String),
            })
        }
        BuiltinId::StringChars => Ok(sequence_iterator_value(
            value.chars().map(Value::Char).collect(),
            Type::Char,
        )),
        BuiltinId::StringBytes => Ok(sequence_iterator_value(
            value.bytes().map(Value::U8).collect(),
            Type::Integer(IntegerType::U8),
        )),
        BuiltinId::StringLines => Ok(sequence_iterator_value(
            value
                .lines()
                .map(|line| Value::String(Rc::from(line)))
                .collect(),
            Type::String,
        )),
        BuiltinId::StringSplit => Ok(sequence_iterator_value(
            value
                .split(argument(1)?)
                .map(|part| Value::String(Rc::from(part)))
                .collect(),
            Type::String,
        )),
        BuiltinId::StringReplace => {
            let (Value::String(pattern), Value::String(replacement)) =
                (&arguments[1], &arguments[2])
            else {
                return Err("string replace arguments must be string".into());
            };
            Ok(Value::String(Rc::from(
                value.replace(pattern.as_ref(), replacement.as_ref()),
            )))
        }
        _ => unreachable!("string built-in was matched above"),
    }
}
