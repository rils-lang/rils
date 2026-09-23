//! Native methods generated from the shared Rust standard-library definitions.

use std::{collections::VecDeque, rc::Rc};

use crate::{IntegerType, Type, Value};
use rils_stdlib::stdlib::{
    option::Option as NativeOption,
    string::{Iterator, String as NativeString},
};

fn string_input(value: &Value) -> Result<NativeString, String> {
    match super::import_receiver(value)? {
        Value::String(value) => Ok(NativeString::from(value.to_string())),
        value => Err(format!(
            "string method receiver or argument is {}, expected string",
            value.type_name()
        )),
    }
}

fn usize_input(value: &Value) -> Result<usize, String> {
    match value {
        Value::Usize(value) => Ok(*value),
        value => Err(format!(
            "string repeat count must be usize, found {}",
            value.type_name()
        )),
    }
}

trait StringOutput {
    fn into_value(self) -> Result<Value, String>;
}

impl StringOutput for bool {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::Bool(self))
    }
}
impl StringOutput for usize {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::Usize(self))
    }
}
impl StringOutput for NativeString {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::String(Rc::from(std::string::String::from(self))))
    }
}
impl StringOutput for NativeOption<usize> {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::Option {
            value: match self {
                NativeOption::Some(value) => Some(Rc::new(Value::Usize(value))),
                NativeOption::None => None,
            },
            element_type: Some(Type::USIZE),
        })
    }
}
impl StringOutput for NativeOption<NativeString> {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::Option {
            value: match self {
                NativeOption::Some(value) => Some(Rc::new(Value::String(Rc::from(
                    std::string::String::from(value),
                )))),
                NativeOption::None => None,
            },
            element_type: Some(Type::String),
        })
    }
}
impl StringOutput for Iterator<char> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0.into_iter().map(Value::Char).collect::<VecDeque<_>>(),
            Type::Char,
        ))
    }
}
impl StringOutput for Iterator<u8> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0.into_iter().map(Value::U8).collect::<VecDeque<_>>(),
            Type::Integer(IntegerType::U8),
        ))
    }
}
impl StringOutput for Iterator<NativeString> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0
                .into_iter()
                .map(|value| Value::String(Rc::from(std::string::String::from(value))))
                .collect::<VecDeque<_>>(),
            Type::String,
        ))
    }
}

mod option {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::option_definition!(decl_rils_native);
}

mod result {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::result_definition!(decl_rils_native);
}

mod string {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::string_definition!(decl_rils_native);
}

pub fn call(
    id: rils_builtins::BuiltinId,
    arguments: &[crate::Value],
) -> Option<Result<crate::Value, String>> {
    option::call(id, arguments)
        .or_else(|| result::call(id, arguments))
        .or_else(|| string::call(id, arguments))
}
