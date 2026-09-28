//! Native methods generated from the shared Rust standard-library definitions.

use std::collections::VecDeque;

use crate::{IntegerType, Type, Value};
use rils_stdlib::stdlib::{
    option::Option as NativeOption,
    string::{Iterator, String as NativeString},
};

pub(crate) fn string_input(value: &Value) -> Result<NativeString, String> {
    let value = super::import_receiver(value)?;
    crate::value::string_payload(&value)
        .map(NativeString::from)
        .ok_or_else(|| {
            format!(
                "string method receiver or argument is {}, expected string",
                value.type_name()
            )
        })
}

pub(crate) fn usize_input(value: &Value) -> Result<usize, String> {
    value.as_usize().ok_or_else(|| {
        format!(
            "string repeat count must be usize, found {}",
            value.type_name()
        )
    })
}

pub(crate) trait StringOutput {
    fn into_value(self) -> Result<Value, String>;
}

impl StringOutput for bool {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::Bool(self))
    }
}
impl StringOutput for usize {
    fn into_value(self) -> Result<Value, String> {
        Ok(crate::numeric::native_usize(self))
    }
}
impl StringOutput for NativeString {
    fn into_value(self) -> Result<Value, String> {
        Ok(crate::value::native_string(std::string::String::from(self)))
    }
}
impl StringOutput for NativeOption<usize> {
    fn into_value(self) -> Result<Value, String> {
        let item = match self {
            NativeOption::Some(value) => Some(crate::numeric::native_usize(value)),
            NativeOption::None => None,
        };
        match crate::value::dynamic_option::construct(item, &Type::USIZE)? {
            crate::value::dynamic_option::Construction::Native(value) => Ok(value),
            crate::value::dynamic_option::Construction::Unsupported(_) => {
                Err("no native Option<usize> layout".into())
            }
        }
    }
}
impl StringOutput for NativeOption<NativeString> {
    fn into_value(self) -> Result<Value, String> {
        let item = match self {
            NativeOption::Some(value) => Some(crate::value::native_string(
                std::string::String::from(value),
            )),
            NativeOption::None => None,
        };
        match crate::value::dynamic_option::construct(item, &Type::String)? {
            crate::value::dynamic_option::Construction::Native(value) => Ok(value),
            crate::value::dynamic_option::Construction::Unsupported(_) => {
                Err("no native Option<string> layout".into())
            }
        }
    }
}
impl StringOutput for Iterator<char> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0
                .into_iter()
                .map(crate::value::native_char)
                .collect::<VecDeque<_>>(),
            Type::Char,
        ))
    }
}
impl StringOutput for Iterator<u8> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0
                .into_iter()
                .map(Value::from_u8)
                .collect::<VecDeque<_>>(),
            Type::Integer(IntegerType::U8),
        ))
    }
}
impl StringOutput for Iterator<NativeString> {
    fn into_value(self) -> Result<Value, String> {
        Ok(super::owned_iterator_value(
            self.0
                .into_iter()
                .map(|value| crate::value::native_string(std::string::String::from(value)))
                .collect::<VecDeque<_>>(),
            Type::String,
        ))
    }
}

mod option {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::option_definition!(decl_rils_native);
}

mod result {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::result_definition!(decl_rils_native);
}

mod string {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::string_definition!(decl_rils_native);
}

mod vector {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::vec_definition!(decl_rils_native);
}

mod range {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::range_definition!(decl_rils_native);
}

mod indexed_iterator {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::iter_definition!(decl_rils_native);
}

pub fn call(
    id: rils_builtins::BuiltinId,
    arguments: &[crate::Value],
) -> Option<Result<crate::Value, String>> {
    id.canonical_path()
        .and_then(|symbol| call_symbol(symbol, arguments))
}

pub fn call_symbol(
    symbol: &str,
    arguments: &[crate::Value],
) -> Option<Result<crate::Value, String>> {
    option::call_symbol(symbol, arguments)
        .or_else(|| result::call_symbol(symbol, arguments))
        .or_else(|| {
            let receiver = super::import_receiver(arguments.first()?).ok()?;
            let Value::Native(object) = receiver else {
                return None;
            };
            object.call(symbol, &arguments[1..])
        })
        .or_else(|| string::call_symbol(symbol, arguments))
        .or_else(|| super::vector_dynamic::call_symbol(symbol, arguments))
        .or_else(|| vector::call_symbol(symbol, arguments))
        .or_else(|| range::call_symbol(symbol, arguments))
        .or_else(|| indexed_iterator::call_symbol(symbol, arguments))
}

mod callable_functions {
    use rils_stdlib_macros::decl_rils_function_native;

    pub(super) mod apply_twice {
        use super::decl_rils_function_native;
        rils_stdlib::core_ops_apply_twice_definition!(decl_rils_function_native);
    }
    pub(super) mod combine {
        use super::decl_rils_function_native;
        rils_stdlib::core_ops_combine_definition!(decl_rils_function_native);
    }
    pub(super) mod chain {
        use super::decl_rils_function_native;
        rils_stdlib::core_ops_chain_definition!(decl_rils_function_native);
    }
}

pub fn call_callback_symbol<E>(
    symbol: &str,
    arguments: &[crate::Value],
    callback: &mut super::NativeCallback<'_, E>,
) -> Option<Result<crate::Value, super::NativeCallError<E>>> {
    option::call_callback_symbol(symbol, arguments, callback)
        .or_else(|| result::call_callback_symbol(symbol, arguments, callback))
        .or_else(|| {
            callable_functions::apply_twice::call_callback_symbol(symbol, arguments, callback)
        })
        .or_else(|| callable_functions::combine::call_callback_symbol(symbol, arguments, callback))
        .or_else(|| callable_functions::chain::call_callback_symbol(symbol, arguments, callback))
}
