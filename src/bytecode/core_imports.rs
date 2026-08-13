use super::*;

pub(super) fn core_imports() -> Vec<(&'static str, FunctionSignature)> {
    let shared = || Type::Reference {
        mutable: false,
        inner: Box::new(Type::Unknown),
    };
    let mutable = || Type::Reference {
        mutable: true,
        inner: Box::new(Type::Unknown),
    };
    vec![
        (
            "type_of",
            FunctionSignature::fixed(vec![Type::Unknown], Type::String),
        ),
        (
            "clone",
            FunctionSignature::fixed(
                vec![Type::Reference {
                    mutable: false,
                    inner: Box::new(Type::Unknown),
                }],
                Type::Unknown,
            ),
        ),
        (
            "is_ok",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_err",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_some",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "is_none",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool),
        ),
        (
            "unwrap",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown),
        ),
        (
            "unwrap_or",
            FunctionSignature::fixed(vec![Type::Unknown, Type::Unknown], Type::Unknown),
        ),
        ("core::assert", FunctionSignature::variadic(Type::Unit)),
        (
            "core::vec::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::vec::from",
            FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        (
            "core::sequence::len",
            FunctionSignature::fixed(vec![shared()], Type::USIZE),
        ),
        (
            "core::value::is_empty",
            FunctionSignature::fixed(vec![shared()], Type::Bool),
        ),
        (
            "core::vec::push",
            FunctionSignature::fixed(vec![mutable(), Type::Unknown], Type::Unit),
        ),
        (
            "core::vec::pop",
            FunctionSignature::fixed(vec![mutable()], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::vec::clear",
            FunctionSignature::fixed(vec![mutable()], Type::Unit),
        ),
        (
            "core::vec::truncate",
            FunctionSignature::fixed(vec![mutable(), Type::USIZE], Type::Unit),
        ),
        (
            "core::string::contains",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::starts_with",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::ends_with",
            FunctionSignature::fixed(vec![shared(), Type::String], Type::Bool),
        ),
        (
            "core::string::find",
            FunctionSignature::fixed(
                vec![shared(), Type::String],
                Type::Option(Box::new(Type::USIZE)),
            ),
        ),
        (
            "core::string::trim",
            FunctionSignature::fixed(vec![shared()], Type::String),
        ),
        (
            "core::string::replace",
            FunctionSignature::fixed(vec![shared(), Type::String, Type::String], Type::String),
        ),
        (
            "core::value::expect",
            FunctionSignature::fixed(vec![Type::Unknown, Type::String], Type::Unknown),
        ),
        (
            "core::result::ok",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::result::err",
            FunctionSignature::fixed(vec![Type::Unknown], Type::Option(Box::new(Type::Unknown))),
        ),
        (
            "core::option::take",
            FunctionSignature::fixed(vec![mutable()], Type::Option(Box::new(Type::Unknown))),
        ),
    ]
}

pub(super) fn call_core_import(name: &str, arguments: &[Value]) -> Result<Value, String> {
    match name {
        "type_of" => Ok(Value::String(Rc::from(arguments[0].type_name()))),
        "clone" => match &arguments[0] {
            Value::Reference(reference) => reference.read()?.clone_owned(),
            value => Err(format!(
                "`clone` expects a reference, found {}; use `clone(&value)`",
                value.type_name()
            )),
        },
        "is_ok" => match &arguments[0] {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_ok())),
            value => Err(format!(
                "`is_ok` expects Result, found {}",
                value.type_name()
            )),
        },
        "is_err" => match &arguments[0] {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_err())),
            value => Err(format!(
                "`is_err` expects Result, found {}",
                value.type_name()
            )),
        },
        "is_some" => match &arguments[0] {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_some())),
            value => Err(format!(
                "`is_some` expects Option, found {}",
                value.type_name()
            )),
        },
        "is_none" => match &arguments[0] {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_none())),
            value => Err(format!(
                "`is_none` expects Option, found {}",
                value.type_name()
            )),
        },
        "unwrap" => match &arguments[0] {
            Value::Option {
                value: Some(value), ..
            }
            | Value::Result {
                value: Ok(value), ..
            } => value.clone_owned(),
            Value::Option { value: None, .. } => Err("called `unwrap` on `None`".into()),
            Value::Result {
                value: Err(value), ..
            } => Err(format!("called `unwrap` on Err({value})")),
            value => Err(format!(
                "`unwrap` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        "unwrap_or" => match &arguments[0] {
            Value::Option {
                value,
                element_type,
            } => {
                if let Some(expected) = element_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                value
                    .as_ref()
                    .map_or_else(|| arguments[1].clone_owned(), |value| value.clone_owned())
            }
            Value::Result { value, ok_type, .. } => {
                if let Some(expected) = ok_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                match value {
                    Ok(value) => value.clone_owned(),
                    Err(_) => arguments[1].clone_owned(),
                }
            }
            value => Err(format!(
                "`unwrap_or` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        "core::assert" => match arguments.first() {
            Some(Value::Bool(true)) => Ok(Value::Unit),
            Some(Value::Bool(false)) => Err(arguments
                .get(1)
                .map(ToString::to_string)
                .unwrap_or_else(|| "assertion failed".into())),
            Some(value) => Err(format!(
                "`assert` expects bool, found {}",
                value.type_name()
            )),
            None => Err("`assert` expects at least one argument".into()),
        },
        "core::vec::new" => Ok(Value::Vec(Rc::new(SequenceValue {
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        }))),
        "core::vec::from" => {
            let Value::Array(array) = &arguments[0] else {
                return Err("Vec::from expects an array".into());
            };
            if array
                .elements
                .borrow()
                .iter()
                .any(|slot| slot.references > 0)
            {
                return Err("cannot move an array into Vec while an element is referenced".into());
            }
            let elements = array.elements.borrow_mut().drain(..).collect();
            Ok(Value::Vec(Rc::new(SequenceValue {
                elements: RefCell::new(elements),
                element_type: RefCell::new(array.element_type.borrow().clone()),
            })))
        }
        "core::sequence::len" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("len receiver must be a reference".into());
            };
            let value = reference.read()?;
            let length = match value {
                Value::Array(sequence) | Value::Vec(sequence) => sequence.elements.borrow().len(),
                Value::String(value) => value.len(),
                value => {
                    return Err(format!(
                        "len receiver is not a collection: {}",
                        value.type_name()
                    ));
                }
            };
            Ok(Value::Usize(length))
        }
        "core::value::is_empty" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("is_empty receiver must be a reference".into());
            };
            let value = reference.read()?;
            let empty = match value {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    sequence.elements.borrow().is_empty()
                }
                Value::String(value) => value.is_empty(),
                value => return Err(format!("{} has no is_empty method", value.type_name())),
            };
            Ok(Value::Bool(empty))
        }
        "core::vec::push" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::push requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::push requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("push receiver is not Vec".into());
            };
            let value = &arguments[1];
            if value.contains_reference() {
                return Err("Vec cannot own local references".into());
            }
            let current = sequence
                .elements
                .borrow()
                .first()
                .map(|slot| slot.type_annotation.clone())
                .or_else(|| sequence.element_type.borrow().clone())
                .unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&current, &actual)
                .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
            *sequence.element_type.borrow_mut() = Some(element_type.clone());
            sequence.elements.borrow_mut().push(FieldSlot {
                value: Some(value.clone()),
                type_annotation: element_type,
                references: 0,
            });
            Ok(Value::Unit)
        }
        "core::vec::pop" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::pop requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::pop requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("pop receiver is not Vec".into());
            };
            let element_type = sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let value = {
                let mut elements = sequence.elements.borrow_mut();
                if elements.last().is_some_and(|slot| slot.references > 0) {
                    return Err("cannot pop a referenced Vec element".into());
                }
                elements.pop().and_then(|slot| slot.value).map(Rc::new)
            };
            Ok(Value::Option {
                value,
                element_type: Some(element_type),
            })
        }
        "core::vec::clear" | "core::vec::truncate" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            let length = if name == "core::vec::clear" {
                0
            } else {
                let Value::Usize(length) = arguments[1] else {
                    return Err("Vec::truncate length must be usize".into());
                };
                length
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements
                .get(length..)
                .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
            {
                return Err("cannot remove a referenced Vec element".into());
            }
            elements.truncate(length);
            Ok(Value::Unit)
        }
        name if name.starts_with("core::string::") => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("string method receiver must be a reference".into());
            };
            let Value::String(value) = reference.read()? else {
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
            match name {
                "core::string::contains" => Ok(Value::Bool(value.contains(argument(1)?))),
                "core::string::starts_with" => Ok(Value::Bool(value.starts_with(argument(1)?))),
                "core::string::ends_with" => Ok(Value::Bool(value.ends_with(argument(1)?))),
                "core::string::find" => Ok(Value::Option {
                    value: value
                        .find(argument(1)?)
                        .map(|offset| Rc::new(Value::Usize(offset))),
                    element_type: Some(Type::USIZE),
                }),
                "core::string::trim" => Ok(Value::String(Rc::from(value.trim()))),
                "core::string::replace" => Ok(Value::String(Rc::from(
                    value.replace(argument(1)?, argument(2)?),
                ))),
                _ => Err(format!("unknown string import `{name}`")),
            }
        }
        "core::value::expect" => {
            let Value::String(message) = &arguments[1] else {
                return Err("expect message must be string".into());
            };
            match &arguments[0] {
                Value::Option {
                    value: Some(value), ..
                }
                | Value::Result {
                    value: Ok(value), ..
                } => value.clone_owned(),
                Value::Option { value: None, .. } => Err(message.to_string()),
                Value::Result {
                    value: Err(value), ..
                } => Err(format!("{message}: {value}")),
                value => Err(format!(
                    "expect requires Option or Result, found {}",
                    value.type_name()
                )),
            }
        }
        "core::result::ok" | "core::result::err" => {
            let Value::Result {
                value,
                ok_type,
                error_type,
            } = &arguments[0]
            else {
                return Err("Result conversion receiver is not Result".into());
            };
            let (value, element_type) = match (name, value) {
                ("core::result::ok", Ok(value)) => (Some(value.clone()), ok_type.clone()),
                ("core::result::err", Err(value)) => (Some(value.clone()), error_type.clone()),
                ("core::result::ok", Err(_)) => (None, ok_type.clone()),
                ("core::result::err", Ok(_)) => (None, error_type.clone()),
                _ => unreachable!(),
            };
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        "core::option::take" => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Option::take requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Option::take requires `&mut self`".into());
            }
            let Value::Option {
                value,
                element_type,
            } = reference.read()?
            else {
                return Err("Option::take receiver is not Option".into());
            };
            reference
                .write(Value::Option {
                    value: None,
                    element_type: element_type.clone(),
                })
                .map_err(|error| assign_error(error, Span::default()).message)?;
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        _ => Err(format!("unknown core import `{name}`")),
    }
}
