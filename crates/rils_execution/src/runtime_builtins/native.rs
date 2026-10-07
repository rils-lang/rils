//! Native methods generated from the shared Rust standard-library definitions.

use std::{collections::HashSet, rc::Rc, sync::OnceLock};

use crate::{IntegerType, Type, Value, value::OwnedIteratorValue};
use rils_stdlib::stdlib::{
    iterator::Iter, option::Option as NativeOption, string::String as NativeString,
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
impl StringOutput for Iter<char> {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::OwnedIterator(Rc::new(
            OwnedIteratorValue::from_generator(self.map(crate::value::native_char), Type::Char),
        )))
    }
}
impl StringOutput for Iter<u8> {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::OwnedIterator(Rc::new(
            OwnedIteratorValue::from_generator(
                self.map(Value::from_u8),
                Type::Integer(IntegerType::U8),
            ),
        )))
    }
}
impl StringOutput for Iter<NativeString> {
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::OwnedIterator(Rc::new(
            OwnedIteratorValue::from_generator(
                self.map(|value| crate::value::native_string(std::string::String::from(value))),
                Type::String,
            ),
        )))
    }
}

mod option {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::option_definition!(decl_rils_native);
}

mod boxed {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::box_definition!(decl_rils_native);
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

mod hash_set {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::hashset_definition!(decl_rils_native);
}

mod hash_map {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::hashmap_definition!(decl_rils_native);
}

mod vec_deque {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::vecdeque_definition!(decl_rils_native);
}

mod binary_heap {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::binaryheap_definition!(decl_rils_native);
}

mod btree_set {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::btreeset_definition!(decl_rils_native);
}

mod btree_map {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::btreemap_definition!(decl_rils_native);
}

mod rc {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::rc_definition!(decl_rils_native);
}

mod weak {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::weak_definition!(decl_rils_native);
}

mod cell {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::cell_definition!(decl_rils_native);
}

mod ref_cell {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::refcell_definition!(decl_rils_native);
}

pub fn call_symbol(
    symbol: &str,
    arguments: &[crate::Value],
) -> Option<Result<crate::Value, String>> {
    hash_set::call_symbol(symbol, arguments)
        .or_else(|| hash_map::call_symbol(symbol, arguments))
        .or_else(|| vec_deque::call_symbol(symbol, arguments))
        .or_else(|| binary_heap::call_symbol(symbol, arguments))
        .or_else(|| btree_set::call_symbol(symbol, arguments))
        .or_else(|| btree_map::call_symbol(symbol, arguments))
        .or_else(|| rc::call_symbol(symbol, arguments))
        .or_else(|| weak::call_symbol(symbol, arguments))
        .or_else(|| cell::call_symbol(symbol, arguments))
        .or_else(|| ref_cell::call_symbol(symbol, arguments))
        .or_else(|| boxed::call_symbol(symbol, arguments))
        .or_else(|| option::call_symbol(symbol, arguments))
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
        .or_else(|| iterator_next_symbol(symbol, arguments))
        .or_else(|| formatter_symbol(symbol, arguments))
}

fn formatter_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let (owner, member) = rils_builtins::native_member_owner(symbol)?;
    let formatter = rils_builtins::builtin("Formatter")?;
    if !std::ptr::eq(owner, formatter) {
        return None;
    }
    Some((|| {
        let [receiver, value] = arguments else {
            return Err(format!(
                "Formatter::{} expects a receiver and one argument",
                member.name
            ));
        };
        match member.name {
            "write_str" => {
                let value = value
                    .as_string()
                    .ok_or("Formatter::write_str expects string")?;
                crate::formatting::buffer_from_value(receiver)?.write_str(&value);
                Ok(Value::Result {
                    value: Ok(std::rc::Rc::new(Value::Unit)),
                    ok_type: Some(crate::Type::Unit),
                    error_type: Some(crate::Type::named("FormatError")),
                })
            }
            "write_derived_debug" => {
                Err("Formatter::write_derived_debug requires a formatting context".into())
            }
            _ => Err("unsupported Formatter operation".into()),
        }
    })())
}

fn iterator_next_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    static NEXT_SYMBOLS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    let symbols = NEXT_SYMBOLS.get_or_init(|| {
        rils_builtins::BUILTINS
            .iter()
            .flat_map(|declaration| declaration.members)
            .filter(|member| member.name == "next")
            .filter_map(|member| member.native_symbol)
            .collect()
    });
    if !symbols.contains(symbol) {
        return None;
    }
    if arguments.len() != 1 {
        return Some(Err(format!(
            "iterator method expects one receiver, found {} arguments",
            arguments.len()
        )));
    }
    let Value::Reference(reference) = &arguments[0] else {
        return Some(Err("Iterator::next requires a mutable binding".into()));
    };
    match reference.read() {
        Ok(
            Value::OwnedIterator(_)
            | Value::BorrowedIndexedIterator(_)
            | Value::BorrowedMapIterator(_)
            | Value::BorrowedSetIterator(_),
        ) => Some(super::indexed_iter::next(arguments)),
        Ok(value) if crate::value::native_ops::is_iterator(&value) => {
            Some(super::range::next(arguments))
        }
        Err(message) => Some(Err(message)),
        _ => Some(Err("next receiver is not an iterator".into())),
    }
}

pub fn call_owned_symbol(
    symbol: &str,
    arguments: Vec<crate::Value>,
    context: &super::NativeOwnedContext,
) -> Option<Result<crate::Value, String>> {
    if super::requires_native_callback(symbol) {
        Some(Err("native callback context is unavailable".into()))
    } else if option::is_owned_symbol(symbol) {
        option::call_owned_symbol(symbol, arguments, context)
    } else if result::is_owned_symbol(symbol) {
        result::call_owned_symbol(symbol, arguments, context)
    } else if super::cell_native::is_owned_symbol(symbol) {
        super::cell_native::call_owned_symbol(symbol, arguments, context)
    } else if super::native_set::is_owned_symbol(symbol) {
        super::native_set::call_owned_symbol(symbol, arguments, context)
    } else if super::native_map::is_owned_symbol(symbol) {
        super::native_map::call_owned_symbol(symbol, arguments, context)
    } else if super::rc_native::is_owned_symbol(symbol) {
        super::rc_native::call_owned_symbol(symbol, arguments, context)
    } else if boxed::is_owned_symbol(symbol) {
        boxed::call_owned_symbol(symbol, arguments, context)
    } else if super::vec_deque::is_owned_symbol(symbol) {
        super::vec_deque::call_owned_symbol(symbol, arguments, context)
    } else if super::binary_heap::is_owned_symbol(symbol) {
        super::binary_heap::call_owned_symbol(symbol, arguments, context)
    } else {
        super::vector_dynamic::call_owned_symbol(symbol, arguments, context)
    }
}

pub fn is_owned_symbol(symbol: &str) -> bool {
    option::is_owned_symbol(symbol)
        || result::is_owned_symbol(symbol)
        || super::cell_native::is_owned_symbol(symbol)
        || super::native_set::is_owned_symbol(symbol)
        || super::native_map::is_owned_symbol(symbol)
        || super::rc_native::is_owned_symbol(symbol)
        || boxed::is_owned_symbol(symbol)
        || super::vec_deque::is_owned_symbol(symbol)
        || super::binary_heap::is_owned_symbol(symbol)
        || super::vector_dynamic::is_owned_symbol(symbol)
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
    arguments: Vec<crate::Value>,
    context: &super::NativeOwnedContext,
    expected: Option<&crate::Type>,
    callback: &mut super::NativeCallback<'_, E>,
) -> Option<Result<crate::Value, super::NativeCallError<E>>> {
    if option::is_callback_symbol(symbol) {
        option::call_callback_symbol(symbol, arguments, context, expected, callback)
    } else if result::is_callback_symbol(symbol) {
        result::call_callback_symbol(symbol, arguments, context, expected, callback)
    } else if callable_functions::apply_twice::is_callback_symbol(symbol) {
        callable_functions::apply_twice::call_callback_symbol(symbol, arguments, callback)
    } else if callable_functions::combine::is_callback_symbol(symbol) {
        callable_functions::combine::call_callback_symbol(symbol, arguments, callback)
    } else if callable_functions::chain::is_callback_symbol(symbol) {
        callable_functions::chain::call_callback_symbol(symbol, arguments, callback)
    } else {
        None
    }
}
